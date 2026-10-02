//! Firmware for hardware that arrives after installation.
//!
//! The installer puts in the firmware that the drivers loaded while it runs ask
//! for. A dock, a USB network adapter or a Wi-Fi stick plugged in later loads a
//! driver that may ask for a file nobody installed. The kernel logs `Direct
//! firmware load for FILE failed with error -2`, the device works without it
//! or not at all, and nothing else says so.
//!
//! `alpymist firmware watch`, which the alpymist-firmware service runs as root,
//! reads the kernel log from the start of the boot and then follows it. For
//! each such line it installs the package holding the file, from the system's
//! own repositories and by the installer's rule
//! ([`alpymist_core::firmware::package_for`]). A device on USB is started again
//! so its driver picks the file up; anything else, a built-in GPU say, has it
//! from the next boot. What it did goes to its log and, as a notification, to
//! every desktop session. With no network it tries again every few minutes.
//! `alpymist firmware check` does one pass over the log so far and stops.

use alpymist_core::firmware::package_for;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::OpenOptionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The kernel log, one record per read, from the start of the boot.
const KMSG: &str = "/dev/kmsg";
/// Where the kernel looks for firmware, in its order.
const FIRMWARE_DIRS: [&str; 2] = ["/lib/firmware/updates", "/lib/firmware"];
/// A dock brings up several drivers at once; gather them into one install.
const SETTLE: Duration = Duration::from_secs(3);
/// How soon to try again when the packages could not be fetched.
const RETRY: Duration = Duration::from_mins(5);
/// The compositors whose sessions are told what was installed.
const COMPOSITORS: [&str; 1] = ["Hyprland"];
/// `O_NONBLOCK` on Linux, for reading the log so far without waiting for more.
const O_NONBLOCK: i32 = 0o4000;

/// A driver that asked for a firmware file and did not get it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Missing {
    /// The driver, as the kernel names it: `r8152`.
    pub driver: String,
    /// The device it was for: `6-2.1.2:1.0`.
    pub device: String,
    /// The file, relative to the firmware directory: `rtl_nic/rtl8153b-2.fw`.
    pub file: String,
}

/// The request in one `/dev/kmsg` record, if it is a firmware file that was
/// not there.
///
/// A record is `PRIORITY,SEQUENCE,TIME,FLAGS;MESSAGE`, and a driver's message
/// starts with its name and its device's. Error `-2` is "no such file"; other
/// errors are a driver's problem, not a missing package.
#[must_use]
pub fn missing_in(record: &str) -> Option<Missing> {
    let (_, message) = record.split_once(';')?;
    let (prefix, rest) = message.split_once(": Direct firmware load for ")?;
    let (file, error) = rest.rsplit_once(" failed with error ")?;
    if error.trim() != "-2" {
        return None;
    }
    let (driver, device) = prefix.split_once(' ')?;
    Some(Missing {
        driver: driver.to_string(),
        device: device.to_string(),
        file: file.to_string(),
    })
}

/// Whether the kernel would now find `file`, as it is or compressed.
fn present(file: &str) -> bool {
    FIRMWARE_DIRS.iter().any(|dir| {
        ["", ".zst", ".xz"]
            .iter()
            .any(|ext| Path::new(dir).join(format!("{file}{ext}")).exists())
    })
}

/// Say what happened, on stderr, which the service sends to syslog.
fn say(message: &str) {
    eprintln!("{message}");
}

/// Whether the package is installed.
fn installed(package: &str) -> bool {
    Command::new("apk")
        .args(["info", "-e", package])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Which of `packages` the repositories have. Names a driver mentions but no
/// package has would fail the whole `apk add`, so they are asked about first.
fn available(packages: &[String]) -> Result<Vec<String>, String> {
    let output = Command::new("apk")
        .args(["search", "--quiet", "--exact", "--update-cache"])
        .args(packages)
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("could not run apk: {e}"))?;
    let listed = String::from_utf8_lossy(&output.stdout);
    let names: Vec<&str> = listed.split_whitespace().collect();
    Ok(packages
        .iter()
        .filter(|p| names.contains(&p.as_str()))
        .cloned()
        .collect())
}

/// Start a USB device's driver again, so that it asks for its firmware again.
/// Only USB: a device that can be unplugged can be restarted without taking
/// the machine with it.
fn restart(device: &str) -> bool {
    let plain = !device.is_empty()
        && device
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_'));
    if !plain {
        return false;
    }
    let Ok(driver) = std::fs::canonicalize(
        Path::new("/sys/bus/usb/devices")
            .join(device)
            .join("driver"),
    ) else {
        return false;
    };
    std::fs::write(driver.join("unbind"), device).is_ok()
        && std::fs::write(driver.join("bind"), device).is_ok()
}

/// A string in `GVariant`'s text format, which `gdbus call` reads its
/// arguments in.
fn gvariant_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A desktop session: its account's user and group, and the variables that
/// reach its session bus.
struct Session {
    uid: u32,
    gid: u32,
    env: Vec<(String, String)>,
}

/// The desktop sessions running now, one per compositor.
fn sessions() -> Vec<Session> {
    let mut found: Vec<Session> = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let proc = entry.path();
        let is_compositor = std::fs::read_to_string(proc.join("comm"))
            .is_ok_and(|comm| COMPOSITORS.contains(&comm.trim()));
        if !is_compositor {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(proc.join("status")) else {
            continue;
        };
        let id = |field: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(field))
                .and_then(|v| v.split_whitespace().next()?.parse::<u32>().ok())
        };
        let (Some(uid), Some(gid)) = (id("Uid:"), id("Gid:")) else {
            continue;
        };
        let Ok(environ) = std::fs::read(proc.join("environ")) else {
            continue;
        };
        let env: Vec<(String, String)> = environ
            .split(|b| *b == 0)
            .filter_map(|var| std::str::from_utf8(var).ok()?.split_once('='))
            .filter(|(k, _)| matches!(*k, "DBUS_SESSION_BUS_ADDRESS" | "XDG_RUNTIME_DIR"))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let has_bus = env.iter().any(|(k, _)| k == "DBUS_SESSION_BUS_ADDRESS");
        if has_bus && !found.iter().any(|s| s.uid == uid && s.env == env) {
            found.push(Session { uid, gid, env });
        }
    }
    found
}

/// Put a notification in front of everyone with a desktop open, as them.
fn tell_sessions(summary: &str, body: &str) {
    for Session { uid, gid, env } in sessions() {
        let _ = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--timeout",
                "5",
                "--dest",
                "org.freedesktop.Notifications",
                "--object-path",
                "/org/freedesktop/Notifications",
                "--method",
                "org.freedesktop.Notifications.Notify",
                // Whatever follows is the method's arguments, not options.
                "--",
                "'Alpymist'",
                "0",
                "''",
            ])
            .arg(gvariant_string(summary))
            .arg(gvariant_string(body))
            // No hints, and an expiry of 0: it stays until it is clicked
            // away. This happens while nobody may be looking, and the
            // desktop's usual few seconds would pass unseen.
            .args(["[]", "{}", "0"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .envs(env)
            .uid(uid)
            .gid(gid)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Install what `pending` needs. Returns `false` when it should be tried
/// again later: the index or the packages could not be fetched.
fn provide(pending: &mut BTreeSet<Missing>) -> bool {
    // Found since, installed by hand, or not a path a package could hold.
    pending.retain(|m| !present(&m.file) && package_for(&m.file).is_some());
    let wanted: BTreeSet<String> = pending
        .iter()
        .filter_map(|m| package_for(&m.file))
        .collect();
    // Drivers name more files than any package ships; where the package is
    // in and the file still is not, there is nothing to install.
    let wanted: Vec<String> = wanted.into_iter().filter(|p| !installed(p)).collect();
    pending.retain(|m| package_for(&m.file).is_some_and(|p| wanted.contains(&p)));
    if pending.is_empty() {
        return true;
    }

    let available = match available(&wanted) {
        Ok(found) => found,
        Err(e) => {
            say(&format!("{e}; trying again later"));
            return false;
        }
    };
    for package in wanted.iter().filter(|p| !available.contains(p)) {
        for m in pending
            .iter()
            .filter(|m| package_for(&m.file).as_ref() == Some(package))
        {
            say(&format!(
                "{} {} asked for {}, and no package has it ({package} does not exist)",
                m.driver, m.device, m.file
            ));
        }
    }
    pending.retain(|m| package_for(&m.file).is_some_and(|p| available.contains(&p)));
    if available.is_empty() {
        return true;
    }

    let added = Command::new("apk")
        .args(["add", "--quiet", "--no-progress"])
        .args(&available)
        .status();
    if !added.is_ok_and(|s| s.success()) {
        say(&format!(
            "could not install {}; trying again later",
            available.join(" ")
        ));
        return false;
    }

    let mut restarted = Vec::new();
    let mut later = Vec::new();
    for m in std::mem::take(pending) {
        let who = format!("{} {}", m.driver, m.device);
        if restart(&m.device) {
            if !restarted.contains(&who) {
                restarted.push(who);
            }
        } else if !later.contains(&who) {
            later.push(who);
        }
    }
    let mut body = format!("Installed {}.", available.join(", "));
    if !restarted.is_empty() {
        let _ = write!(body, " Started again: {}.", restarted.join(", "));
    }
    if !later.is_empty() {
        let _ = write!(body, " Used from the next restart: {}.", later.join(", "));
    }
    say(&body);
    tell_sessions("Firmware installed", &body);
    true
}

/// Refuse to run as anyone but root: the log, apk and the drivers all need it.
fn as_root() -> Result<(), Box<dyn Error>> {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let root = status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|v| v.split_whitespace().nth(1))
        == Some("0");
    if root {
        Ok(())
    } else {
        Err("firmware needs root: it reads the kernel log and installs packages".into())
    }
}

/// Every missing-firmware record in `reader`, until it ends.
fn read_all(reader: impl BufRead, mut each: impl FnMut(Missing) -> bool) {
    let mut reader = reader;
    let mut record = Vec::new();
    loop {
        record.clear();
        match reader.read_until(b'\n', &mut record) {
            Ok(0) => return,
            Ok(_) => {
                if let Some(m) = missing_in(&String::from_utf8_lossy(&record))
                    && !each(m)
                {
                    return;
                }
            }
            // EPIPE: records were overwritten before they were read. The next
            // read carries on from the oldest one left.
            Err(e) if e.raw_os_error() == Some(32) => {}
            Err(_) => return,
        }
    }
}

/// What drivers have asked for and not found since the boot, for `alpymist
/// report`: `None` where the kernel log may not be read, which on a system
/// that restricts it is anyone but root.
#[must_use]
pub fn missing_so_far() -> Option<BTreeSet<Missing>> {
    let kmsg = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(KMSG)
        .ok()?;
    let mut missing = BTreeSet::new();
    read_all(BufReader::new(kmsg), |m| {
        missing.insert(m);
        true
    });
    Some(missing)
}

/// One pass over the kernel log so far.
///
/// # Errors
/// Not root, or the log could not be read.
pub fn check() -> Result<(), Box<dyn Error>> {
    as_root()?;
    let kmsg = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(KMSG)?;
    let mut pending = BTreeSet::new();
    read_all(BufReader::new(kmsg), |m| {
        pending.insert(m);
        true
    });
    if !provide(&mut pending) {
        return Err("some firmware could not be installed yet".into());
    }
    Ok(())
}

/// Follow the kernel log for as long as the machine runs.
///
/// # Errors
/// Not root, or the log could not be read.
pub fn watch() -> Result<(), Box<dyn Error>> {
    as_root()?;
    let kmsg = File::open(KMSG)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || read_all(BufReader::new(kmsg), |m| tx.send(m).is_ok()));

    let mut pending = BTreeSet::new();
    let mut next_try: Option<Instant> = None;
    loop {
        let wait = next_try.map_or(Duration::from_hours(1), |t| {
            t.saturating_duration_since(Instant::now())
        });
        match rx.recv_timeout(wait) {
            Ok(m) => {
                pending.insert(m);
                while let Ok(m) = rx.recv_timeout(SETTLE) {
                    pending.insert(m);
                }
                next_try = Some(Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("the kernel log stopped".into());
            }
        }
        if next_try.is_some_and(|t| Instant::now() >= t) {
            next_try = (!provide(&mut pending)).then(|| Instant::now() + RETRY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Missing, gvariant_string, missing_in};

    #[test]
    fn a_failed_firmware_load_is_read_from_its_record() {
        let record = "4,1234,172063183,-;r8152 6-2.1.2:1.0: Direct firmware load for \
                      rtl_nic/rtl8153b-2.fw failed with error -2\n";
        assert_eq!(
            missing_in(record),
            Some(Missing {
                driver: "r8152".into(),
                device: "6-2.1.2:1.0".into(),
                file: "rtl_nic/rtl8153b-2.fw".into(),
            })
        );
    }

    #[test]
    fn a_file_name_with_spaces_survives() {
        let record = "4,9,9,-;brcmfmac mmc1:0001:1: Direct firmware load for \
                      brcm/brcmfmac43455-sdio.Raspberry Pi Foundation-Raspberry Pi 4 Model B.txt \
                      failed with error -2";
        assert_eq!(
            missing_in(record).map(|m| m.file).as_deref(),
            Some("brcm/brcmfmac43455-sdio.Raspberry Pi Foundation-Raspberry Pi 4 Model B.txt")
        );
    }

    #[test]
    fn only_a_missing_file_counts() {
        assert_eq!(
            missing_in(
                "4,1,1,-;iwlwifi 0000:02:00.0: Direct firmware load for x.ucode failed with error -110"
            ),
            None,
            "a timeout is the driver's problem, not a missing package"
        );
        assert_eq!(missing_in(" SUBSYSTEM=usb"), None, "continuation lines");
        assert_eq!(
            missing_in("6,2,2,-;usb 1-1: new high-speed USB device"),
            None
        );
    }

    #[test]
    fn notification_text_is_quoted_for_gdbus() {
        assert_eq!(gvariant_string("it's"), r"'it\'s'");
        assert_eq!(gvariant_string(r"a\b"), r"'a\\b'");
    }
}

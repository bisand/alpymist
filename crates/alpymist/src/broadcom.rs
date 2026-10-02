//! `alpymist firmware broadcom`: the firmware Broadcom's older Wi-Fi cards
//! need and nobody may ship.
//!
//! The `b43` driver has the cards of a great many laptops made before 2012,
//! every Mac of those years among them, and does nothing without firmware
//! that Broadcom never let anyone pass on. No distribution packages it, and
//! Alpymist does not either (ADR 0021). What is published is Broadcom's own
//! driver for routers, which has the firmware in it, and `b43-fwcutter`,
//! which cuts it out. So:
//!
//! - `alpymist firmware broadcom`, as root on a machine with a network — by
//!   cable, or a phone's — downloads that driver, checks it is the file this
//!   was written against, cuts the firmware into `/lib/firmware` and starts
//!   the driver again.
//! - `alpymist firmware broadcom --to DIR`, on any machine with a network,
//!   puts it in `DIR/alpymist-firmware` instead: on a stick, for the machine
//!   that has none. The installer takes it from the stick it installs from.
//! - `alpymist firmware broadcom --from DIR`, as root on the machine without,
//!   takes it from there.
//!
//! Nothing runs this for anyone. It is one file from one place, held to a
//! checksum, and what it leaves is firmware only that driver reads.

use alpymist_core::firmware::{CARRIED, take_carried};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Broadcom's driver, as `OpenWrt` keeps it for building its own images. The
/// version the `b43` developers name for kernels since 3.2.
const ARCHIVE: &str = "broadcom-wl-5.100.138.tar.bz2";
/// Where it is, tried in order.
const MIRRORS: [&str; 3] = [
    "https://sources.openwrt.org",
    "https://downloads.openwrt.org/sources",
    "https://mirror2.openwrt.org/sources",
];
/// What it must be. A file that is not this is not cut and not kept.
const SHA256: &str = "f1e7067aac5b62b67b8b6e4c517990277804339ac16065eb13c731ff909ae46f";
/// The object in it that holds the firmware.
const OBJECT: &str = "broadcom-wl-5.100.138/linux/wl_apsta.o";
/// The tool that cuts it out, and Alpine's package of it.
const CUTTER: &str = "b43-fwcutter";
/// Where the kernel looks.
const FIRMWARE: &str = "/lib/firmware";
/// The driver that reads it.
const DRIVER: &str = "b43";

/// Whether this is root.
fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status").is_ok_and(|status| {
        status
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .and_then(|v| v.split_whitespace().nth(1))
            == Some("0")
    })
}

/// Run a program that says what it does, and fail with `what` if it fails.
fn run(program: &str, args: &[&str], what: &str) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(what.to_string())
    }
}

/// Whether `program` can be run.
fn has(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// The checksum in what `sha256sum` prints for one file.
fn checksum_in(output: &str) -> Option<&str> {
    output
        .split_whitespace()
        .next()
        .filter(|sum| sum.len() == 64)
}

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("alpymist-firmware-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Download the driver, check it, and cut its firmware into `into`, which
/// gains a `b43` directory.
fn fetch(into: &Path) -> Result<(), String> {
    if !has(CUTTER) {
        if !is_root() {
            return Err(format!(
                "{CUTTER} is not installed, and installing it needs root: \
                 doas apk add {CUTTER}, then run this again"
            ));
        }
        println!("Installing {CUTTER}, which cuts the firmware out.");
        run(
            "apk",
            &["add", "--no-progress", CUTTER],
            "could not install b43-fwcutter: is there a network?",
        )?;
    }

    let scratch = Scratch::new()?;
    let archive = scratch.0.join(ARCHIVE);
    let archive_text = archive.to_string_lossy().into_owned();
    let mut fetched = false;
    for mirror in MIRRORS {
        let url = format!("{mirror}/{ARCHIVE}");
        println!("Downloading {url}");
        // busybox's, which every system has; it checks the certificate.
        if run("wget", &["-q", "-O", &archive_text, &url], "").is_ok() {
            fetched = true;
            break;
        }
    }
    if !fetched {
        return Err(
            "could not download Broadcom's driver from any mirror: is there a network?".into(),
        );
    }

    let sum = Command::new("sha256sum")
        .arg(&archive)
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("sha256sum: {e}"))?;
    let sum = String::from_utf8_lossy(&sum.stdout).into_owned();
    if checksum_in(&sum) != Some(SHA256) {
        return Err(format!(
            "what was downloaded is not the file this was written against \
             (sha256 {}, not {SHA256}); nothing was installed",
            checksum_in(&sum).unwrap_or("unreadable")
        ));
    }

    let scratch_text = scratch.0.to_string_lossy().into_owned();
    run(
        "tar",
        &["-xjf", &archive_text, "-C", &scratch_text, OBJECT],
        "could not unpack Broadcom's driver",
    )?;
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let object = scratch.0.join(OBJECT);
    let cut = Command::new(CUTTER)
        .arg("-w")
        .arg(into)
        .arg(&object)
        // It names every file it writes, a hundred and more.
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("{CUTTER}: {e}"))?;
    if !cut.success() {
        return Err(format!("{CUTTER} could not cut the firmware out"));
    }
    Ok(())
}

/// Start the driver again, so it finds what it could not. Whatever comes of
/// it: a machine with no such card has no such driver loaded.
fn restart_driver() {
    for args in [["-r", DRIVER], ["-q", DRIVER]] {
        let _ = Command::new("modprobe")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// `alpymist firmware broadcom`.
///
/// # Errors
/// Not root where root is needed, no network, a download that is not the
/// file expected, or nothing carried where `from` says.
pub fn run_command(to: Option<&Path>, from: Option<&Path>) -> Result<(), Box<dyn Error>> {
    if let Some(dir) = to {
        let into = dir.join(CARRIED);
        fetch(&into)?;
        println!(
            "Broadcom's Wi-Fi firmware is in {}.\n\
             The installer takes it from the stick it installs from. On a machine \
             already installed:\n    doas alpymist firmware broadcom --from {}",
            into.display(),
            dir.display()
        );
        return Ok(());
    }
    if !is_root() {
        return Err("this puts files in /lib/firmware, which needs root: run it with doas".into());
    }
    if let Some(dir) = from {
        // The stick's top, or the directory itself.
        let carried = [dir.join(CARRIED), dir.to_path_buf()]
            .into_iter()
            .find(|d| d.join(DRIVER).is_dir())
            .ok_or_else(|| format!("no {CARRIED}/{DRIVER} in {}", dir.display()))?;
        let copied = take_carried(&carried, Path::new(FIRMWARE))?;
        if copied == 0 {
            return Err(format!("no firmware files in {}", carried.display()).into());
        }
        println!("Copied {copied} firmware files to {FIRMWARE}/{DRIVER}.");
    } else {
        fetch(Path::new(FIRMWARE))?;
        println!("Broadcom's Wi-Fi firmware is in {FIRMWARE}/{DRIVER}.");
    }
    restart_driver();
    println!("The driver was started again: the card should show its networks in a moment.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ARCHIVE, MIRRORS, OBJECT, SHA256, checksum_in};

    #[test]
    fn the_checksum_is_the_first_word_of_what_sha256sum_prints() {
        let line = format!("{SHA256}  /tmp/x/{ARCHIVE}\n");
        assert_eq!(checksum_in(&line), Some(SHA256));
        assert_eq!(checksum_in("sha256sum: no such file\n"), None);
        assert_eq!(checksum_in(""), None);
    }

    /// One file, from places that serve it over TLS, held to one checksum.
    #[test]
    fn the_driver_is_one_pinned_file_fetched_over_tls() {
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(MIRRORS.iter().all(|m| m.starts_with("https://")));
        assert!(OBJECT.starts_with(ARCHIVE.trim_end_matches(".tar.bz2")));
    }
}

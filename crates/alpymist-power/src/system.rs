//! What only root may do: the helper's side.
//!
//! `alpymist-power-helper` is run through pkexec and does one of a closed set of
//! things, with arguments checked here, against fixed paths under `/sys`:
//! set a power mode, set a charge limit, suspend, hibernate, shut down, and
//! put back at boot the mode last chosen.

use crate::profile::{Knobs, Profile};
use std::path::{Path, PathBuf};

/// Where the last mode chosen is kept, for boot.
pub const STATE: &str = "/var/lib/alpymist/power-profile";

/// Where the last charge limit chosen is kept: many firmwares forget it at
/// every boot.
pub const LIMIT_STATE: &str = "/var/lib/alpymist/charge-limit";

/// Charge limits offered, in percent; 100 is no limit.
pub const CHARGE_LIMITS: [u8; 4] = [100, 90, 80, 60];

/// A helper verb, checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Put the machine in a power mode, and remember it.
    Profile(Profile),
    /// Put back the remembered mode and charge limit.
    Restore,
    /// Stop charging at a percentage.
    ChargeLimit(u8),
    /// Suspend to memory.
    Suspend,
    /// Hibernate to disk.
    Hibernate,
    /// Shut down.
    PowerOff,
    /// Restart.
    Reboot,
}

impl Verb {
    /// Parse the helper's arguments.
    ///
    /// # Errors
    /// Anything not in the closed set.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match args.as_slice() {
            ["profile", name] => Profile::parse(name)
                .map(Self::Profile)
                .ok_or_else(|| format!("no such power mode: {name}")),
            ["restore"] => Ok(Self::Restore),
            ["charge-limit", n] => n
                .parse()
                .ok()
                .filter(|n| CHARGE_LIMITS.contains(n))
                .map(Self::ChargeLimit)
                .ok_or_else(|| format!("charge limit must be one of {CHARGE_LIMITS:?}")),
            ["suspend"] => Ok(Self::Suspend),
            ["hibernate"] => Ok(Self::Hibernate),
            ["power-off"] => Ok(Self::PowerOff),
            ["reboot"] => Ok(Self::Reboot),
            _ => Err(
                "usage: alpymist-power-helper profile MODE | restore | charge-limit N | \
                 suspend | hibernate | power-off | reboot"
                    .into(),
            ),
        }
    }
}

fn write(path: &Path, value: &str) -> Result<(), String> {
    std::fs::write(path, value).map_err(|e| format!("{}: {e}", path.display()))
}

/// Put the machine under `sysfs` in `profile`.
///
/// # Errors
/// Nothing here can express it, or a write was refused.
pub fn set_profile(sysfs: &Path, profile: Profile) -> Result<(), String> {
    let plan = Knobs::read(sysfs).plan(profile);
    if plan.is_empty() {
        return Err(format!(
            "this machine has no way to set {}",
            profile.label().to_lowercase()
        ));
    }
    let mut failed = Vec::new();
    for (path, value) in plan {
        // One refused knob — a governor the firmware holds — leaves the rest
        // worth setting.
        if let Err(e) = write(&sysfs.join(&path), &value) {
            failed.push(e);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Every battery's charge limit files under `sysfs`.
fn limit_files(sysfs: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let root = sysfs.join("class/power_supply");
    let Ok(dir) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<_> = dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            std::fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery")
                && p.join("charge_control_end_threshold").exists()
        })
        .map(|p| {
            let start = p.join("charge_control_start_threshold");
            (
                p.join("charge_control_end_threshold"),
                start.exists().then_some(start),
            )
        })
        .collect();
    out.sort();
    out
}

/// Stop charging at `limit` percent.
///
/// # Errors
/// No battery has a limit, or the firmware refused it.
pub fn set_charge_limit(sysfs: &Path, limit: u8) -> Result<(), String> {
    let files = limit_files(sysfs);
    if files.is_empty() {
        return Err("this battery has no charge limit".into());
    }
    for (end, start) in files {
        // Where there is a start threshold it must stay below the end, or the
        // write of the end is refused; charging resumes a little under it.
        if let Some(start) = start {
            let resume = if limit >= 100 {
                95
            } else {
                limit.saturating_sub(5)
            };
            write(&start, "0").ok();
            write(&end, &limit.to_string())?;
            write(&start, &resume.to_string()).ok();
        } else {
            write(&end, &limit.to_string())?;
        }
    }
    Ok(())
}

/// Sleep, where the kernel offers `state` (`mem` or `disk`).
///
/// # Errors
/// The kernel does not offer it, or refused.
pub fn sleep(sysfs: &Path, state: &str) -> Result<(), String> {
    can_sleep(sysfs, state)?;
    write(&sysfs.join("power/state"), state)
}

/// Run `/sbin/poweroff` or `/sbin/reboot`, with nothing of the caller's
/// environment.
fn halt(program: &str) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .status()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed ({status})"))
    }
}

/// Whether the kernel offers `state` (`mem` or `disk`), and for `disk`
/// whether there is somewhere to resume from. Anyone may ask: the files are
/// readable without root, so a lock screen need not go up for nothing.
///
/// # Errors
/// Why it cannot, in a sentence.
pub fn can_sleep(sysfs: &Path, state: &str) -> Result<(), String> {
    let offered = std::fs::read_to_string(sysfs.join("power/state")).unwrap_or_default();
    if !offered.split_whitespace().any(|s| s == state) {
        return Err(match state {
            "disk" => "this machine cannot hibernate".into(),
            _ => "this machine cannot suspend".into(),
        });
    }
    if state == "disk" && !resume_configured(sysfs) {
        return Err("hibernation needs a swap partition given as resume= at boot".into());
    }
    Ok(())
}

fn resume_configured(sysfs: &Path) -> bool {
    std::fs::read_to_string(sysfs.join("power/resume")).is_ok_and(|r| {
        let r = r.trim();
        !r.is_empty() && r != "0:0"
    })
}

/// Whether the machine can hibernate.
#[must_use]
pub fn can_hibernate(sysfs: &Path) -> bool {
    can_sleep(sysfs, "disk").is_ok()
}

fn remember(file: &str, value: &str) -> Result<(), String> {
    let path = Path::new(file);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    write(path, value)
}

/// Carry out a verb, as root.
///
/// # Errors
/// What went wrong, in a sentence.
pub fn carry_out(verb: Verb) -> Result<(), String> {
    let sysfs = Path::new(crate::profile::SYSFS);
    match verb {
        Verb::Profile(p) => {
            set_profile(sysfs, p)?;
            remember(STATE, p.id())
        }
        Verb::Restore => {
            let mut result = Ok(());
            if let Some(p) = std::fs::read_to_string(STATE)
                .ok()
                .and_then(|s| Profile::parse(&s))
            {
                result = set_profile(sysfs, p);
            }
            if let Some(n) = std::fs::read_to_string(LIMIT_STATE)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .filter(|n| CHARGE_LIMITS.contains(n))
            {
                result = result.and(set_charge_limit(sysfs, n));
            }
            result
        }
        Verb::ChargeLimit(n) => {
            set_charge_limit(sysfs, n)?;
            remember(LIMIT_STATE, &n.to_string())
        }
        Verb::Suspend => sleep(sysfs, "mem"),
        Verb::Hibernate => sleep(sysfs, "disk"),
        Verb::PowerOff => halt("/sbin/poweroff"),
        Verb::Reboot => halt("/sbin/reboot"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Verb, set_charge_limit, sleep};
    use crate::profile::Profile;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn only_the_closed_set_is_accepted() {
        assert_eq!(
            Verb::parse(&args("profile performance")),
            Ok(Verb::Profile(Profile::Performance))
        );
        assert_eq!(
            Verb::parse(&args("charge-limit 80")),
            Ok(Verb::ChargeLimit(80))
        );
        assert!(Verb::parse(&args("charge-limit 42")).is_err());
        assert!(Verb::parse(&args("profile ../../etc")).is_err());
        assert!(Verb::parse(&args("suspend now")).is_err());
        assert!(Verb::parse(&[]).is_err());
    }

    #[test]
    fn a_limit_keeps_the_start_threshold_under_the_end() {
        let root = std::env::temp_dir().join(format!("alpymist-limit-{}", std::process::id()));
        let bat = root.join("class/power_supply/BAT0");
        std::fs::create_dir_all(&bat).unwrap();
        std::fs::write(bat.join("type"), "Battery\n").unwrap();
        std::fs::write(bat.join("charge_control_end_threshold"), "100\n").unwrap();
        std::fs::write(bat.join("charge_control_start_threshold"), "95\n").unwrap();
        set_charge_limit(&root, 80).unwrap();
        let read = |f: &str| std::fs::read_to_string(bat.join(f)).unwrap();
        assert_eq!(read("charge_control_end_threshold"), "80");
        assert_eq!(read("charge_control_start_threshold"), "75");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sleeping_is_refused_where_the_kernel_does_not_offer_it() {
        let root = std::env::temp_dir().join(format!("alpymist-sleep-{}", std::process::id()));
        std::fs::create_dir_all(root.join("power")).unwrap();
        std::fs::write(root.join("power/state"), "freeze mem\n").unwrap();
        assert!(sleep(&root, "disk").unwrap_err().contains("hibernate"));
        std::fs::remove_dir_all(&root).ok();
    }
}

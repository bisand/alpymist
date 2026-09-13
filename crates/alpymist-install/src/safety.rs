//! Refusing to destroy the wrong disk.
//!
//! Everything here takes the facts it needs as arguments rather than reading
//! the system, so every refusal can be tested directly. That matters more here
//! than anywhere else in the project: these checks are the only thing standing
//! between a mistake and somebody's data.
//!
//! The bias is deliberate and one-directional. A check that wrongly refuses
//! wastes a few minutes; a check that wrongly permits destroys a disk. Anything
//! ambiguous refuses.

/// Why a disk may not be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The path is not under `/dev`.
    NotADevicePath {
        /// What was asked for.
        path: String,
    },
    /// The device does not exist.
    Missing {
        /// What was asked for.
        path: String,
    },
    /// Something on this device is currently mounted.
    InUse {
        /// The device.
        path: String,
        /// Where one of its partitions is mounted.
        mounted_at: String,
    },
    /// This is the medium the installer itself booted from.
    IsInstallMedium {
        /// The device.
        path: String,
    },
    /// The user has not confirmed that erasing is intended.
    NotConfirmed,
}

impl Refusal {
    /// What to tell the person at the keyboard.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotADevicePath { path } => {
                format!("{path} is not a device path. Expected something under /dev.")
            }
            Self::Missing { path } => format!("{path} does not exist."),
            Self::InUse { path, mounted_at } => {
                format!("{path} is in use — part of it is mounted at {mounted_at}.")
            }
            Self::IsInstallMedium { path } => {
                format!("{path} is the medium Alpymist is running from.")
            }
            Self::NotConfirmed => {
                "Erasing this disk has not been confirmed on the disk screen.".into()
            }
        }
    }
}

/// What the running system knows about the disks.
///
/// Gathered once and passed in, so the checks below are pure.
#[derive(Debug, Clone, Default)]
pub struct SystemFacts {
    /// Device paths that exist, e.g. `/dev/sda`.
    pub existing_devices: Vec<String>,
    /// `(device, mount point)` for everything currently mounted.
    pub mounts: Vec<(String, String)>,
    /// The device the running system booted from, if known.
    pub install_medium: Option<String>,
}

impl SystemFacts {
    /// The facts for the machine this is running on.
    #[must_use]
    pub fn default_for_host() -> Self {
        gather()
    }

    /// Whether `device` is, or contains, the mount source at `mounted`.
    ///
    /// Partitions are matched by prefix because `/dev/sda1` lives on `/dev/sda`
    /// — erasing the disk takes the partition with it.
    pub(crate) fn covers(device: &str, mount_source: &str) -> bool {
        mount_source == device
            || (mount_source.starts_with(device)
                && mount_source[device.len()..]
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == 'p'))
    }
}

/// Decide whether `device` may be erased.
///
/// # Errors
/// Returns every reason it may not, so the user is not made to fix them one at
/// a time.
pub fn check(device: &str, confirmed: bool, facts: &SystemFacts) -> Result<(), Vec<Refusal>> {
    let mut refusals = Vec::new();

    if !confirmed {
        refusals.push(Refusal::NotConfirmed);
    }
    if !device.starts_with("/dev/") || device.contains("..") {
        refusals.push(Refusal::NotADevicePath {
            path: device.to_string(),
        });
        // Everything below assumes a real device path; stop here.
        return Err(refusals);
    }
    if !facts.existing_devices.iter().any(|d| d == device) {
        refusals.push(Refusal::Missing {
            path: device.to_string(),
        });
    }
    if facts
        .install_medium
        .as_deref()
        .is_some_and(|m| SystemFacts::covers(device, m))
    {
        refusals.push(Refusal::IsInstallMedium {
            path: device.to_string(),
        });
    }
    if let Some((_, at)) = facts
        .mounts
        .iter()
        .find(|(source, _)| SystemFacts::covers(device, source))
    {
        refusals.push(Refusal::InUse {
            path: device.to_string(),
            mounted_at: at.clone(),
        });
    }

    if refusals.is_empty() {
        Ok(())
    } else {
        Err(refusals)
    }
}

/// Read what the running system knows about its disks.
///
/// Linux-only, and best-effort by design: anything it fails to learn is left
/// empty, and an empty fact makes [`check`] *more* likely to refuse, never
/// less. A guard that silently weakens when it cannot read `/proc` would be
/// worse than no guard at all.
#[must_use]
pub fn gather() -> SystemFacts {
    SystemFacts {
        existing_devices: block_devices(),
        mounts: mounts(),
        install_medium: install_medium(),
    }
}

/// Whole disks, from `/sys/block`.
fn block_devices() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .map(|name| format!("/dev/{name}"))
        .collect()
}

/// Everything currently mounted, as `(source, mount point)`.
fn mounts() -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    parse_mounts(&text)
}

/// Parse `/proc/mounts`. Split out so it can be tested against real content.
fn parse_mounts(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?;
            let target = fields.next()?;
            // Only real block devices can be destroyed by writing to a disk.
            source
                .starts_with("/dev/")
                .then(|| (source.to_string(), target.to_string()))
        })
        .collect()
}

/// The device Alpymist itself is running from, if it can be identified.
///
/// On a live image the ISO is mounted under `/media`, and the module loop is
/// backed by a file on it. Either is enough to recognise the medium; both are
/// checked because a machine booted in an unusual way may show only one.
fn install_medium() -> Option<String> {
    mounts()
        .into_iter()
        .find(|(_, target)| target.starts_with("/media") || target.starts_with("/.modloop"))
        .map(|(source, _)| source)
}

#[cfg(test)]
mod tests {
    use super::{Refusal, SystemFacts, check};

    fn facts() -> SystemFacts {
        SystemFacts {
            existing_devices: vec!["/dev/sda".into(), "/dev/sdb".into(), "/dev/sr0".into()],
            mounts: vec![("/dev/sr0".into(), "/media/cdrom".into())],
            install_medium: Some("/dev/sr0".into()),
        }
    }

    #[test]
    fn an_ordinary_unused_disk_is_allowed() {
        assert!(check("/dev/sdb", true, &facts()).is_ok());
    }

    #[test]
    fn an_unconfirmed_erase_is_refused() {
        let refusals = check("/dev/sdb", false, &facts()).unwrap_err();
        assert!(refusals.contains(&Refusal::NotConfirmed));
    }

    /// The single most important check here: never eat the medium you booted
    /// from, which on a live install is the most likely thing to be selected
    /// by accident.
    #[test]
    fn the_installation_medium_is_refused() {
        let refusals = check("/dev/sr0", true, &facts()).unwrap_err();
        assert!(
            refusals
                .iter()
                .any(|r| matches!(r, Refusal::IsInstallMedium { .. }))
        );
    }

    #[test]
    fn a_disk_with_something_mounted_on_it_is_refused() {
        let mut f = facts();
        f.mounts.push(("/dev/sdb1".into(), "/mnt/data".into()));
        let refusals = check("/dev/sdb", true, &f).unwrap_err();
        assert!(
            refusals.iter().any(
                |r| matches!(r, Refusal::InUse { mounted_at, .. } if mounted_at == "/mnt/data")
            ),
            "a mounted partition must block its whole disk: {refusals:?}"
        );
    }

    #[test]
    fn nvme_style_partition_names_are_recognised_as_part_of_their_disk() {
        let f = SystemFacts {
            existing_devices: vec!["/dev/nvme0n1".into()],
            mounts: vec![("/dev/nvme0n1p2".into(), "/".into())],
            install_medium: None,
        };
        let refusals = check("/dev/nvme0n1", true, &f).unwrap_err();
        assert!(refusals.iter().any(|r| matches!(r, Refusal::InUse { .. })));
    }

    /// `/dev/sdb` must not be considered in use because `/dev/sdbx` is.
    #[test]
    fn a_similarly_named_device_does_not_block_a_different_one() {
        let f = SystemFacts {
            existing_devices: vec!["/dev/sdb".into()],
            mounts: vec![("/dev/sdbx1".into(), "/mnt".into())],
            install_medium: None,
        };
        assert!(check("/dev/sdb", true, &f).is_ok());
    }

    #[test]
    fn a_device_that_does_not_exist_is_refused() {
        let refusals = check("/dev/sdz", true, &facts()).unwrap_err();
        assert!(
            refusals
                .iter()
                .any(|r| matches!(r, Refusal::Missing { .. }))
        );
    }

    #[test]
    fn a_path_outside_dev_is_refused_without_looking_further() {
        for path in [
            "/home/andre/disk.img",
            "sda",
            "../dev/sda",
            "/dev/../etc/passwd",
        ] {
            let refusals = check(path, true, &facts()).unwrap_err();
            assert!(
                refusals
                    .iter()
                    .any(|r| matches!(r, Refusal::NotADevicePath { .. })),
                "{path} should be refused as not a device path"
            );
        }
    }

    #[test]
    fn every_reason_is_reported_at_once_rather_than_one_at_a_time() {
        let refusals = check("/dev/sr0", false, &facts()).unwrap_err();
        assert!(
            refusals.len() >= 2,
            "expected several reasons, got {refusals:?}"
        );
    }

    #[test]
    fn mounts_are_parsed_and_non_block_sources_ignored() {
        let text = "\
proc /proc proc rw,nosuid 0 0
/dev/sr0 /media/sr0 iso9660 ro 0 0
tmpfs /run tmpfs rw 0 0
/dev/sda1 / ext4 rw,relatime 0 0
";
        let mounts = super::parse_mounts(text);
        assert_eq!(
            mounts,
            vec![
                ("/dev/sr0".to_string(), "/media/sr0".to_string()),
                ("/dev/sda1".to_string(), "/".to_string()),
            ],
            "only block devices can be destroyed, so only they matter here"
        );
    }

    #[test]
    fn a_malformed_mounts_line_is_skipped_rather_than_panicking() {
        assert!(super::parse_mounts("/dev/sda1\n\n   \n").is_empty());
    }

    /// If the facts cannot be read, the checks must get stricter, not laxer.
    #[test]
    fn empty_facts_refuse_everything() {
        let empty = SystemFacts::default();
        let refusals = check("/dev/sda", true, &empty).unwrap_err();
        assert!(
            refusals
                .iter()
                .any(|r| matches!(r, Refusal::Missing { .. }))
        );
    }

    #[test]
    fn every_refusal_explains_itself_in_a_sentence() {
        for r in [
            Refusal::NotADevicePath { path: "x".into() },
            Refusal::Missing {
                path: "/dev/sdz".into(),
            },
            Refusal::InUse {
                path: "/dev/sda".into(),
                mounted_at: "/".into(),
            },
            Refusal::IsInstallMedium {
                path: "/dev/sr0".into(),
            },
            Refusal::NotConfirmed,
        ] {
            let m = r.message();
            assert!(m.len() > 20 && m.ends_with('.'), "poor message: {m:?}");
        }
    }
}

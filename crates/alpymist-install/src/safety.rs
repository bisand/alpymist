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
    /// Whether `device` is, or contains, the mount source at `mounted`.
    ///
    /// Partitions are matched by prefix because `/dev/sda1` lives on `/dev/sda`
    /// — erasing the disk takes the partition with it.
    fn covers(device: &str, mount_source: &str) -> bool {
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

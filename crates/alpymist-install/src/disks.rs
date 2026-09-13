//! Finding the disks a system could be installed to.
//!
//! Read from `/sys/block` rather than by running `lsblk` or `blkid`: sysfs is
//! always there on the live image, needs no parsing of tool output that changes
//! between versions, and can be faked with a directory in tests.
//!
//! This list only decides what is *offered*. Whether a disk may actually be
//! erased is still decided by [`crate::safety`] right before it happens, so a
//! mistake here can at worst show a disk the installer then refuses.

use crate::safety::SystemFacts;
use std::path::Path;

/// A whole disk the user could choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// Device path, e.g. `/dev/vda`.
    pub device: String,
    /// Capacity in bytes.
    pub bytes: u64,
    /// What the hardware calls itself, if it says. Often empty in VMs.
    pub model: String,
    /// Where part of this disk is mounted, if anywhere.
    ///
    /// Such a disk is shown but cannot be chosen: hiding it would leave people
    /// hunting for a disk they can see in their machine.
    pub mounted_at: Option<String>,
}

/// Longest model name shown before it is cut, so a row cannot overflow the panel.
const MODEL_CHARS: usize = 24;

/// Kernel name prefixes that are never somewhere to install to.
///
/// Loop and RAM devices are the live image's own plumbing, `sr` and `fd` are
/// optical and floppy drives, and `dm-`, `md` and `nbd` are built on top of
/// other disks, where erasing the layer would not do what anybody expects.
const NOT_INSTALLABLE: [&str; 9] = [
    "loop", "ram", "zram", "sr", "fd", "dm-", "md", "nbd", "mtdblock",
];

impl Disk {
    /// The row text: device, size, model, in aligned columns.
    #[must_use]
    pub fn label(&self) -> String {
        let model: String = self.model.chars().take(MODEL_CHARS).collect();
        let text = format!("{:<13}{:>10}   {model}", self.device, size_text(self.bytes));
        match &self.mounted_at {
            Some(at) => format!("{}  (in use at {at})", text.trim_end()),
            None => text.trim_end().to_string(),
        }
    }
}

/// Human-readable size in binary units, labelled as such.
///
/// Binary rather than decimal because that is what every other Linux tool the
/// user might cross-check against (`lsblk`, `fdisk`) prints.
#[must_use]
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0;
    while unit < UNITS.len() - 1 && u128::from(bytes) >= 1u128 << (10 * (unit + 1)) {
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} B");
    }
    // Tenths, in integers: `bytes` can exceed what f64 holds exactly.
    let divisor = 1u128 << (10 * unit);
    let tenths = (u128::from(bytes) * 10 + divisor / 2) / divisor;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

/// The disks on the running system.
#[must_use]
pub fn discover(facts: &SystemFacts) -> Vec<Disk> {
    read_from(Path::new("/sys/block"), facts)
}

/// The disks described by a sysfs-shaped directory.
///
/// Leaves out anything that is not a real disk, is read-only, is empty (a card
/// reader with no card), or holds the medium the installer is running from.
#[must_use]
pub fn read_from(sys_block: &Path, facts: &SystemFacts) -> Vec<Disk> {
    let Ok(entries) = std::fs::read_dir(sys_block) else {
        return Vec::new();
    };
    let mut disks: Vec<Disk> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !installable_name(&name) {
                return None;
            }
            let dir = entry.path();
            if read_trimmed(&dir.join("ro")).as_deref() == Some("1") {
                return None;
            }
            // sysfs counts in 512-byte sectors whatever the real sector size.
            let sectors: u64 = read_trimmed(&dir.join("size"))?.parse().ok()?;
            if sectors == 0 {
                return None;
            }
            let device = format!("/dev/{name}");
            if facts
                .install_medium
                .as_deref()
                .is_some_and(|m| SystemFacts::covers(&device, m))
            {
                return None;
            }
            let model = ["device/model", "device/name"]
                .iter()
                .find_map(|f| read_trimmed(&dir.join(f)).filter(|s| !s.is_empty()))
                .unwrap_or_default();
            let mounted_at = facts
                .mounts
                .iter()
                .find(|(source, _)| SystemFacts::covers(&device, source))
                .map(|(_, at)| at.clone());
            Some(Disk {
                device,
                bytes: sectors.saturating_mul(512),
                model,
                mounted_at,
            })
        })
        .collect();
    // Directory order is arbitrary; sda before sdb is what people expect.
    disks.sort_by(|a, b| a.device.cmp(&b.device));
    disks
}

fn installable_name(name: &str) -> bool {
    !NOT_INSTALLABLE.iter().any(|p| name.starts_with(p))
        // eMMC exposes its boot areas and replay-protected block as extra disks.
        && !(name.starts_with("mmcblk") && (name.contains("boot") || name.contains("rpmb")))
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Stand-in disks for previews on machines that have no `/sys/block`.
#[must_use]
pub fn sample() -> Vec<Disk> {
    vec![
        Disk {
            device: "/dev/sda".into(),
            bytes: 256_060_514_304,
            model: "Samsung SSD 860".into(),
            mounted_at: None,
        },
        Disk {
            device: "/dev/sdb".into(),
            bytes: 4_000_787_030_016,
            model: "WDC WD40EFRX".into(),
            mounted_at: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{Disk, read_from, size_text};
    use crate::safety::SystemFacts;
    use std::path::{Path, PathBuf};

    /// A throwaway sysfs lookalike, removed when dropped.
    struct FakeSys(PathBuf);

    impl FakeSys {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("alpymist-disks-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn disk(&self, name: &str, sectors: u64, ro: bool, model: Option<&str>) -> &Self {
            let d = self.0.join(name);
            std::fs::create_dir_all(d.join("device")).unwrap();
            std::fs::write(d.join("size"), format!("{sectors}\n")).unwrap();
            std::fs::write(d.join("ro"), if ro { "1\n" } else { "0\n" }).unwrap();
            if let Some(m) = model {
                std::fs::write(d.join("device/model"), format!("{m}   \n")).unwrap();
            }
            self
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for FakeSys {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn no_facts() -> SystemFacts {
        SystemFacts {
            existing_devices: Vec::new(),
            mounts: Vec::new(),
            install_medium: None,
        }
    }

    #[test]
    fn real_disks_are_listed_in_order_with_size_and_model() {
        let sys = FakeSys::new("order");
        sys.disk("vdb", 2_097_152, false, None).disk(
            "nvme0n1",
            500_107_862,
            false,
            Some("Samsung SSD 970 EVO"),
        );
        let disks = read_from(sys.path(), &no_facts());
        assert_eq!(
            disks,
            vec![
                Disk {
                    device: "/dev/nvme0n1".into(),
                    bytes: 500_107_862 * 512,
                    model: "Samsung SSD 970 EVO".into(),
                    mounted_at: None,
                },
                Disk {
                    device: "/dev/vdb".into(),
                    bytes: 1024 * 1024 * 1024,
                    model: String::new(),
                    mounted_at: None,
                },
            ]
        );
    }

    #[test]
    fn plumbing_read_only_and_empty_devices_are_left_out() {
        let sys = FakeSys::new("plumbing");
        sys.disk("loop0", 1000, false, None)
            .disk("zram0", 1000, false, None)
            .disk("sr0", 1000, false, None)
            .disk("dm-0", 1000, false, None)
            .disk("mmcblk0boot0", 1000, false, None)
            .disk("sdc", 0, false, None)
            .disk("sdd", 1000, true, None)
            .disk("mmcblk0", 1000, false, None);
        let names: Vec<String> = read_from(sys.path(), &no_facts())
            .into_iter()
            .map(|d| d.device)
            .collect();
        assert_eq!(names, vec!["/dev/mmcblk0".to_string()]);
    }

    /// The installer must not even offer the stick it is running from.
    #[test]
    fn the_install_medium_is_not_offered() {
        let sys = FakeSys::new("medium");
        sys.disk("sda", 1000, false, None)
            .disk("sdb", 1000, false, None);
        let facts = SystemFacts {
            install_medium: Some("/dev/sdb1".into()),
            ..no_facts()
        };
        let names: Vec<String> = read_from(sys.path(), &facts)
            .into_iter()
            .map(|d| d.device)
            .collect();
        assert_eq!(names, vec!["/dev/sda".to_string()]);
    }

    #[test]
    fn a_disk_with_something_mounted_says_where() {
        let sys = FakeSys::new("mounted");
        sys.disk("vda", 8_388_608, false, None);
        let facts = SystemFacts {
            mounts: vec![("/dev/vda3".into(), "/".into())],
            ..no_facts()
        };
        let disks = read_from(sys.path(), &facts);
        assert_eq!(disks[0].mounted_at.as_deref(), Some("/"));
        assert_eq!(disks[0].label(), "/dev/vda        4.0 GiB  (in use at /)");
    }

    #[test]
    fn a_missing_sysfs_yields_no_disks_rather_than_an_error() {
        assert!(read_from(Path::new("/definitely/not/sys/block"), &no_facts()).is_empty());
    }

    #[test]
    fn sizes_read_the_way_lsblk_prints_them() {
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(20 * 1024 * 1024 * 1024), "20.0 GiB");
        assert_eq!(size_text(256_060_514_304), "238.5 GiB");
        assert_eq!(size_text(4_000_787_030_016), "3.6 TiB");
        assert_eq!(size_text(8 * 1024 * 1024), "8.0 MiB");
    }

    #[test]
    fn long_model_names_are_cut_so_the_row_fits() {
        let d = Disk {
            device: "/dev/sda".into(),
            bytes: 1024 * 1024 * 1024,
            model: "An Extraordinarily Long Model Name Indeed".into(),
            mounted_at: None,
        };
        assert!(d.label().ends_with("An Extraordinarily Long ".trim_end()));
        assert!(d.label().starts_with("/dev/sda"));
    }
}

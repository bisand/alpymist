//! What the kernel says about Thunderbolt and USB4, read from sysfs.
//!
//! Every Thunderbolt 3, Thunderbolt 4 and USB4 host, Intel or AMD, appears
//! the same way under `/sys/bus/thunderbolt/devices`: a `domainN` per host
//! controller, carrying the `security` level the firmware chose, and a
//! directory per device plugged into it, named `N-ROUTE`, with `authorized`,
//! `unique_id`, `vendor_name` and `device_name`, and a `key` file only where
//! the device can prove who it is. Nothing here knows any vendor or model;
//! see the kernel's Documentation/admin-guide/thunderbolt.rst.

use std::path::{Path, PathBuf};

/// Where the bus is, under the root.
pub const BUS: &str = "sys/bus/thunderbolt/devices";

/// A domain's security level, as the firmware set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Every device is let in by the firmware; there is nothing to approve.
    None,
    /// A device is let in when someone writes `1` to its `authorized`; it is
    /// known only by the UUID it reports, which it could lie about.
    User,
    /// As `User`, and a device that supports it also answers a challenge
    /// against a key stored in it the first time, so it can prove who it is.
    Secure,
    /// Only `DisplayPort` is tunnelled; there is nothing to approve.
    DpOnly,
    /// Only `DisplayPort` and USB are tunnelled, by the firmware; there is
    /// nothing to approve.
    UsbOnly,
    /// `PCIe` tunnelling is off; there is nothing to approve.
    NoPcie,
}

impl Level {
    /// The level named in a domain's `security` file.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "none" => Self::None,
            "user" => Self::User,
            "secure" => Self::Secure,
            "dponly" => Self::DpOnly,
            "usbonly" => Self::UsbOnly,
            "nopcie" => Self::NoPcie,
            _ => return None,
        })
    }

    /// Whether devices at this level wait for someone to approve them.
    #[must_use]
    pub fn asks(self) -> bool {
        matches!(self, Self::User | Self::Secure)
    }
}

/// A host controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    /// The number in `domainN`, and the prefix of its devices' names.
    pub index: u32,
    /// Its security level, when the kernel names one it knows.
    pub level: Option<Level>,
    /// Whether the IOMMU keeps what a device may reach by DMA to what its
    /// driver gave it (`iommu_dma_protection`).
    pub dma_protected: bool,
}

/// A device plugged into a domain, directly or through another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Its directory name, such as `0-1` or `0-301`.
    pub name: String,
    /// The domain it is in.
    pub domain: u32,
    /// The UUID it reports.
    pub uuid: String,
    /// Its vendor, as it says.
    pub vendor: String,
    /// Its name, as it says.
    pub model: String,
    /// Whether it is let in: 0 not, 1 or 2 yes.
    pub authorized: bool,
    /// Whether it can prove who it is with a key (it has a `key` file).
    pub keyed: bool,
    /// Its Thunderbolt generation (3, 4) or USB4, when the kernel says.
    pub generation: Option<u32>,
    /// Where it is in sysfs.
    pub path: PathBuf,
}

impl Device {
    /// How far down a daisy chain it is: a device behind another must wait
    /// for the one in front to be let in.
    #[must_use]
    pub fn depth(&self) -> usize {
        // The route is hex, a byte per hop from the host, printed without
        // leading zeros: the one behind is always the longer, so 0-301 comes
        // after 0-1.
        self.name
            .split_once('-')
            .map_or(0, |(_, route)| route.trim_start_matches('0').len())
    }

    /// What to call it: the name it gives, or its vendor's where it gives
    /// none. The vendor is said beside it anyway.
    #[must_use]
    pub fn title(&self) -> String {
        let model = self.model.trim();
        let vendor = self.vendor.trim();
        if !model.is_empty() {
            model.to_owned()
        } else if !vendor.is_empty() {
            format!("{vendor} device")
        } else {
            "Unnamed device".into()
        }
    }

    /// "Thunderbolt 3", "USB4", or just "Thunderbolt".
    #[must_use]
    pub fn kind(&self) -> String {
        match self.generation {
            Some(g @ 1..=3) => format!("Thunderbolt {g}"),
            Some(4) => "USB4 or Thunderbolt 4".into(),
            _ => "Thunderbolt".into(),
        }
    }
}

/// Everything on the bus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bus {
    /// The host controllers.
    pub domains: Vec<Domain>,
    /// The devices, host routers left out, nearest the host first.
    pub devices: Vec<Device>,
}

impl Bus {
    /// Read the bus under `root` (`/` on a real system). A machine without
    /// Thunderbolt or USB4 reads as empty.
    #[must_use]
    pub fn read(root: &Path) -> Self {
        let dir = root.join(BUS);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Self::default();
        };
        let mut bus = Self::default();
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = dir.join(&name);
            if let Some(index) = name.strip_prefix("domain").and_then(|n| n.parse().ok()) {
                bus.domains.push(Domain {
                    index,
                    level: read(&path, "security").as_deref().and_then(Level::parse),
                    dma_protected: read(&path, "iommu_dma_protection").as_deref() == Some("1"),
                });
            } else if let Some(device) = device(&name, &path) {
                bus.devices.push(device);
            }
        }
        bus.domains.sort_by_key(|d| d.index);
        bus.devices
            .sort_by(|a, b| (a.domain, a.depth(), &a.name).cmp(&(b.domain, b.depth(), &b.name)));
        bus
    }

    /// The domain a device is in.
    #[must_use]
    pub fn domain_of(&self, device: &Device) -> Option<&Domain> {
        self.domains.iter().find(|d| d.index == device.domain)
    }

    /// The device reporting `uuid`.
    #[must_use]
    pub fn find(&self, uuid: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.uuid == uuid)
    }
}

/// A device directory, or `None` for a host router, a retimer, a link to
/// another computer, or anything else that is not a device to approve.
fn device(name: &str, path: &Path) -> Option<Device> {
    let (domain, route) = name.split_once('-')?;
    let domain = domain.parse().ok()?;
    // N-0 is the host router itself; retimers and XDomain links have ':' or
    // '.' in their names and no `authorized`.
    if route.chars().any(|c| !c.is_ascii_hexdigit()) || route.trim_start_matches('0').is_empty() {
        return None;
    }
    let authorized = read(path, "authorized")?;
    let uuid = read(path, "unique_id").filter(|u| valid_uuid(u))?;
    Some(Device {
        name: name.to_owned(),
        domain,
        uuid,
        vendor: read(path, "vendor_name").unwrap_or_default(),
        model: read(path, "device_name").unwrap_or_default(),
        authorized: authorized != "0",
        keyed: path.join("key").exists(),
        generation: read(path, "generation").and_then(|g| g.parse().ok()),
        path: path.to_owned(),
    })
}

fn read(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file))
        .ok()
        .map(|s| s.trim().to_owned())
}

/// Whether `s` is a UUID as the kernel prints one: 8-4-4-4-12 hex digits.
#[must_use]
pub fn valid_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Bus, Level, valid_uuid};
    use std::path::{Path, PathBuf};

    /// A fake sysfs under a fresh directory.
    pub struct Fake(pub PathBuf);

    impl Fake {
        pub fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "alpymist-thunderbolt-{name}-{}",
                std::process::id()
            ));
            std::fs::remove_dir_all(&root).ok();
            std::fs::create_dir_all(root.join(super::BUS)).unwrap();
            Self(root)
        }

        pub fn file(&self, rel: &str, contents: &str) -> &Self {
            let p = self.0.join(super::BUS).join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, format!("{contents}\n")).unwrap();
            self
        }

        pub fn domain(&self, n: u32, level: &str) -> &Self {
            self.file(&format!("domain{n}/security"), level)
                .file(&format!("domain{n}/iommu_dma_protection"), "0")
        }

        pub fn device(&self, name: &str, uuid: &str, model: &str, authorized: u8) -> &Self {
            self.file(&format!("{name}/authorized"), &authorized.to_string())
                .file(&format!("{name}/unique_id"), uuid)
                .file(&format!("{name}/vendor_name"), "Lenovo")
                .file(&format!("{name}/device_name"), model)
                .file(&format!("{name}/generation"), "3")
        }

        pub fn root(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    pub const DOCK: &str = "004f2815-a7cf-0801-ffff-ffffffffffff";

    #[test]
    fn a_docked_laptop_reads_as_one_domain_and_one_device() {
        let f = Fake::new("docked");
        f.domain(0, "user")
            .device(
                "0-0",
                "11111111-2222-3333-4444-555555555555",
                "X1 Carbon",
                1,
            )
            .device("0-1", DOCK, "ThinkPad Thunderbolt 3 Dock", 0);
        let bus = Bus::read(f.root());
        assert_eq!(bus.domains.len(), 1);
        assert_eq!(bus.domains[0].level, Some(Level::User));
        assert!(!bus.domains[0].dma_protected);
        // The host router is not a device to approve.
        assert_eq!(bus.devices.len(), 1);
        let dock = &bus.devices[0];
        assert_eq!(dock.uuid, DOCK);
        assert!(!dock.authorized);
        assert!(!dock.keyed);
        assert_eq!(dock.title(), "ThinkPad Thunderbolt 3 Dock");
        assert_eq!(dock.kind(), "Thunderbolt 3");
    }

    #[test]
    fn a_daisy_chain_comes_nearest_first() {
        let f = Fake::new("chain");
        f.domain(0, "secure")
            .device("0-301", "00000000-0000-0000-0000-000000000003", "Drive", 0)
            .device("0-1", "00000000-0000-0000-0000-000000000001", "Dock", 0)
            .file("0-1/key", "");
        let bus = Bus::read(f.root());
        let names: Vec<_> = bus.devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["0-1", "0-301"]);
        assert!(bus.devices[0].keyed);
        assert!(!bus.devices[1].keyed);
    }

    #[test]
    fn retimers_links_and_nonsense_are_not_devices() {
        let f = Fake::new("others");
        f.domain(0, "user")
            .file("0-0:1.1/authorized", "0")
            .file("0-1.1/unique_id", super::tests::DOCK)
            .device("0-2", "not-a-uuid", "Liar", 0);
        assert!(Bus::read(f.root()).devices.is_empty());
    }

    #[test]
    fn no_thunderbolt_is_an_empty_bus() {
        assert_eq!(Bus::read(Path::new("/nonexistent")), Bus::default());
    }

    #[test]
    fn only_user_and_secure_ask() {
        for (name, asks) in [
            ("none", false),
            ("user", true),
            ("secure", true),
            ("dponly", false),
            ("usbonly", false),
            ("nopcie", false),
        ] {
            assert_eq!(Level::parse(name).unwrap().asks(), asks, "{name}");
        }
        assert_eq!(Level::parse("bogus"), None);
    }

    #[test]
    fn a_device_without_a_name_is_called_by_its_vendor() {
        let f = Fake::new("titles");
        f.domain(0, "user").device("0-1", DOCK, "Dock", 0).device(
            "0-2",
            "00000000-0000-0000-0000-000000000002",
            "",
            0,
        );
        let bus = Bus::read(f.root());
        assert_eq!(bus.devices[0].title(), "Dock");
        assert_eq!(bus.devices[1].title(), "Lenovo device");
    }

    #[test]
    fn uuids_are_checked_strictly() {
        assert!(valid_uuid(DOCK));
        assert!(!valid_uuid("004f2815-a7cf-0801-ffff"));
        assert!(!valid_uuid("../../../etc/passwd"));
        assert!(!valid_uuid("004f2815-a7cf-0801-ffff-fffffffffffg"));
    }
}

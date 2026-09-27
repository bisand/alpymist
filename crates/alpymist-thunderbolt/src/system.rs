//! What only root may do: the helper's side.
//!
//! `alpymist-thunderbolt-helper` is run through pkexec from the session, and
//! by udev and `OpenRC` at boot. It does one of a closed set of things. A
//! device is named by the UUID it reports, checked, and found by reading the
//! bus; no path is ever taken from the caller.

use crate::policy::{self, Decision, Method, Session};
use crate::store::{self, Allowed, Store};
use crate::sysfs::{Bus, Device, valid_uuid};
use std::path::Path;

/// How long an allowing lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Until it is unplugged.
    Once,
    /// From now on.
    Always,
}

/// A helper verb, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verb {
    /// Let a device in now, and perhaps from now on.
    Allow(String, Keep),
    /// Let in again a device allowed always.
    Reconnect(String),
    /// Stop allowing a device always.
    Forget(String),
    /// Let in every device plugged in that can prove it was allowed always:
    /// at boot, and whenever udev sees one arrive.
    Boot,
}

/// The helper's usage.
pub const USAGE: &str = "usage: alpymist-thunderbolt-helper allow UUID once|always | \
                         reconnect UUID | forget UUID | boot";

impl Verb {
    /// Parse the helper's arguments.
    ///
    /// # Errors
    /// Anything not in the closed set, or not a UUID.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let uuid = |u: &str| {
            if valid_uuid(u) {
                Ok(u.to_ascii_lowercase())
            } else {
                Err(format!("not a device UUID: {u}"))
            }
        };
        match args.as_slice() {
            ["allow", u, "once"] => Ok(Self::Allow(uuid(u)?, Keep::Once)),
            ["allow", u, "always"] => Ok(Self::Allow(uuid(u)?, Keep::Always)),
            ["reconnect", u] => Ok(Self::Reconnect(uuid(u)?)),
            ["forget", u] => Ok(Self::Forget(uuid(u)?)),
            ["boot"] => Ok(Self::Boot),
            _ => Err(USAGE.into()),
        }
    }
}

/// Do `verb` on the system under `root` (`/` for real).
///
/// # Errors
/// The device is not there, is not allowed, or refused.
pub fn carry_out(root: &Path, verb: &Verb) -> Result<(), String> {
    let store = Store::new(root);
    match verb {
        Verb::Allow(uuid, keep) => allow(root, &store, uuid, *keep),
        Verb::Reconnect(uuid) => {
            if store.get(uuid).is_none() {
                return Err("that device is not allowed always".into());
            }
            let bus = Bus::read(root);
            let device = present(&bus, uuid)?;
            if device.authorized {
                return Ok(());
            }
            let key = store.key(uuid);
            let method = match policy::first_method(device, bus.domain_of(device)) {
                Method::StoreKey if key.is_some() => Method::Challenge,
                _ => Method::Plain,
            };
            authorize(device, method, key.as_deref())
        }
        Verb::Forget(uuid) => store.remove(uuid),
        Verb::Boot => {
            boot(root, &store);
            Ok(())
        }
    }
}

fn present<'a>(bus: &'a Bus, uuid: &str) -> Result<&'a Device, String> {
    bus.find(uuid)
        .ok_or_else(|| "that device is not plugged in".into())
}

fn allow(root: &Path, store: &Store, uuid: &str, keep: Keep) -> Result<(), String> {
    let bus = Bus::read(root);
    let device = present(&bus, uuid)?;
    let domain = bus.domain_of(device);
    if !domain
        .and_then(|d| d.level)
        .is_some_and(crate::sysfs::Level::asks)
    {
        return Err("nothing needs allowing: the firmware decides for this port".into());
    }
    let key = if device.authorized {
        // In already, perhaps allowed once: remembering it needs no second
        // key, and it cannot be given one now.
        None
    } else {
        let method = policy::first_method(device, domain);
        let key = (method == Method::StoreKey)
            .then(store::new_key)
            .transpose()?;
        authorize(device, method, key.as_deref())?;
        key
    };
    if keep == Keep::Always {
        store.add(
            Allowed {
                uuid: device.uuid.clone(),
                vendor: device.vendor.clone(),
                model: device.model.clone(),
            },
            key.as_deref(),
        )?;
    }
    Ok(())
}

/// Let in, nearest the host first, every device allowed always that proves
/// who it is. A device behind another appears only once the one in front is
/// in, so the bus is read again until nothing more is let in.
fn boot(root: &Path, store: &Store) {
    let allowed = store.allowed();
    for _ in 0..8 {
        let bus = Bus::read(root);
        let mut progress = false;
        for device in &bus.devices {
            let entry = allowed.iter().find(|a| a.uuid == device.uuid);
            let key = store.key(&device.uuid);
            let decision = policy::decide(
                device,
                bus.domain_of(device),
                entry,
                key.is_some(),
                Session::None,
            );
            if let Decision::Reconnect(method) = decision {
                match authorize(device, method, key.as_deref()) {
                    Ok(()) => progress = true,
                    Err(e) => eprintln!("alpymist-thunderbolt-helper: {}: {e}", device.title()),
                }
            }
        }
        if !progress {
            break;
        }
    }
}

/// Let `device` in by `method`, with `key` where the method needs one.
fn authorize(device: &Device, method: Method, key: Option<&str>) -> Result<(), String> {
    let write = |file: &str, value: &str| {
        std::fs::write(device.path.join(file), value).map_err(|e| (file.to_owned(), e))
    };
    let result = match method {
        Method::Plain => write("authorized", "1"),
        Method::StoreKey | Method::Challenge => {
            let key = key.filter(|k| store::valid_key(k)).ok_or("no key")?;
            write("key", key).and_then(|()| {
                write(
                    "authorized",
                    if method == Method::Challenge {
                        "2"
                    } else {
                        "1"
                    },
                )
            })
        }
    };
    result.map_err(|(file, e)| match (method, e.raw_os_error()) {
        // EKEYREJECTED: the device's answer did not match the key.
        (Method::Challenge, Some(129)) => format!(
            "{} did not prove it is the device that was allowed; it stays out",
            device.title()
        ),
        _ => format!("{}: {e}", device.path.join(file).display()),
    })
}

#[cfg(test)]
mod tests {
    use super::{Keep, Verb, carry_out};
    use crate::store::Store;
    use crate::sysfs::tests::{DOCK, Fake};

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn read(f: &Fake, rel: &str) -> String {
        std::fs::read_to_string(f.root().join(crate::sysfs::BUS).join(rel))
            .unwrap()
            .trim()
            .to_owned()
    }

    #[test]
    fn only_the_closed_set_parses() {
        assert_eq!(
            Verb::parse(&args(&format!("allow {DOCK} always"))),
            Ok(Verb::Allow(DOCK.into(), Keep::Always))
        );
        assert_eq!(Verb::parse(&args("boot")), Ok(Verb::Boot));
        assert!(Verb::parse(&args(&format!("allow {DOCK} forever"))).is_err());
        assert!(Verb::parse(&args("allow ../../x once")).is_err());
        assert!(Verb::parse(&args("reconnect")).is_err());
        assert!(Verb::parse(&args("rm -rf /")).is_err());
    }

    #[test]
    fn allowed_once_it_is_let_in_and_not_remembered() {
        let f = Fake::new("allow-once");
        f.domain(0, "user").device("0-1", DOCK, "Dock", 0);
        carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Once)).unwrap();
        assert_eq!(read(&f, "0-1/authorized"), "1");
        assert!(Store::new(f.root()).allowed().is_empty());
    }

    #[test]
    fn allowed_always_on_a_secure_port_it_is_given_a_key() {
        let f = Fake::new("allow-secure");
        f.domain(0, "secure")
            .device("0-1", DOCK, "Dock", 0)
            .file("0-1/key", "");
        carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Always)).unwrap();
        let store = Store::new(f.root());
        let key = store.key(DOCK).expect("a key kept for next time");
        assert_eq!(read(&f, "0-1/key"), key);
        assert_eq!(read(&f, "0-1/authorized"), "1");
        assert_eq!(store.allowed().len(), 1);
    }

    #[test]
    fn at_boot_only_a_device_that_can_prove_itself_comes_back() {
        let f = Fake::new("boot");
        f.domain(0, "secure")
            .device("0-1", DOCK, "Dock", 0)
            .file("0-1/key", "")
            .device("0-3", "00000000-0000-0000-0000-000000000003", "Drive", 0);
        let store = Store::new(f.root());
        carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Always)).unwrap();
        carry_out(
            f.root(),
            &Verb::Allow("00000000-0000-0000-0000-000000000003".into(), Keep::Always),
        )
        .unwrap();
        assert_eq!(store.allowed().len(), 2);
        // Unplugged and plugged back in.
        f.file("0-1/authorized", "0").file("0-3/authorized", "0");
        carry_out(f.root(), &Verb::Boot).unwrap();
        assert_eq!(read(&f, "0-1/authorized"), "2", "challenged with its key");
        assert_eq!(
            read(&f, "0-3/authorized"),
            "0",
            "a UUID alone waits for a session"
        );
    }

    #[test]
    fn reconnect_is_only_for_what_was_allowed_always() {
        let f = Fake::new("reconnect");
        f.domain(0, "user").device("0-1", DOCK, "Dock", 0);
        assert!(carry_out(f.root(), &Verb::Reconnect(DOCK.into())).is_err());
        assert_eq!(read(&f, "0-1/authorized"), "0");
        carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Always)).unwrap();
        f.file("0-1/authorized", "0");
        carry_out(f.root(), &Verb::Reconnect(DOCK.into())).unwrap();
        assert_eq!(read(&f, "0-1/authorized"), "1");
        carry_out(f.root(), &Verb::Forget(DOCK.into())).unwrap();
        f.file("0-1/authorized", "0");
        assert!(carry_out(f.root(), &Verb::Reconnect(DOCK.into())).is_err());
    }

    #[test]
    fn a_port_the_firmware_decides_for_is_not_touched() {
        let f = Fake::new("none");
        f.domain(0, "none").device("0-1", DOCK, "Dock", 0);
        assert!(carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Always)).is_err());
        assert!(Store::new(f.root()).allowed().is_empty());
    }

    #[test]
    fn an_absent_device_is_an_error_not_a_guess() {
        let f = Fake::new("absent");
        f.domain(0, "user");
        assert!(carry_out(f.root(), &Verb::Allow(DOCK.into(), Keep::Once)).is_err());
    }
}

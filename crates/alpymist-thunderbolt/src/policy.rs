//! What to do about a device that is waiting to be let in, decided with no
//! files and no hardware, so every case can be tested.
//!
//! The rules, from ADR 0012:
//!
//! - Nothing is let in that nobody chose to let in.
//! - A device nobody has allowed is asked about, and only while someone is
//!   at an unlocked desktop to answer. Never at the login screen or while
//!   the session is locked.
//! - A device allowed always is let in again without asking. If it proved
//!   who it is with its key, that holds at any time, at boot too. If it can
//!   only give a UUID, which another device could copy, it waits for an
//!   unlocked session like a new one would.

use crate::store::Allowed;
use crate::sysfs::{Device, Domain, Level};

/// Where the decision is being made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// Nobody is logged in: boot, udev, the login screen.
    None,
    /// A session is running and its lock screen is up.
    Locked,
    /// Someone is at an unlocked desktop.
    Unlocked,
}

/// How a device is let in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Write `1` to `authorized`: the device is taken at its word.
    Plain,
    /// Write a fresh key to `key`, then `1`: the device stores the key, and
    /// can prove who it is from then on.
    StoreKey,
    /// Write the stored key to `key`, then `2`: the device must answer a
    /// challenge against it.
    Challenge,
}

/// What to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing: it is in already, nothing needs approving, or now is not
    /// the time.
    Nothing,
    /// Let it in again, as allowed always.
    Reconnect(Method),
    /// Ask whoever is at the desktop.
    Ask,
}

/// What to do about `device` in `domain`, given whether it is allowed always
/// (`allowed`), whether root holds a key for it (`has_key`), and `session`.
#[must_use]
pub fn decide(
    device: &Device,
    domain: Option<&Domain>,
    allowed: Option<&Allowed>,
    has_key: bool,
    session: Session,
) -> Decision {
    let Some(level) = domain.and_then(|d| d.level) else {
        return Decision::Nothing;
    };
    if device.authorized || !level.asks() {
        return Decision::Nothing;
    }
    let verified = level == Level::Secure && device.keyed && has_key;
    match (allowed.is_some(), verified, session) {
        (true, true, _) => Decision::Reconnect(Method::Challenge),
        (true, false, Session::Unlocked) => Decision::Reconnect(Method::Plain),
        (false, _, Session::Unlocked) => Decision::Ask,
        _ => Decision::Nothing,
    }
}

/// How to let `device` in when someone has just said yes: with a key stored
/// in it where the domain and the device can both use one, so next time it
/// can prove who it is.
#[must_use]
pub fn first_method(device: &Device, domain: Option<&Domain>) -> Method {
    if domain.and_then(|d| d.level) == Some(Level::Secure) && device.keyed {
        Method::StoreKey
    } else {
        Method::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, Method, Session, decide, first_method};
    use crate::store::Allowed;
    use crate::sysfs::{Device, Domain, Level};
    use std::path::PathBuf;

    fn domain(level: Level) -> Domain {
        Domain {
            index: 0,
            level: Some(level),
            dma_protected: false,
        }
    }

    fn device(keyed: bool, authorized: bool) -> Device {
        Device {
            name: "0-1".into(),
            domain: 0,
            uuid: crate::sysfs::tests::DOCK.into(),
            vendor: "Lenovo".into(),
            model: "ThinkPad Thunderbolt 3 Dock".into(),
            authorized,
            keyed,
            generation: Some(3),
            path: PathBuf::new(),
        }
    }

    fn allowed() -> Allowed {
        Allowed {
            uuid: crate::sysfs::tests::DOCK.into(),
            vendor: String::new(),
            model: String::new(),
        }
    }

    const ALL: [Session; 3] = [Session::None, Session::Locked, Session::Unlocked];

    #[test]
    fn a_new_device_is_asked_about_only_at_an_unlocked_desktop() {
        let d = device(false, false);
        let dom = domain(Level::User);
        assert_eq!(
            decide(&d, Some(&dom), None, false, Session::Unlocked),
            Decision::Ask
        );
        assert_eq!(
            decide(&d, Some(&dom), None, false, Session::Locked),
            Decision::Nothing
        );
        assert_eq!(
            decide(&d, Some(&dom), None, false, Session::None),
            Decision::Nothing
        );
    }

    #[test]
    fn a_device_known_only_by_its_uuid_waits_for_an_unlocked_session() {
        let d = device(false, false);
        let dom = domain(Level::User);
        let a = allowed();
        assert_eq!(
            decide(&d, Some(&dom), Some(&a), false, Session::Unlocked),
            Decision::Reconnect(Method::Plain)
        );
        assert_eq!(
            decide(&d, Some(&dom), Some(&a), false, Session::Locked),
            Decision::Nothing
        );
        assert_eq!(
            decide(&d, Some(&dom), Some(&a), false, Session::None),
            Decision::Nothing
        );
    }

    #[test]
    fn a_device_that_proves_who_it_is_comes_back_at_any_time() {
        let d = device(true, false);
        let dom = domain(Level::Secure);
        let a = allowed();
        for s in ALL {
            assert_eq!(
                decide(&d, Some(&dom), Some(&a), true, s),
                Decision::Reconnect(Method::Challenge),
                "{s:?}"
            );
        }
    }

    #[test]
    fn a_secure_domain_without_the_key_is_no_better_than_a_uuid() {
        let a = allowed();
        let dom = domain(Level::Secure);
        // The device cannot take a key, or root has lost it.
        for (keyed, has_key) in [(false, true), (true, false)] {
            let d = device(keyed, false);
            assert_eq!(
                decide(&d, Some(&dom), Some(&a), has_key, Session::None),
                Decision::Nothing
            );
            assert_eq!(
                decide(&d, Some(&dom), Some(&a), has_key, Session::Unlocked),
                Decision::Reconnect(Method::Plain)
            );
        }
    }

    #[test]
    fn nothing_is_done_where_nothing_needs_approving() {
        let d = device(false, false);
        for level in [Level::None, Level::DpOnly, Level::UsbOnly, Level::NoPcie] {
            for s in ALL {
                assert_eq!(
                    decide(&d, Some(&domain(level)), None, false, s),
                    Decision::Nothing
                );
            }
        }
        // An unknown level, or no domain, is left alone rather than guessed at.
        assert_eq!(
            decide(&d, None, None, false, Session::Unlocked),
            Decision::Nothing
        );
    }

    #[test]
    fn a_device_already_in_is_left_alone() {
        let d = device(false, true);
        let dom = domain(Level::User);
        for s in ALL {
            assert_eq!(decide(&d, Some(&dom), None, false, s), Decision::Nothing);
        }
    }

    #[test]
    fn a_key_is_stored_where_both_sides_can_use_one() {
        assert_eq!(
            first_method(&device(true, false), Some(&domain(Level::Secure))),
            Method::StoreKey
        );
        assert_eq!(
            first_method(&device(false, false), Some(&domain(Level::Secure))),
            Method::Plain
        );
        assert_eq!(
            first_method(&device(true, false), Some(&domain(Level::User))),
            Method::Plain
        );
    }
}

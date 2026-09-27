//! The question asked when a device nobody has allowed is plugged in: what
//! it says and what every key and click does, with no pixels.

use crate::sysfs::{Device, Domain, Level};
use crate::system::Keep;

/// A button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Leave it out.
    Deny,
    /// Let it in until it is unplugged.
    Once,
    /// Let it in from now on.
    Always,
}

impl Target {
    /// Left to right, and in Tab order.
    pub const ALL: [Self; 3] = [Self::Deny, Self::Once, Self::Always];

    /// What the button says.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Deny => "Don't allow",
            Self::Once => "Allow once",
            Self::Always => "Always allow",
        }
    }
}

/// What the dialog decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Leave it out; do not ask again until it is plugged in again.
    Deny,
    /// Let it in, for so long.
    Allow(Keep),
}

/// What the window has to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Close with this answer.
    Done(Answer),
}

/// A key, as far as the dialog cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Tab or Right.
    Next,
    /// Shift+Tab or Left.
    Previous,
    /// Enter or Space: press what has focus.
    Activate,
    /// Escape.
    Cancel,
}

/// What the dialog says about one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The question, naming the device.
    pub title: String,
    /// Its vendor, its kind, and that it was just connected.
    pub subtitle: String,
    /// What letting it in means.
    pub risk: String,
    /// What Always means for this device.
    pub always: String,
    /// Whether `always` is a warning rather than reassurance.
    pub always_warns: bool,
}

impl Question {
    /// The question for `device` in `domain`.
    #[must_use]
    pub fn new(device: &Device, domain: Option<&Domain>) -> Self {
        let vendor = device.vendor.trim();
        let subtitle = if vendor.is_empty() {
            format!("{} · just connected", device.kind())
        } else {
            format!("{vendor} · {} · just connected", device.kind())
        };
        let protected = domain.is_some_and(|d| d.dma_protected);
        let risk = if protected {
            "Once allowed, it is connected to this computer as if it were inside it. \
             The IOMMU limits what it can reach, but allow only a device you trust."
        } else {
            "Once allowed, it is connected to this computer as if it were inside it, \
             and can read and change its memory. Allow only a device you trust."
        };
        let verified = domain.and_then(|d| d.level) == Some(Level::Secure) && device.keyed;
        let always = if verified {
            "It proves who it is with a key each time, so Always allow lets in only \
             this device, at startup too."
        } else {
            "This port cannot check who a device is: another device could pretend to be \
             this one, and Always allow would let it in too whenever the screen is unlocked."
        };
        Self {
            title: format!("Allow “{}”?", device.title()),
            subtitle,
            risk: risk.into(),
            always: always.into(),
            always_warns: !verified,
        }
    }
}

/// The dialog's state.
#[derive(Debug, Clone)]
pub struct Dialog {
    /// What it says.
    pub question: Question,
    hover: Option<Target>,
    focus: Option<Target>,
}

impl Dialog {
    /// The dialog for `question`, nothing hovered or focused: Enter alone
    /// allows nothing.
    #[must_use]
    pub fn new(question: Question) -> Self {
        Self {
            question,
            hover: None,
            focus: None,
        }
    }

    /// What the pointer is over.
    #[must_use]
    pub fn hover(&self) -> Option<Target> {
        self.hover
    }

    /// What has keyboard focus.
    #[must_use]
    pub fn focus(&self) -> Option<Target> {
        self.focus
    }

    /// The pointer moved over `target`, or off everything.
    pub fn hover_over(&mut self, target: Option<Target>) -> Outcome {
        if self.hover == target {
            return Outcome::Unchanged;
        }
        self.hover = target;
        Outcome::Redraw
    }

    /// `target` was clicked.
    #[must_use]
    pub fn click(&self, target: Target) -> Outcome {
        Outcome::Done(match target {
            Target::Deny => Answer::Deny,
            Target::Once => Answer::Allow(Keep::Once),
            Target::Always => Answer::Allow(Keep::Always),
        })
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        let n = Target::ALL.len();
        let at = self
            .focus
            .and_then(|t| Target::ALL.iter().position(|&x| x == t));
        match key {
            Key::Next => {
                self.focus = Some(Target::ALL[at.map_or(0, |i| (i + 1) % n)]);
                Outcome::Redraw
            }
            Key::Previous => {
                self.focus = Some(Target::ALL[at.map_or(n - 1, |i| (i + n - 1) % n)]);
                Outcome::Redraw
            }
            Key::Activate => self.focus.map_or(Outcome::Unchanged, |t| self.click(t)),
            Key::Cancel => Outcome::Done(Answer::Deny),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Answer, Dialog, Key, Outcome, Question, Target};
    use crate::sysfs::{Device, Domain, Level};
    use crate::system::Keep;
    use std::path::PathBuf;

    fn device(keyed: bool) -> Device {
        Device {
            name: "0-1".into(),
            domain: 0,
            uuid: crate::sysfs::tests::DOCK.into(),
            vendor: "Lenovo".into(),
            model: "ThinkPad Thunderbolt 3 Dock".into(),
            authorized: false,
            keyed,
            generation: Some(3),
            path: PathBuf::new(),
        }
    }

    fn domain(level: Level, dma_protected: bool) -> Domain {
        Domain {
            index: 0,
            level: Some(level),
            dma_protected,
        }
    }

    #[test]
    fn it_names_the_device_and_what_it_is() {
        let q = Question::new(&device(false), Some(&domain(Level::User, false)));
        assert_eq!(q.title, "Allow “ThinkPad Thunderbolt 3 Dock”?");
        assert_eq!(q.subtitle, "Lenovo · Thunderbolt 3 · just connected");
        assert!(q.risk.contains("memory"));
    }

    #[test]
    fn always_warns_unless_the_device_proves_who_it_is() {
        let uuid_only = Question::new(&device(false), Some(&domain(Level::User, false)));
        assert!(uuid_only.always_warns);
        assert!(uuid_only.always.contains("pretend"));
        let no_key = Question::new(&device(false), Some(&domain(Level::Secure, false)));
        assert!(no_key.always_warns);
        let keyed = Question::new(&device(true), Some(&domain(Level::Secure, true)));
        assert!(!keyed.always_warns);
        assert!(keyed.risk.contains("IOMMU"));
    }

    #[test]
    fn enter_alone_allows_nothing_and_escape_denies() {
        let q = Question::new(&device(false), None);
        let mut d = Dialog::new(q);
        assert_eq!(d.key(Key::Activate), Outcome::Unchanged);
        assert_eq!(d.key(Key::Cancel), Outcome::Done(Answer::Deny));
    }

    #[test]
    fn the_keys_go_round_the_buttons() {
        let mut d = Dialog::new(Question::new(&device(false), None));
        d.key(Key::Next);
        assert_eq!(d.focus(), Some(Target::Deny));
        d.key(Key::Next);
        d.key(Key::Next);
        assert_eq!(
            d.key(Key::Activate),
            Outcome::Done(Answer::Allow(Keep::Always))
        );
        d.key(Key::Next);
        assert_eq!(d.focus(), Some(Target::Deny));
        d.key(Key::Previous);
        assert_eq!(d.focus(), Some(Target::Always));
        assert_eq!(
            d.click(Target::Once),
            Outcome::Done(Answer::Allow(Keep::Once))
        );
    }
}

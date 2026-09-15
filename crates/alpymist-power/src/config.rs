//! What the user has chosen: what the bar shows, and what closing the lid and
//! pressing the power button do.
//!
//! One small TOML file, `$XDG_CONFIG_HOME/alpymist/power.toml`, which the
//! popup writes and anyone may edit. A missing file, or a missing key, is the
//! default; a file that does not parse is reported and the defaults used, so
//! the lid still does something sensible.

use serde::Deserialize;
use std::path::PathBuf;

/// What closing the lid or pressing the power button can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Nothing at all.
    Nothing,
    /// Lock the screen.
    Lock,
    /// Lock the screen, then suspend to memory.
    Suspend,
    /// Lock the screen, then hibernate to disk.
    Hibernate,
    /// Shut down.
    PowerOff,
    /// Open the power menu, to choose. The power button only.
    Menu,
}

impl Action {
    /// What the lid can be set to, in the order stepped through.
    pub const LID: [Self; 4] = [Self::Suspend, Self::Lock, Self::Nothing, Self::Hibernate];
    /// What the power button can be set to, in the order stepped through.
    pub const BUTTON: [Self; 4] = [Self::Menu, Self::Suspend, Self::PowerOff, Self::Nothing];

    /// The name in the file.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Lock => "lock",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::PowerOff => "power-off",
            Self::Menu => "menu",
        }
    }

    /// The name shown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Nothing => "Do nothing",
            Self::Lock => "Lock",
            Self::Suspend => "Suspend",
            Self::Hibernate => "Hibernate",
            Self::PowerOff => "Shut down",
            Self::Menu => "Show power menu",
        }
    }

    /// The next in `order` after this one, wrapping; `by` -1 for the one
    /// before.
    #[must_use]
    pub fn step(self, order: &[Self], by: isize) -> Self {
        let len = order.len().cast_signed();
        if len == 0 {
            return self;
        }
        let at = order.iter().position(|a| *a == self).map_or(0, |i| {
            (i.cast_signed() + by).rem_euclid(len).cast_unsigned()
        });
        order[at]
    }
}

/// What the bar shows beside the battery icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Bar {
    /// The charge, `78%`.
    pub percentage: bool,
    /// Time until empty or full, `3:12`.
    pub time: bool,
    /// Power in or out, `7.4 W`.
    pub power: bool,
    /// The power mode's icon, when not Balanced.
    pub profile: bool,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            percentage: true,
            time: false,
            power: false,
            profile: false,
        }
    }
}

/// What the lid and the power button do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Actions {
    /// Closing the lid on battery.
    pub lid: Action,
    /// Closing the lid on the charger.
    pub lid_on_power: Action,
    /// Closing the lid with another screen connected.
    pub lid_docked: Action,
    /// Pressing the power button.
    pub power_button: Action,
}

impl Default for Actions {
    fn default() -> Self {
        Self {
            lid: Action::Suspend,
            lid_on_power: Action::Suspend,
            lid_docked: Action::Nothing,
            power_button: Action::Menu,
        }
    }
}

/// The commands actions run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Commands {
    /// Locks the screen, and returns once it is locked.
    pub lock: String,
    /// Opens the power menu.
    pub menu: String,
}

impl Default for Commands {
    fn default() -> Self {
        Self {
            // -f returns once the lock is up, which is what suspending after
            // it needs.
            lock: "swaylock -f -c 0b121e".into(),
            menu: "alpymist-menu system".into(),
        }
    }
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// What the bar shows.
    pub bar: Bar,
    /// What the lid and power button do.
    pub actions: Actions,
    /// The commands those run.
    pub commands: Commands,
}

/// Where the file is.
#[must_use]
pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|d| !d.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("alpymist").join("power.toml"))
}

impl Config {
    /// Parse a file's text.
    ///
    /// # Errors
    /// When it is not TOML, or names something unknown.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Read the user's file, or the defaults. A file that does not parse is
    /// reported on stderr.
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                eprintln!("alpymist-power: {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// The file's text.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let quote = |s: &str| {
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        };
        let Self {
            bar,
            actions,
            commands,
        } = self;
        format!(
            "# Alpymist power: the battery popup writes this file; edit it too.\n\
             # Actions: nothing, lock, suspend, hibernate, power-off, menu.\n\
             \n\
             [bar]\n\
             # Beside the battery icon.\n\
             percentage = {}\n\
             time = {}\n\
             power = {}\n\
             profile = {}\n\
             \n\
             [actions]\n\
             lid = {}\n\
             lid_on_power = {}\n\
             # With another screen connected.\n\
             lid_docked = {}\n\
             power_button = {}\n\
             \n\
             [commands]\n\
             lock = {}\n\
             menu = {}\n",
            bar.percentage,
            bar.time,
            bar.power,
            bar.profile,
            quote(actions.lid.id()),
            quote(actions.lid_on_power.id()),
            quote(actions.lid_docked.id()),
            quote(actions.power_button.id()),
            quote(&commands.lock),
            quote(&commands.menu),
        )
    }

    /// Write the user's file, atomically.
    ///
    /// # Errors
    /// No home, or the file could not be written.
    pub fn save(&self) -> Result<(), String> {
        let path = path().ok_or("there is no home directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let temporary = path.with_extension("toml.new");
        std::fs::write(&temporary, self.to_toml())
            .and_then(|()| std::fs::rename(&temporary, &path))
            .map_err(|e| format!("could not save {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Config};

    #[test]
    fn what_is_written_reads_back() {
        let mut c = Config::default();
        c.bar.time = true;
        c.actions.lid_on_power = Action::Nothing;
        c.commands.lock = "swaylock -f -c \"#000\"".into();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }

    #[test]
    fn an_empty_file_is_the_defaults_and_a_typo_is_an_error() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        assert!(Config::parse("[actions]\nlid = \"explode\"\n").is_err());
        assert!(
            Config::parse("[bar]\ntime = true\n")
                .unwrap()
                .bar
                .percentage,
            "keys left out keep their defaults"
        );
    }

    #[test]
    fn stepping_wraps_both_ways() {
        assert_eq!(Action::Suspend.step(&Action::LID, -1), Action::Hibernate);
        assert_eq!(Action::Hibernate.step(&Action::LID, 1), Action::Suspend);
        assert_eq!(Action::Menu.step(&Action::LID, 1), Action::Suspend);
    }
}

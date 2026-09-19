//! Every Alpymist setting, once.
//!
//! The `alpymist` command, the settings app and the popups under the bar all
//! read and change settings through this library, so each setting has one
//! implementation (ADR 0007). A setting has an id (`touchpad.natural-scroll`),
//! a kind of value, a scope (the account's or the system's) and a note on when
//! a change shows. The [`areas`] know where each lives and how to apply it.
//!
//! ```no_run
//! use alpymist_settings::{Env, Settings};
//!
//! let env = Env::detect();
//! let settings = Settings::new();
//! let outcome = settings.set(&env, "touchpad.natural-scroll", "on", false);
//! ```
//!
//! A system setting refuses to be written by anyone but root:
//! [`Error::NeedsRoot`] tells the caller to run `pkexec alpymist set …`, and
//! then to call [`Settings::live`] itself, since root cannot reach the
//! person's session.

#![forbid(unsafe_code)]

mod areas;
pub mod env;
pub mod generated;
pub mod menu;
pub mod model;
pub mod values;

pub use env::Env;
pub use model::{Applies, Area, Choice, Kind, Scope, Setting, Value};

use std::fmt;
use std::path::Path;

/// Why a setting could not be read or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No setting has that id.
    Unknown(String),
    /// Not a value the setting takes.
    Invalid(String),
    /// A system setting, and this process is not root.
    NeedsRoot(String),
    /// Anything else: a file, a program, a device.
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "no setting `{id}`; `alpymist list` shows them all"),
            Self::Invalid(why) | Self::Failed(why) => f.write_str(why),
            Self::NeedsRoot(id) => write!(f, "{id} is a system setting and needs root"),
        }
    }
}

impl std::error::Error for Error {}

/// What a change did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The value now set.
    pub value: Value,
    /// Anything worth telling: when it shows, what could not be done.
    pub notes: Vec<String>,
}

/// An I/O error with its path.
pub(crate) fn io_error(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// The registry of every area and setting.
pub struct Settings {
    settings: Vec<Setting>,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

impl Settings {
    /// Every setting Alpymist has.
    #[must_use]
    pub fn new() -> Self {
        Self {
            settings: areas::all(),
        }
    }

    /// Every area, in order: the pages, and one for each screensaver installed.
    ///
    /// What resolves a setting's id, and what the command line lists. The app's
    /// side list is [`Settings::pages`], which is this without the
    /// screensavers'.
    #[must_use]
    pub fn areas(&self) -> &'static [Area] {
        areas::areas()
    }

    /// The areas the app lists down its side, in order.
    #[must_use]
    pub fn pages(&self) -> &'static [Area] {
        areas::pages()
    }

    /// One area per screensaver installed, in the order they are offered.
    ///
    /// Each holds a screensaver's own settings. They are reached from the
    /// Screensaver page rather than from the side list, so the app asks for
    /// them by name rather than finding them among the pages.
    #[must_use]
    pub fn screensavers(&self) -> &'static [Area] {
        areas::screensaver::areas()
    }

    /// The area holding the settings of the screensaver `show` names, if it
    /// names one that is installed.
    ///
    /// `show` is the value of `screensaver.show`: a screensaver's id, or
    /// `random`, which names all of them and so none in particular.
    #[must_use]
    pub fn screensaver(&self, show: &str) -> Option<&'static Area> {
        areas::screensaver::area_of(show)
    }

    /// Every setting, in order.
    #[must_use]
    pub fn all(&self) -> &[Setting] {
        &self.settings
    }

    /// The setting with `id`.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }

    /// The area with `id`.
    #[must_use]
    pub fn area(&self, id: &str) -> Option<&'static Area> {
        areas::areas().iter().find(|a| a.id == id)
    }

    /// The settings in an area.
    pub fn in_area<'a>(&'a self, area: &'a str) -> impl Iterator<Item = &'a Setting> + 'a {
        self.settings.iter().filter(move |s| s.area() == area)
    }

    fn setting(&self, id: &str) -> Result<&Setting, Error> {
        self.find(id).ok_or_else(|| Error::Unknown(id.to_owned()))
    }

    /// A setting's current value.
    ///
    /// # Errors
    /// An unknown id, or the value could not be read.
    pub fn get(&self, env: &Env, id: &str) -> Result<Value, Error> {
        let s = self.setting(id)?;
        areas::get(env, s).map_err(Error::Failed)
    }

    /// Set a setting from text, as typed on the command line.
    ///
    /// Writes the files; does not touch the running session (see
    /// [`Settings::live`]). `force` replaces a generated file edited by hand.
    ///
    /// # Errors
    /// An unknown id, a value it does not take, a system setting without
    /// root, or the change could not be made.
    pub fn set(&self, env: &Env, id: &str, text: &str, force: bool) -> Result<Changed, Error> {
        let s = self.setting(id)?;
        let value = s.parse(text).map_err(Error::Invalid)?;
        Self::write(env, s, Some(value), force)
    }

    /// Put a setting back to its default.
    ///
    /// # Errors
    /// As [`Settings::set`].
    pub fn reset(&self, env: &Env, id: &str, force: bool) -> Result<Changed, Error> {
        let s = self.setting(id)?;
        Self::write(env, s, None, force)
    }

    fn write(env: &Env, s: &Setting, value: Option<Value>, force: bool) -> Result<Changed, Error> {
        if s.scope == Scope::System && !env.is_root {
            return Err(Error::NeedsRoot(s.id.to_owned()));
        }
        let mut notes = areas::set(env, s, value.as_ref(), force).map_err(Error::Failed)?;
        notes.extend(s.applies.note().map(str::to_owned));
        Ok(Changed {
            value: value.unwrap_or_else(|| s.default.clone()),
            notes,
        })
    }

    /// Show a change in the person's running session, where it can be shown
    /// at once: Hyprland's input settings and keyboard layout.
    ///
    /// # Errors
    /// An unknown id, or the session refused.
    pub fn live(&self, env: &Env, id: &str, value: &Value) -> Result<(), Error> {
        let s = self.setting(id)?;
        areas::live(env, s, value).map_err(Error::Failed)
    }

    /// What a session needs before its compositor reads its configuration.
    ///
    /// # Errors
    /// A file could not be written.
    pub fn prepare_session(&self, env: &Env) -> Result<(), Error> {
        areas::input::prepare_session(env).map_err(Error::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Env, Error, Scope, Settings, Value};
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[test]
    fn ids_are_unique_and_in_known_areas() {
        let settings = Settings::new();
        let mut seen = HashSet::new();
        for s in settings.all() {
            assert!(seen.insert(s.id), "{} twice", s.id);
            assert!(settings.area(s.area()).is_some(), "{} has no area", s.id);
            assert!(
                s.parse(&s.default.to_string()).is_ok(),
                "{}'s default",
                s.id
            );
        }
        for a in settings.areas() {
            assert!(settings.in_area(a.id).next().is_some(), "{} is empty", a.id);
        }
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("alpymist-settings-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn an_account_setting_writes_its_values_and_hyprlands_file() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("account");
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();
        let changed = settings
            .set(&env, "touchpad.natural-scroll", "on", false)
            .unwrap();
        assert_eq!(changed.value, Value::Bool(true));
        assert_eq!(
            settings.get(&env, "touchpad.natural-scroll"),
            Ok(Value::Bool(true))
        );
        settings
            .live(&env, "touchpad.natural-scroll", &changed.value)
            .unwrap();
        let conf = std::fs::read_to_string(env.account("alpymist/hypr/settings.conf")).unwrap();
        assert!(conf.contains("natural_scroll = true"), "{conf}");
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            ["hyprctl keyword input:touchpad:natural_scroll true"]
        );

        // Edited by hand: left alone, and the value is not changed either.
        std::fs::write(env.account("alpymist/hypr/settings.conf"), "input { }\n").unwrap();
        let refused = settings.set(&env, "mouse.speed", "50", false);
        assert!(
            matches!(refused, Err(Error::Failed(ref m)) if m.contains("by hand")),
            "{refused:?}"
        );
        assert_eq!(settings.get(&env, "mouse.speed"), Ok(Value::Number(0)));
        settings.set(&env, "mouse.speed", "50", true).unwrap();
        assert_eq!(settings.get(&env, "mouse.speed"), Ok(Value::Number(50)));

        settings
            .reset(&env, "touchpad.natural-scroll", false)
            .unwrap();
        assert_eq!(
            settings.get(&env, "touchpad.natural-scroll"),
            Ok(Value::Bool(false))
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_system_setting_needs_root_and_takes_over_the_installers_files() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("system");
        let settings = Settings::new();
        let layout = settings.find("keyboard.layout").unwrap();
        assert_eq!(layout.scope, Scope::System);
        assert_eq!(
            settings.set(
                &Env::test(&d, false, &RAN),
                "keyboard.layout",
                "no-mac",
                false
            ),
            Err(Error::NeedsRoot("keyboard.layout".into()))
        );

        let env = Env::test(&d, true, &RAN);
        let etc = env.system("etc/alpymist");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(
            etc.join("session.env"),
            "XKB_DEFAULT_LAYOUT=gb\nXKB_DEFAULT_VARIANT=\n",
        )
        .unwrap();
        std::fs::write(
            etc.join("hyprland-keyboard.conf"),
            "# Written by the Alpymist installer: the keyboard layout chosen at install.\n\
             input {\n    kb_layout = gb\n    kb_variant = \n}\n",
        )
        .unwrap();
        assert_eq!(
            settings.get(&env, "keyboard.layout"),
            Ok(Value::Text("gb".into()))
        );

        settings
            .set(&env, "keyboard.layout", "no-mac", false)
            .unwrap();
        let hypr = std::fs::read_to_string(etc.join("hyprland-keyboard.conf")).unwrap();
        assert!(
            hypr.contains("kb_layout = no\n    kb_variant = mac\n"),
            "{hypr}"
        );
        let session = std::fs::read_to_string(etc.join("session.env")).unwrap();
        assert!(
            session.ends_with("XKB_DEFAULT_LAYOUT=no\nXKB_DEFAULT_VARIANT=mac\n"),
            "{session}"
        );
        assert_eq!(
            settings.get(&env, "keyboard.layout"),
            Ok(Value::Text("no-mac".into()))
        );
        assert!(
            RAN.lock()
                .unwrap()
                .contains(&"setup-keymap no no-mac".to_owned())
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_theme_keeps_what_it_is_not_told() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("theme");
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();
        settings
            .set(&env, "appearance.scheme", "light", false)
            .unwrap();
        settings
            .set(&env, "appearance.text-size", "18", false)
            .unwrap();
        assert_eq!(
            settings.get(&env, "appearance.scheme"),
            Ok(Value::Text("light".into()))
        );
        assert_eq!(
            settings.get(&env, "appearance.text-size"),
            Ok(Value::Number(18))
        );
        assert!(
            settings
                .set(&env, "appearance.accent", "pink", false)
                .is_err()
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

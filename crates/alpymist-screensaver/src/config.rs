//! When the screensaver comes on, which one, and when the screen goes off.
//!
//! One small TOML file, `$XDG_CONFIG_HOME/alpymist/screensaver.toml`, which
//! Settings writes and anyone may edit. A missing file, or a missing key, is
//! the default; a file that does not parse is reported and the defaults used,
//! so a typo cannot leave a laptop with a screen that never turns off.
//!
//! What a *particular* screensaver has been set to is not here. That belongs to
//! the screensaver, is declared by its [`crate::definition::Definition`], and
//! lives in a file of its own — see [`crate::values`]. This file is the policy:
//! when, and which.

use crate::picture::Show;
use serde::Deserialize;
use std::path::PathBuf;

/// The account's screensaver file, under the configuration directory.
pub const FILE: &str = "alpymist/screensaver.toml";

/// Where the file lives for this account.
#[must_use]
pub fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(FILE)
}

/// The largest timeout offered.
///
/// An hour, not a working day. The eight hours this allowed at first made the
/// setting unusable: everything anyone actually chooses lives in the first ten
/// minutes, so Settings drew both sliders with the handle jammed against the
/// left stop and no way to tell five minutes from ten. Past an hour, "never" —
/// which is what zero is — is what people mean anyway.
pub const MAX_MINUTES: u32 = 60;

/// The policy: when the screensaver comes on, and what happens after.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct Config {
    /// Minutes of stillness before the screensaver appears. Zero never shows it.
    pub after: u32,
    /// Minutes of stillness before the screen turns off. Zero leaves it on.
    ///
    /// Counted from the last input, not from when the screensaver appeared, so
    /// it is the number people mean when they say "the screen turns off after
    /// ten minutes".
    pub blank_after: u32,
    /// Lock the screen when it turns off.
    pub lock: bool,
    /// Which screensaver, by name, or `random`.
    #[serde(deserialize_with = "read_show")]
    pub show: Show,

    /// How chunky the picture was, when there was only ever one picture.
    ///
    /// Read and ignored. It moved into the screensaver that draws it, where
    /// anything about a particular picture belongs, but `deny_unknown_fields`
    /// would otherwise refuse a whole file written before it moved — and a
    /// refused file means a laptop whose screen stops turning off, over a key
    /// that no longer matters.
    #[serde(default)]
    pub(crate) block: Option<i64>,
}

/// Read `show` as a name, whatever it names.
///
/// No check that such a screensaver is installed: they are packages, one named
/// here may be about to be installed or may have just been removed, and this
/// file is read by things that have no business scanning a directory for it.
/// Resolving the name is the launcher's job, at the moment it matters.
fn read_show<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Show, D::Error> {
    use serde::Deserialize as _;
    Ok(Show::of(&String::deserialize(d)?))
}

impl Default for Config {
    /// Five minutes to the screensaver, ten to darkness, and no lock.
    ///
    /// Not locking by default is deliberate: a screensaver is a picture, not a
    /// guard, and a machine that starts asking for a password when nobody chose
    /// that is a machine people turn the feature off on. Settings offers the
    /// switch to anyone who wants it.
    fn default() -> Self {
        Self {
            after: 5,
            blank_after: 10,
            lock: false,
            show: Show::default(),
            block: None,
        }
    }
}

impl Config {
    /// Read the account's file. A missing file is the defaults.
    ///
    /// # Errors
    /// When the file is there but cannot be read or does not parse.
    pub fn load() -> Result<Self, String> {
        Self::read(&path())
    }

    /// Read a named file, for tests and for a caller with its own idea of home.
    ///
    /// # Errors
    /// When the file is there but cannot be read or does not parse.
    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Parse the file's text.
    ///
    /// # Errors
    /// When it is not the TOML this expects.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str::<Self>(text)
            .map_err(|e| e.message().to_owned())
            .map(Self::sane)
    }

    /// The same settings with anything out of range brought back into it.
    ///
    /// Clamped rather than refused: an hour and a half is a plausible thing to
    /// have meant by a number this side of a day.
    #[must_use]
    fn sane(self) -> Self {
        Self {
            after: self.after.min(MAX_MINUTES),
            blank_after: self.blank_after.min(MAX_MINUTES),
            ..self
        }
    }

    /// The file this would be written as.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let Self {
            after,
            blank_after,
            lock,
            show,
            ..
        } = self;
        let show = show.id();
        format!(
            "# Alpymist screensaver: Settings writes this file; edit it too.\n\
             # Times are in minutes, and zero means never.\n\
             \n\
             # Before the screensaver appears.\n\
             after = {after}\n\
             # Before the screen turns off, counted from the last key or click.\n\
             blank-after = {blank_after}\n\
             # Ask for the password when the screen turns off.\n\
             lock = {lock}\n\
             # Which screensaver, by name, or \"random\" for a different one each\n\
             # time. Each has its own settings, in its own file beside this one.\n\
             show = \"{show}\"\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::picture::Show;

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let missing = std::env::temp_dir().join("alpymist-no-such-screensaver.toml");
        assert_eq!(Config::read(&missing).unwrap(), Config::default());
    }

    #[test]
    fn keys_are_read_as_written_in_the_file() {
        let c = Config::parse("after = 2\nblank-after = 15\nlock = true\nshow = \"aquarium\"\n")
            .unwrap();
        assert_eq!(c.after, 2);
        assert_eq!(c.blank_after, 15);
        assert!(c.lock);
        assert_eq!(c.show, Show::One("aquarium".into()));
    }

    #[test]
    fn a_name_is_taken_as_written_even_for_a_screensaver_not_installed() {
        // It may be about to be installed, or may have just been removed; the
        // launcher says so, rather than this quietly drawing something else.
        let c = Config::parse("show = \"not-installed-yet\"\n").unwrap();
        assert_eq!(c.show, Show::One("not-installed-yet".into()));
    }

    #[test]
    fn a_typo_is_an_error_rather_than_a_setting_silently_ignored() {
        assert!(Config::parse("blank_after = 5\n").is_err(), "underscores");
        assert!(Config::parse("aftre = 5\n").is_err(), "a misspelling");
    }

    /// 0.0.7 wrote `block` here, before chunkiness became the mountains' own.
    /// Refusing the file would stop the screen turning off over a dead key.
    #[test]
    fn a_file_from_before_the_screensavers_were_separate_still_reads() {
        let old = "after = 5\nblank-after = 10\nlock = false\nblock = 6\n";
        let c = Config::parse(old).expect("an upgraded account's file still parses");
        assert_eq!(c.after, 5);
        assert_eq!(c.blank_after, 10);
    }

    #[test]
    fn what_a_hand_edit_can_put_out_of_range_comes_back_into_it() {
        let c = Config::parse("after = 99999\nblank-after = 99999\n").unwrap();
        assert_eq!(c.after, super::MAX_MINUTES);
        assert_eq!(c.blank_after, super::MAX_MINUTES);
    }

    #[test]
    fn what_it_writes_is_what_it_reads_back() {
        let c = Config {
            after: 3,
            blank_after: 7,
            lock: true,
            show: Show::One("mountains".into()),
            block: None,
        };
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        assert_eq!(
            Config::parse(&Config::default().to_toml()).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn what_it_writes_never_carries_the_key_that_moved_out() {
        assert!(
            !Config::default().to_toml().contains("block"),
            "writing it back would spread a dead key to every account"
        );
    }
}

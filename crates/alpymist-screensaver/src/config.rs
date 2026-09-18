//! What the user has chosen: when the mountains appear, when the screen goes
//! off, whether it locks, and how chunky the pixels are.
//!
//! One small TOML file, `$XDG_CONFIG_HOME/alpymist/screensaver.toml`, which
//! Settings writes and anyone may edit. A missing file, or a missing key, is
//! the default; a file that does not parse is reported and the defaults used,
//! so a typo cannot leave a laptop with a screen that never turns off.

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

/// The largest timeout offered: a whole working day, past which "never" is
/// what the user means.
pub const MAX_MINUTES: u32 = 480;

/// The chunkiest and finest the pixels go. One is no pixelation at all, which
/// is a legitimate choice and not worth forbidding.
pub const BLOCK: (u32, u32) = (1, 16);

/// Everything the screensaver and the idle watcher read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct Config {
    /// Minutes of stillness before the mountains appear. Zero never shows them.
    pub after: u32,
    /// Minutes of stillness before the screen turns off. Zero leaves it on.
    ///
    /// Counted from the last input, not from when the screensaver appeared, so
    /// it is the number people mean when they say "the screen turns off after
    /// ten minutes".
    pub blank_after: u32,
    /// Lock the screen when it turns off.
    pub lock: bool,
    /// How many physical pixels one drawn pixel covers.
    pub block: u32,
}

impl Default for Config {
    /// Five minutes to the mountains, ten to darkness, and no lock.
    ///
    /// Not locking by default is deliberate: this screensaver is a picture, not
    /// a guard, and a machine that starts asking for a password when nobody
    /// chose that is a machine people turn the feature off on. Settings offers
    /// the switch to anyone who wants it.
    fn default() -> Self {
        Self {
            after: 5,
            blank_after: 10,
            lock: false,
            // Six: chunky enough to read as a choice rather than as a screen
            // driven at the wrong resolution, fine enough that the ridges are
            // still ridges. A 1920x1080 panel draws 320x180.
            block: 6,
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
    /// Clamped rather than refused: a hand-edited file with `block = 400` in it
    /// should draw something, and an hour and a half is a plausible thing to
    /// have meant by a number this side of a day.
    #[must_use]
    fn sane(self) -> Self {
        Self {
            after: self.after.min(MAX_MINUTES),
            blank_after: self.blank_after.min(MAX_MINUTES),
            lock: self.lock,
            block: self.block.clamp(BLOCK.0, BLOCK.1),
        }
    }

    /// The file this would be written as.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let Self {
            after,
            blank_after,
            lock,
            block,
        } = self;
        format!(
            "# Alpymist screensaver: Settings writes this file; edit it too.\n\
             # Times are in minutes, and zero means never.\n\
             \n\
             # Before the mountains appear.\n\
             after = {after}\n\
             # Before the screen turns off, counted from the last key or click.\n\
             blank-after = {blank_after}\n\
             # Ask for the password when the screen turns off.\n\
             lock = {lock}\n\
             # Physical pixels to one drawn pixel: how chunky the picture is.\n\
             block = {block}\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

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
        let c = Config::parse("after = 2\nblank-after = 15\nlock = true\nblock = 8\n").unwrap();
        assert_eq!(
            c,
            Config {
                after: 2,
                blank_after: 15,
                lock: true,
                block: 8
            }
        );
    }

    #[test]
    fn a_typo_is_an_error_rather_than_a_setting_silently_ignored() {
        assert!(Config::parse("blank_after = 5\n").is_err(), "underscores");
        assert!(Config::parse("aftre = 5\n").is_err(), "a misspelling");
    }

    #[test]
    fn what_a_hand_edit_can_put_out_of_range_comes_back_into_it() {
        let c = Config::parse("after = 99999\nblock = 400\n").unwrap();
        assert_eq!(c.after, super::MAX_MINUTES);
        assert_eq!(c.block, super::BLOCK.1);
        assert_eq!(Config::parse("block = 0\n").unwrap().block, super::BLOCK.0);
    }

    #[test]
    fn what_it_writes_is_what_it_reads_back() {
        let c = Config {
            after: 3,
            blank_after: 7,
            lock: true,
            block: 6,
        };
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        assert_eq!(
            Config::parse(&Config::default().to_toml()).unwrap(),
            Config::default()
        );
    }
}

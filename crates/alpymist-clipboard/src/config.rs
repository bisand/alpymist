//! What Settings › Clipboard says: `$XDG_CONFIG_HOME/alpymist/clipboard.toml`.
//!
//! A missing file, or a missing key, is the default: no history at all. A
//! file that does not parse is the default too, so a typo turns the history
//! off rather than on.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The account's file, under the configuration directory.
pub const FILE: &str = "alpymist/clipboard.toml";

/// The fewest entries the history keeps.
pub const MIN_SIZE: u32 = 10;
/// The most.
pub const MAX_SIZE: u32 = 500;

/// Settings › Clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// Keep a history at all.
    pub history: bool,
    /// Keep it across logins, in a file only this account can read.
    pub remember: bool,
    /// How many entries, pinned ones not counted.
    pub size: u32,
    /// Forget all but pinned entries when the screen locks.
    pub clear_on_lock: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history: false,
            remember: false,
            size: 50,
            clear_on_lock: true,
        }
    }
}

impl Config {
    /// The file at `path`, or the defaults.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// This account's.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&path())
    }

    /// Write it to `path`.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string(self).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The size, kept within what is offered.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.size.clamp(MIN_SIZE, MAX_SIZE) as usize
    }
}

/// `$XDG_CONFIG_HOME/alpymist/clipboard.toml`.
#[must_use]
pub fn path() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config").join(FILE)
}

/// Where a remembered history is kept: `$XDG_STATE_HOME/alpymist/clipboard`.
#[must_use]
pub fn history_path() -> PathBuf {
    base("XDG_STATE_HOME", ".local/state").join("alpymist/clipboard")
}

/// The daemon's socket, in the session's runtime directory, which only this
/// account can enter.
#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join("alpymist-clipboard.sock"))
}

fn base(variable: &str, under_home: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(under_home)))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn nothing_or_nonsense_is_no_history() {
        let d = std::env::temp_dir().join(format!("alpymist-clip-config-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("clipboard.toml");
        assert!(!Config::load_from(&path).history);
        std::fs::write(&path, "history = maybe\n").unwrap();
        assert!(!Config::load_from(&path).history);
        let on = Config {
            history: true,
            size: 5,
            ..Config::default()
        };
        on.save_to(&path).unwrap();
        let back = Config::load_from(&path);
        assert!(back.history && back.clear_on_lock && !back.remember);
        assert_eq!(back.limit(), 10, "kept within what is offered");
        std::fs::remove_dir_all(d).ok();
    }
}

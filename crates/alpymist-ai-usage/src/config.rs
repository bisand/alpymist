//! Which providers are turned on, and how often they are asked.
//!
//! `~/.config/alpymist/ai-usage.toml`, written by `alpymist-ai-usage enable`
//! and its kin. Nothing is on until someone turns it on, and no key is ever
//! in this file: those are the keyring's ([`crate::secrets`]).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// The providers turned on, by id.
    pub enabled: Vec<String>,
    /// Minutes between askings, where a provider allows it that often.
    pub refresh_minutes: i64,
    /// The percentage used from which the bar warns.
    pub warn_at: u8,
    /// The battery percentage below which nothing is asked while unplugged.
    pub battery_floor: u8,
    /// Whether to notify when a provider passes `warn_at`, and again when it
    /// is nearly spent.
    pub notify: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: Vec::new(),
            refresh_minutes: 10,
            warn_at: 80,
            battery_floor: 20,
            notify: true,
        }
    }
}

/// The settings' file, under the account's configuration directory.
pub const FILE: &str = "alpymist/ai-usage.toml";

/// Where the settings are.
#[must_use]
pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join(FILE))
}

impl Config {
    /// The settings on this account, or the defaults where there are none or
    /// they do not parse.
    #[must_use]
    pub fn load() -> Self {
        path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    /// The settings in `path`, or the defaults where it is not there or does
    /// not parse.
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write them.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&path().ok_or("no home directory to keep settings in")?)
    }

    /// Write them to `path`.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = toml::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Whether provider `id` is turned on.
    #[must_use]
    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.iter().any(|e| e == id)
    }

    /// Turn `id` on or off. Returns whether that changed anything.
    pub fn set_enabled(&mut self, id: &str, on: bool) -> bool {
        let was = self.is_enabled(id);
        if on && !was {
            self.enabled.push(id.to_owned());
            self.enabled.sort();
        } else if !on {
            self.enabled.retain(|e| e != id);
        }
        was != on
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn nothing_is_on_until_turned_on() {
        let mut c = Config::default();
        assert!(c.enabled.is_empty());
        assert!(c.set_enabled("openrouter", true));
        assert!(!c.set_enabled("openrouter", true));
        assert!(c.set_enabled("claude", true));
        assert_eq!(c.enabled, ["claude", "openrouter"]);
        let text = toml::to_string(&c).unwrap();
        assert!(text.contains("refresh-minutes = 10"), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), c);
        assert!(c.set_enabled("claude", false));
        assert!(!c.is_enabled("claude"));
        // A file from before a setting existed still reads.
        assert_eq!(
            toml::from_str::<Config>(
                "enabled = [\"x\"]
"
            )
            .unwrap()
            .warn_at,
            80
        );
    }
}

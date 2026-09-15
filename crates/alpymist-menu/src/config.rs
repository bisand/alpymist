//! The menu's configuration: what is in it, and how it looks.
//!
//! One TOML file. Each submenu is a table under `[menu]`, named, with its
//! entries listed inline, one per line — flat and diffable, where nesting the
//! tree in TOML would bury the fourth level under `[[item.item.item]]`. An
//! entry either runs something or opens another menu by name:
//!
//! ```toml
//! [menu.root]
//! items = [
//!   { name = "Apps",   icon = "󰀻", menu = "apps" },
//!   { name = "System", icon = "󰐥", menu = "system" },
//! ]
//!
//! [menu.system]
//! title = "System"
//! items = [
//!   { name = "Lock", icon = "", exec = "swaylock -f" },
//! ]
//! ```
//!
//! `apps` is built in: every installed application, found from its desktop
//! entry. Defining a menu called `apps` replaces it.
//!
//! Where the file comes from, first found wins: `$XDG_CONFIG_HOME/alpymist/menu.toml`,
//! then `/etc/alpymist/menu.toml`, then the copy compiled into the binary. A
//! file that does not parse is reported on the menu itself and the next one
//! down is used, so a typo never leaves anyone without a way to shut down.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The configuration this binary was built with.
pub const DEFAULT: &str = include_str!("../menu.toml");

/// The name of the menu opened when none is asked for.
pub const ROOT: &str = "root";

/// The name of the built-in applications menu.
pub const APPS: &str = "apps";

/// The whole file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// How commands marked `terminal = true` are run.
    #[serde(default)]
    pub terminal: Option<Vec<String>>,
    /// Sizes, fonts and colours.
    #[serde(default)]
    pub appearance: Appearance,
    /// Every submenu, by name.
    #[serde(default)]
    pub menu: BTreeMap<String, MenuDef>,
}

/// One submenu.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuDef {
    /// Shown above the search field while this menu is open.
    #[serde(default)]
    pub title: Option<String>,
    /// The entries, in the order they are listed.
    #[serde(default)]
    pub items: Vec<ItemDef>,
}

/// One entry.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    /// What the entry is called.
    pub name: String,
    /// A glyph from the icon font, usually a Nerd Font symbol.
    #[serde(default)]
    pub icon: Option<String>,
    /// Dimmer text after the name.
    #[serde(default)]
    pub detail: Option<String>,
    /// Other words a search should find this entry by.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// A shell command to run.
    #[serde(default)]
    pub exec: Option<String>,
    /// The name of a menu to open.
    #[serde(default)]
    pub menu: Option<String>,
    /// Run `exec` in a terminal.
    #[serde(default)]
    pub terminal: bool,
    /// With `terminal`, wait for Enter before the terminal closes, so the
    /// output can be read.
    #[serde(default)]
    pub hold: bool,
}

/// Sizes, fonts and colours. Every field has a default.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Appearance {
    /// Font for names and the search field.
    pub font: String,
    /// Font for icons.
    pub icon_font: String,
    /// Text height in logical pixels.
    pub font_size: u16,
    /// Panel width in logical pixels.
    pub width: u32,
    /// How many entries show at once.
    pub rows: u32,
    /// Panel background.
    pub background: Colour,
    /// Panel border.
    pub border: Colour,
    /// Names and the search text.
    pub text: Colour,
    /// Details, hints, titles.
    pub dim: Colour,
    /// Icons, matched letters, the caret.
    pub accent: Colour,
    /// Behind the selected entry.
    pub selection: Colour,
}

impl Default for Appearance {
    /// Alpymist's theme (`theme.toml`), read once per process: what the menu's
    /// own `[appearance]` does not set comes from there, and so does every
    /// window that dresses itself like the menu.
    fn default() -> Self {
        static THEME: std::sync::OnceLock<alpymist_theme::ThemeFile> = std::sync::OnceLock::new();
        Self::from_theme(THEME.get_or_init(|| {
            // Tests and snapshots draw the same whatever this machine's theme is.
            if cfg!(test) {
                alpymist_theme::ThemeFile::default()
            } else {
                alpymist_theme::load().file
            }
        }))
    }
}

impl Appearance {
    /// The appearance a theme gives, with the menu's own sizes.
    #[must_use]
    pub fn from_theme(theme: &alpymist_theme::ThemeFile) -> Self {
        let c = theme.colours();
        Self {
            font: theme.font.clone(),
            icon_font: theme.icon_font.clone(),
            font_size: theme.font_size,
            width: 560,
            rows: 9,
            background: Colour(c.background),
            border: Colour(c.border),
            text: Colour(c.text),
            dim: Colour(c.dim),
            accent: Colour(c.accent),
            selection: Colour(c.selection),
        }
    }
}

/// A colour written `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour(pub [u8; 4]);

impl Colour {
    /// Parse `#rrggbb` or `#rrggbbaa`; the `#` is optional.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let alpha = if hex.len() == 8 { byte(6)? } else { 0xFF };
        Some(Self([byte(0)?, byte(2)?, byte(4)?, alpha]))
    }
}

impl<'de> Deserialize<'de> for Colour {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let text = String::deserialize(de)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("`{text}` is not a colour like #7fb8d9"))
        })
    }
}

impl Config {
    /// Parse and check a configuration.
    ///
    /// # Errors
    /// A message fit to show the user: TOML's own, with line and column, or
    /// the entry that is wrong and why.
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        config.check()?;
        Ok(config)
    }

    /// What `parse` cannot express in types.
    fn check(&self) -> Result<(), String> {
        if !self.menu.contains_key(ROOT) {
            return Err(format!("there is no [menu.{ROOT}]"));
        }
        for (menu, def) in &self.menu {
            for item in &def.items {
                let here = || format!("[menu.{menu}] \"{}\"", item.name);
                match (&item.exec, &item.menu) {
                    (Some(_), Some(_)) => {
                        return Err(format!("{}: has both exec and menu", here()));
                    }
                    (None, None) => {
                        return Err(format!("{}: needs exec or menu", here()));
                    }
                    (None, Some(target)) => {
                        if target != APPS && !self.menu.contains_key(target) {
                            return Err(format!("{}: there is no [menu.{target}]", here()));
                        }
                    }
                    (Some(_), None) => {}
                }
            }
        }
        Ok(())
    }
}

/// A configuration, and what went wrong finding it.
#[derive(Debug)]
pub struct Loaded {
    /// The configuration in use.
    pub config: Config,
    /// Where it came from, or `None` for the built-in copy.
    pub path: Option<PathBuf>,
    /// Files that were there and could not be used, with the reason.
    pub problems: Vec<String>,
}

/// Where a user's configuration lives.
#[must_use]
pub fn user_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("alpymist/menu.toml"))
}

/// The system-wide configuration.
pub const SYSTEM_PATH: &str = "/etc/alpymist/menu.toml";

/// Find and parse the configuration. Never fails; see the module docs.
#[must_use]
pub fn load() -> Loaded {
    let candidates = user_path()
        .into_iter()
        .chain(std::iter::once(PathBuf::from(SYSTEM_PATH)));
    load_from(candidates)
}

/// As [`load`], from a given list of places.
pub fn load_from(candidates: impl IntoIterator<Item = PathBuf>) -> Loaded {
    let mut problems = Vec::new();
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match Config::parse(&text) {
            Ok(config) => {
                return Loaded {
                    config,
                    path: Some(path),
                    problems,
                };
            }
            Err(e) => problems.push(format!("{}: {}", path.display(), first_line(&e))),
        }
    }
    Loaded {
        config: Config::parse(DEFAULT).unwrap_or_default(),
        path: None,
        problems,
    }
}

/// TOML errors quote the offending source over several lines; the menu has
/// room for one.
fn first_line(message: &str) -> String {
    let flat: Vec<&str> = message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('|') && !l.chars().all(|c| c == '^'))
        .collect();
    flat.join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Colour, Config, DEFAULT, load_from};

    #[test]
    fn the_built_in_configuration_is_valid() {
        let config = Config::parse(DEFAULT).expect("menu.toml must parse");
        assert!(config.menu["root"].items.len() > 3);
    }

    #[test]
    fn colours_parse_with_and_without_alpha() {
        assert_eq!(
            Colour::parse("#7fb8d9"),
            Some(Colour([0x7F, 0xB8, 0xD9, 0xFF]))
        );
        assert_eq!(
            Colour::parse("0b121ef0"),
            Some(Colour([0x0B, 0x12, 0x1E, 0xF0]))
        );
        assert_eq!(Colour::parse("#12345"), None);
        assert_eq!(Colour::parse("#gggggg"), None);
        assert_eq!(Colour::parse("#ææææ"), None);
    }

    #[test]
    fn an_entry_must_do_exactly_one_thing() {
        let both = r#"
            [menu.root]
            items = [{ name = "X", exec = "true", menu = "root" }]
        "#;
        assert!(Config::parse(both).unwrap_err().contains("both"));
        let neither = r#"
            [menu.root]
            items = [{ name = "X" }]
        "#;
        assert!(Config::parse(neither).unwrap_err().contains("needs"));
    }

    #[test]
    fn a_reference_to_a_missing_menu_is_caught() {
        let text = r#"
            [menu.root]
            items = [{ name = "X", menu = "nowhere" }]
        "#;
        assert!(Config::parse(text).unwrap_err().contains("nowhere"));
    }

    #[test]
    fn the_applications_menu_needs_no_definition() {
        let text = r#"
            [menu.root]
            items = [{ name = "Apps", menu = "apps" }]
        "#;
        assert!(Config::parse(text).is_ok());
    }

    #[test]
    fn a_typo_in_a_key_is_an_error_not_silence() {
        let text = r#"
            [menu.root]
            items = [{ name = "X", exce = "true" }]
        "#;
        assert!(Config::parse(text).is_err());
    }

    #[test]
    fn appearance_can_be_partly_overridden() {
        let text = r##"
            [appearance]
            rows = 5
            accent = "#ff0000"
            [menu.root]
        "##;
        let config = Config::parse(text).unwrap();
        assert_eq!(config.appearance.rows, 5);
        assert_eq!(config.appearance.accent, Colour([0xFF, 0, 0, 0xFF]));
        assert_eq!(config.appearance.width, 560);
    }

    #[test]
    fn a_broken_file_falls_through_and_is_reported() {
        let dir = std::env::temp_dir().join(format!("alpymist-menu-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.toml");
        std::fs::write(&broken, "[menu.root\nitems = 3").unwrap();
        let good = dir.join("good.toml");
        std::fs::write(&good, "[menu.root]\n").unwrap();

        let loaded = load_from([dir.join("missing.toml"), broken.clone(), good.clone()]);
        assert_eq!(loaded.path.as_deref(), Some(good.as_path()));
        assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
        assert!(loaded.problems[0].contains("broken.toml"));
        assert!(!loaded.problems[0].contains('\n'));

        let fallback = load_from([broken]);
        assert!(fallback.path.is_none());
        assert!(fallback.config.menu.contains_key("root"));
        std::fs::remove_dir_all(dir).ok();
    }
}

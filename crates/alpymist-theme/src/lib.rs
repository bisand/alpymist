//! Alpymist's theme: one small file for every Alpymist window.
//!
//! `theme.toml` says light or dark, which accent colour, and the fonts and
//! their size. It is read from the account (`~/.config/alpymist/theme.toml`),
//! then the system (`/etc/alpymist/theme.toml`), then the built-in default,
//! the first that parses winning whole (ADR 0007).
//!
//! What it becomes:
//!
//! - [`ThemeFile::denise`]: a Denise [`Theme`], for windows drawn with
//!   Denise's widgets.
//! - [`ThemeFile::colours`]: the six colours the hand-drawn windows (the menu,
//!   the popups, the store) paint with.
//!
//! ```toml
//! scheme = "dark"      # or "light"
//! accent = "mist"      # mist, fjord, moss, amber, heather, rose
//! font_size = 16
//! ```

#![forbid(unsafe_code)]

use denise::Color;
use denise::theme::{ColorScheme, Role, Theme};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The system-wide theme file.
pub const SYSTEM: &str = "/etc/alpymist/theme.toml";

/// Light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// Light text on the night sky: Alpymist's own look.
    #[default]
    Dark,
    /// Dark text on mist.
    Light,
}

impl Scheme {
    /// Both, dark first.
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];

    /// As the file and the command line write it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// As a person reads it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

/// The colour for focus, selection, icons and the main action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    /// The blue of distant ridges: Alpymist's own.
    #[default]
    Mist,
    /// Glacier water.
    Fjord,
    /// Lichen on stone.
    Moss,
    /// Late sun.
    Amber,
    /// Heather on the slopes.
    Heather,
    /// Alpenglow.
    Rose,
}

impl Accent {
    /// Every accent, Alpymist's first.
    pub const ALL: [Self; 6] = [
        Self::Mist,
        Self::Fjord,
        Self::Moss,
        Self::Amber,
        Self::Heather,
        Self::Rose,
    ];

    /// As the file and the command line write it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Mist => "mist",
            Self::Fjord => "fjord",
            Self::Moss => "moss",
            Self::Amber => "amber",
            Self::Heather => "heather",
            Self::Rose => "rose",
        }
    }

    /// As a person reads it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mist => "Mist",
            Self::Fjord => "Fjord",
            Self::Moss => "Moss",
            Self::Amber => "Amber",
            Self::Heather => "Heather",
            Self::Rose => "Rose",
        }
    }

    /// The colour, light enough to read on dark or dark enough on light.
    #[must_use]
    pub const fn colour(self, scheme: Scheme) -> Color {
        let (dark, light) = match self {
            Self::Mist => ((0x7F, 0xB8, 0xD9), (0x2F, 0x6F, 0x95)),
            Self::Fjord => ((0x6F, 0xC3, 0xB8), (0x1F, 0x7A, 0x70)),
            Self::Moss => ((0x9F, 0xCF, 0x9F), (0x3C, 0x7A, 0x3C)),
            Self::Amber => ((0xD9, 0xA0, 0x7F), (0x9A, 0x5A, 0x2F)),
            Self::Heather => ((0xB7, 0x9F, 0xD9), (0x6A, 0x4F, 0x9A)),
            Self::Rose => ((0xD9, 0x8F, 0x7F), (0xA1, 0x4A, 0x3A)),
        };
        let (r, g, b) = match scheme {
            Scheme::Dark => dark,
            Scheme::Light => light,
        };
        Color::rgb(r, g, b)
    }
}

macro_rules! parse_by_id {
    ($ty:ty, $what:literal) => {
        impl FromStr for $ty {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, String> {
                Self::ALL.into_iter().find(|v| v.id() == s).ok_or_else(|| {
                    let ids: Vec<&str> = Self::ALL.iter().map(|v| v.id()).collect();
                    format!("`{s}` is not {}: {}", $what, ids.join(", "))
                })
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.id())
            }
        }
    };
}
parse_by_id!(Scheme, "a scheme");
parse_by_id!(Accent, "an accent");

/// The smallest and largest text size, in logical pixels.
pub const FONT_SIZES: (u16, u16) = (12, 24);

/// What `theme.toml` holds. Every key is optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeFile {
    /// Light or dark.
    pub scheme: Scheme,
    /// The accent colour.
    pub accent: Accent,
    /// Text height in logical pixels.
    pub font_size: u16,
    /// The text face.
    pub font: String,
    /// The icon face.
    pub icon_font: String,
}

impl Default for ThemeFile {
    fn default() -> Self {
        Self {
            scheme: Scheme::Dark,
            accent: Accent::Mist,
            font_size: 16,
            font: "/usr/share/fonts/TTF/FiraSans-Regular.ttf".into(),
            icon_font: "/usr/share/fonts/nerd-fonts/SymbolsNerdFontMono-Regular.ttf".into(),
        }
    }
}

/// The colours a hand-drawn window paints with, as `[r, g, b, a]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colours {
    /// Panels and windows.
    pub background: [u8; 4],
    /// A panel's border.
    pub border: [u8; 4],
    /// Text.
    pub text: [u8; 4],
    /// Details and hints.
    pub dim: [u8; 4],
    /// Icons, focus, the caret.
    pub accent: [u8; 4],
    /// Behind what is selected.
    pub selection: [u8; 4],
}

/// The base surfaces and text of a scheme: page, recessed well, border and
/// selection, text, dim text.
const fn base(scheme: Scheme) -> [Color; 5] {
    match scheme {
        Scheme::Dark => [
            Color::rgb(0x0B, 0x12, 0x1E),
            Color::rgb(0x13, 0x1D, 0x2C),
            Color::rgb(0x3A, 0x4C, 0x63),
            Color::rgb(0xEA, 0xF0, 0xF6),
            Color::rgb(0x9A, 0xAB, 0xBD),
        ],
        Scheme::Light => [
            Color::rgb(0xF4, 0xF7, 0xFA),
            Color::rgb(0xE6, 0xEC, 0xF2),
            Color::rgb(0xC9, 0xD3, 0xDE),
            Color::rgb(0x0B, 0x12, 0x1E),
            Color::rgb(0x4E, 0x5F, 0x73),
        ],
    }
}

impl ThemeFile {
    /// Read a theme file's text.
    ///
    /// # Errors
    /// Not TOML, an unknown key, or a value that is not one of the choices.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut file: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        file.font_size = file.font_size.clamp(FONT_SIZES.0, FONT_SIZES.1);
        Ok(file)
    }

    /// As a file, with only what differs from the default written.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let default = Self::default();
        let mut out = String::from("# Alpymist's theme. Settings › Appearance writes this file.\n");
        let mut line = |key: &str, value: String| {
            let _ = writeln!(out, "{key} = {value}");
        };
        if self.scheme != default.scheme {
            line("scheme", format!("\"{}\"", self.scheme));
        }
        if self.accent != default.accent {
            line("accent", format!("\"{}\"", self.accent));
        }
        if self.font_size != default.font_size {
            line("font_size", self.font_size.to_string());
        }
        if self.font != default.font {
            line("font", toml::Value::String(self.font.clone()).to_string());
        }
        if self.icon_font != default.icon_font {
            line(
                "icon_font",
                toml::Value::String(self.icon_font.clone()).to_string(),
            );
        }
        out
    }

    /// A Denise theme: the scheme's surfaces, the accent as the primary and
    /// accent colours, and Alpymist's status colours.
    #[must_use]
    pub fn denise(&self) -> Theme {
        let [page, well, border, text, dim] = base(self.scheme);
        let accent = self.accent.colour(self.scheme);
        let (name, scheme, info, success, warning, error) = match self.scheme {
            Scheme::Dark => (
                "Alpymist dark",
                ColorScheme::Dark,
                Color::rgb(0x7F, 0xB8, 0xD9),
                Color::rgb(0x9F, 0xCF, 0x9F),
                Color::rgb(0xE8, 0xB0, 0x6A),
                Color::rgb(0xD9, 0x8F, 0x7F),
            ),
            Scheme::Light => (
                "Alpymist light",
                ColorScheme::Light,
                Color::rgb(0x2F, 0x6F, 0x95),
                Color::rgb(0x3C, 0x7A, 0x3C),
                Color::rgb(0x9A, 0x5A, 0x2F),
                Color::rgb(0xA1, 0x4A, 0x3A),
            ),
        };
        Theme::from_seeds(
            name, scheme, page, accent, dim, accent, border, info, success, warning, error,
        )
        .with_color(Role::Base200, well)
        .with_color(Role::Base300, border)
        .with_color(Role::BaseContent, text)
    }

    /// The colours the hand-drawn windows use. Dark and Mist are exactly the
    /// colours those windows had before there was a theme.
    #[must_use]
    pub fn colours(&self) -> Colours {
        let [page, _, selection, text, dim] = base(self.scheme);
        let accent = self.accent.colour(self.scheme);
        let rgba = |c: Color, a: u8| [c.r, c.g, c.b, a];
        Colours {
            // A little of the desktop shows through the menu's panel.
            background: rgba(page, 0xF2),
            border: rgba(accent, 0xFF),
            text: rgba(text, 0xFF),
            dim: rgba(dim, 0xFF),
            accent: rgba(accent, 0xFF),
            selection: rgba(selection, 0xFF),
        }
    }
}

/// The account's theme file: `$XDG_CONFIG_HOME/alpymist/theme.toml`, or under
/// `~/.config`.
#[must_use]
pub fn account_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("alpymist/theme.toml"))
}

/// A theme, where it came from, and what could not be read on the way.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The theme.
    pub file: ThemeFile,
    /// The file it was read from, or `None` for the built-in default.
    pub source: Option<PathBuf>,
    /// Files that exist but could not be read, with why.
    pub problems: Vec<String>,
}

/// Read the theme: the account's file, then the system's, then the default.
#[must_use]
pub fn load() -> Loaded {
    let candidates: Vec<PathBuf> = account_path()
        .into_iter()
        .chain(std::iter::once(PathBuf::from(SYSTEM)))
        .collect();
    load_from(&candidates)
}

/// Read the first of `paths` that parses.
#[must_use]
pub fn load_from(paths: &[PathBuf]) -> Loaded {
    let mut problems = Vec::new();
    for path in paths {
        match std::fs::read_to_string(path) {
            Ok(text) => match ThemeFile::parse(&text) {
                Ok(file) => {
                    return Loaded {
                        file,
                        source: Some(path.clone()),
                        problems,
                    };
                }
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    Loaded {
        file: ThemeFile::default(),
        source: None,
        problems,
    }
}

/// Write `file` to `path`, replacing it in one step.
///
/// # Errors
/// The directory cannot be made or the file written.
pub fn save(file: &ThemeFile, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("toml.new");
    std::fs::write(&tmp, file.to_toml())
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{Accent, Scheme, ThemeFile, load_from, save};
    use denise::theme::Role;

    #[test]
    fn the_default_keeps_the_colours_alpymist_had() {
        let c = ThemeFile::default().colours();
        assert_eq!(c.background, [0x0B, 0x12, 0x1E, 0xF2]);
        assert_eq!(c.accent, [0x7F, 0xB8, 0xD9, 0xFF]);
        assert_eq!(c.border, c.accent);
        assert_eq!(c.text, [0xEA, 0xF0, 0xF6, 0xFF]);
        assert_eq!(c.dim, [0x9A, 0xAB, 0xBD, 0xFF]);
        assert_eq!(c.selection, [0x3A, 0x4C, 0x63, 0xFF]);
    }

    #[test]
    fn a_file_says_only_what_it_changes_and_reads_back() {
        let file = ThemeFile {
            scheme: Scheme::Light,
            accent: Accent::Fjord,
            font_size: 18,
            ..ThemeFile::default()
        };
        let text = file.to_toml();
        assert!(text.contains("scheme = \"light\""), "{text}");
        assert!(!text.contains("font ="), "{text}");
        assert_eq!(ThemeFile::parse(&text), Ok(file));
        assert_eq!(
            ThemeFile::parse(&ThemeFile::default().to_toml()),
            Ok(ThemeFile::default())
        );
    }

    #[test]
    fn a_bad_value_is_an_error_and_sizes_are_kept_in_range() {
        assert!(ThemeFile::parse("accent = \"pink\"").is_err());
        assert!(ThemeFile::parse("colour = 1").is_err());
        assert_eq!(ThemeFile::parse("font_size = 99").unwrap().font_size, 24);
    }

    #[test]
    fn deniseui_gets_the_scheme_and_accent() {
        let dark = ThemeFile::default().denise();
        assert_eq!(dark.color(Role::Primary), Accent::Mist.colour(Scheme::Dark));
        let light = ThemeFile {
            scheme: Scheme::Light,
            ..ThemeFile::default()
        }
        .denise();
        assert!(light.color(Role::Base100).r > 0xE0);
        assert!(light.color(Role::BaseContent).r < 0x20);
        // Every surface's content colour is readable on it.
        for theme in [dark, light] {
            assert!(
                theme.validate(denise::theme::AA).is_ok(),
                "{}: {:?}",
                theme.name,
                theme.validate(denise::theme::AA)
            );
        }
    }

    #[test]
    fn the_first_file_that_parses_wins() {
        let dir = std::env::temp_dir().join(format!("alpymist-theme-{}", std::process::id()));
        let (bad, good) = (dir.join("bad.toml"), dir.join("good.toml"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&bad, "scheme = 3").unwrap();
        let light = ThemeFile {
            scheme: Scheme::Light,
            ..ThemeFile::default()
        };
        save(&light, &good).unwrap();
        let loaded = load_from(&[dir.join("missing.toml"), bad.clone(), good.clone()]);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(loaded.file, light);
        assert_eq!(loaded.source, Some(good));
        assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
    }
}

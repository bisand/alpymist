//! The store's configuration: where software comes from.
//!
//! One TOML file, found as the menu's is: `$XDG_CONFIG_HOME/alpymist/store.toml`,
//! then `/etc/alpymist/store.toml`, then the copy compiled into the binary. A
//! file that does not parse is reported in the store's status line and the
//! next one down is used.
//!
//! Every source is a `[[source]]` table with a `kind`. The fields a kind does
//! not use are an error rather than silence, so a `remote` written on an apk
//! source is caught instead of quietly doing nothing.

use alpymist_widget::Colour;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The configuration this binary was built with.
pub const DEFAULT: &str = include_str!("../store.toml");

/// The system-wide configuration.
pub const SYSTEM_PATH: &str = "/etc/alpymist/store.toml";

/// The file as written.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    store: Store,
    #[serde(default)]
    source: Vec<SourceDef>,
}

/// Settings for the store as a whole.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Store {
    /// Window width in logical pixels, where the compositor leaves it open.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            width: 1080,
            height: 720,
        }
    }
}

/// One `[[source]]` as written: every kind's fields, checked into a
/// [`Source`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDef {
    id: String,
    kind: String,
    label: Option<String>,
    icon: Option<String>,
    colour: Option<Colour>,
    #[serde(default = "yes")]
    enabled: bool,
    // flatpak
    remote: Option<String>,
    url: Option<String>,
    installation: Option<String>,
    appstream: Option<PathBuf>,
    // apk
    root: Option<PathBuf>,
    hide: Option<Vec<String>>,
    protect: Option<Vec<String>>,
}

fn yes() -> bool {
    true
}

/// A checked configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Settings for the store as a whole.
    pub store: Store,
    /// The sources, enabled ones only, in the order written.
    pub sources: Vec<Source>,
}

/// One source of software.
#[derive(Debug, Clone)]
pub struct Source {
    /// Its name on the command line.
    pub id: String,
    /// How it is shown.
    pub label: String,
    /// A glyph from the icon font.
    pub icon: String,
    /// Its badge's colour.
    pub colour: Colour,
    /// What kind of source it is, and that kind's settings.
    pub kind: Kind,
}

/// A kind of source.
#[derive(Debug, Clone)]
pub enum Kind {
    /// A Flatpak remote.
    Flatpak(Flatpak),
    /// The system's apk repositories.
    Apk(Apk),
}

/// A Flatpak remote's settings.
#[derive(Debug, Clone)]
pub struct Flatpak {
    /// The remote's name.
    pub remote: String,
    /// Where to add the remote from, when it is missing.
    pub url: Option<String>,
    /// The per-user installation, or the system's.
    pub installation: Installation,
    /// Where the remote's `AppStream` data is, when not where Flatpak keeps
    /// it: the directory holding `appstream.xml` and `icons`. For tests and
    /// previews.
    pub appstream: Option<PathBuf>,
}

/// A Flatpak installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installation {
    /// `--user`.
    User,
    /// `--system`.
    System,
}

impl Installation {
    /// The flag that picks it.
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::User => "--user",
            Self::System => "--system",
        }
    }
}

/// The apk source's settings.
#[derive(Debug, Clone)]
pub struct Apk {
    /// The root the system's apk database is under: `/`, or a copy of one.
    pub root: PathBuf,
    /// Packages left out of results unless named exactly.
    pub hide: Vec<Pattern>,
    /// Packages that cannot be removed.
    pub protect: Vec<Pattern>,
}

/// A package name pattern, where `*` stands for any run of characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(String);

impl Pattern {
    /// A pattern from its text.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    /// Whether `name` matches.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        glob(self.0.as_bytes(), name.as_bytes())
    }
}

/// `*`-only glob matching, iterative, so no pattern can make it slow.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((sp, st)) = star {
            p = sp + 1;
            t = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

impl Config {
    /// Parse and check a configuration.
    ///
    /// # Errors
    /// A message fit to show the user.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|e| first_line(&e.to_string()))?;
        let mut sources = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for def in file.source {
            if !seen.insert(def.id.clone()) {
                return Err(format!("two sources are called \"{}\"", def.id));
            }
            if !def.enabled {
                continue;
            }
            sources.push(check(def)?);
        }
        Ok(Self {
            store: file.store,
            sources,
        })
    }
}

fn check(def: SourceDef) -> Result<Source, String> {
    let here = format!("[[source]] \"{}\"", def.id);
    if def.id.is_empty()
        || !def
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(format!("{here}: an id is letters, digits and dashes"));
    }
    let unused = |name: &str, present: bool| {
        if present {
            Err(format!(
                "{here}: `{name}` is not a setting of kind \"{}\"",
                def.kind
            ))
        } else {
            Ok(())
        }
    };
    let kind = match def.kind.as_str() {
        "flatpak" => {
            unused("root", def.root.is_some())?;
            unused("hide", def.hide.is_some())?;
            unused("protect", def.protect.is_some())?;
            let installation = match def.installation.as_deref() {
                None | Some("user") => Installation::User,
                Some("system") => Installation::System,
                Some(other) => {
                    return Err(format!(
                        "{here}: installation is \"user\" or \"system\", not \"{other}\""
                    ));
                }
            };
            Kind::Flatpak(Flatpak {
                remote: def.remote.unwrap_or_else(|| def.id.clone()),
                url: def.url,
                installation,
                appstream: def.appstream,
            })
        }
        "apk" => {
            unused("remote", def.remote.is_some())?;
            unused("url", def.url.is_some())?;
            unused("installation", def.installation.is_some())?;
            unused("appstream", def.appstream.is_some())?;
            let patterns = |list: Option<Vec<String>>| {
                list.unwrap_or_default()
                    .iter()
                    .map(|p| Pattern::new(p))
                    .collect()
            };
            Kind::Apk(Apk {
                root: def.root.unwrap_or_else(|| PathBuf::from("/")),
                hide: patterns(def.hide),
                protect: patterns(def.protect),
            })
        }
        other => {
            return Err(format!(
                "{here}: kind is \"flatpak\" or \"apk\", not \"{other}\""
            ));
        }
    };
    Ok(Source {
        label: def.label.unwrap_or_else(|| def.id.clone()),
        icon: def.icon.unwrap_or_else(|| "\u{f03d6}".into()),
        colour: def.colour.unwrap_or(Colour([0x7F, 0xB8, 0xD9, 0xFF])),
        id: def.id,
        kind,
    })
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
    Some(base.join("alpymist/store.toml"))
}

/// Find and parse the configuration. Never fails.
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
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    Loaded {
        config: Config::parse(DEFAULT).unwrap_or_else(|_| Config {
            store: Store::default(),
            sources: Vec::new(),
        }),
        path: None,
        problems,
    }
}

/// TOML errors quote the source over several lines; the status line has room
/// for one.
fn first_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('|') && !l.chars().all(|c| c == '^'))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Config, DEFAULT, Installation, Kind, Pattern};

    #[test]
    fn the_built_in_configuration_is_valid() {
        let config = Config::parse(DEFAULT).expect("store.toml must parse");
        assert_eq!(config.sources.len(), 2);
        let Kind::Flatpak(flathub) = &config.sources[0].kind else {
            panic!("Flathub first");
        };
        assert_eq!(flathub.installation, Installation::User);
        let Kind::Apk(apk) = &config.sources[1].kind else {
            panic!("Alpine second");
        };
        assert!(apk.hide.iter().any(|p| p.matches("zsh-doc")));
        assert!(
            apk.protect
                .iter()
                .any(|p| p.matches("alpymist-desktop-full"))
        );
        assert!(!apk.protect.iter().any(|p| p.matches("gimp")));
    }

    #[test]
    fn patterns_match_like_shell_globs() {
        assert!(Pattern::new("*-doc").matches("gimp-doc"));
        assert!(!Pattern::new("*-doc").matches("gimp-docs"));
        assert!(Pattern::new("linux-*").matches("linux-lts"));
        assert!(Pattern::new("a*b*c").matches("aXXbYYc"));
        assert!(!Pattern::new("a*b*c").matches("aXXbYY"));
        assert!(Pattern::new("musl").matches("musl"));
        assert!(!Pattern::new("musl").matches("musl-dev"));
    }

    #[test]
    fn a_setting_of_another_kind_is_an_error() {
        let text = "[[source]]\nid = \"a\"\nkind = \"apk\"\nremote = \"flathub\"\n";
        assert!(Config::parse(text).unwrap_err().contains("remote"));
    }

    #[test]
    fn ids_are_unique_and_kinds_known() {
        let twice =
            "[[source]]\nid = \"a\"\nkind = \"apk\"\n[[source]]\nid = \"a\"\nkind = \"apk\"\n";
        assert!(Config::parse(twice).unwrap_err().contains("two sources"));
        let unknown = "[[source]]\nid = \"a\"\nkind = \"snap\"\n";
        assert!(Config::parse(unknown).unwrap_err().contains("snap"));
    }

    #[test]
    fn a_disabled_source_is_left_out() {
        let text = "[[source]]\nid = \"a\"\nkind = \"apk\"\nenabled = false\n";
        assert!(Config::parse(text).unwrap().sources.is_empty());
    }
}

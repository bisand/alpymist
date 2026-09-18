//! What a screensaver says about itself.
//!
//! A screensaver is a program, and this is the file beside it that says so: its
//! name, what to run, and every setting it takes. Settings renders a page from
//! that file without knowing what the program draws, and the program reads the
//! same file to know what its settings default to — so a default is written
//! once, in one place, and the page and the program cannot disagree about it.
//!
//! ```toml
//! # /usr/share/alpymist/screensavers/mountains.toml
//! name = "Mountains"
//! description = "The ranges from the wallpaper, with the mist moving through them."
//! exec = "alpymist-saver-mountains"
//!
//! [[setting]]
//! key = "block"
//! title = "How chunky the picture is"
//! description = "Physical pixels to one drawn pixel. One is no pixelation at all."
//! kind = "number"
//! min = 1
//! max = 16
//! default = 6
//! ```
//!
//! Anything that ships such a file and the program it names becomes a
//! screensaver Alpymist offers, without Alpymist being rebuilt or knowing it
//! exists.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where screensavers are looked for, most specific first.
///
/// An account's own comes before the system's, so somebody trying one out does
/// not have to be root, and a screensaver of their own with the same name as a
/// packaged one wins for them alone.
#[must_use]
pub fn directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
        });
    if let Some(home) = home {
        dirs.push(home.join("alpymist/screensavers"));
    }
    dirs.push(PathBuf::from("/usr/share/alpymist/screensavers"));
    dirs
}

/// Where an account's own values for a screensaver live.
#[must_use]
pub fn values_path(id: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("alpymist/screensavers")
        .join(format!("{id}.toml"))
}

/// What kind of value a setting takes, and what it may be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dial {
    /// On or off.
    Switch {
        /// Its value when nobody has set it.
        default: bool,
    },
    /// A whole number in a range.
    Number {
        /// The smallest it may be.
        min: i64,
        /// The largest it may be.
        max: i64,
        /// How far one step goes.
        step: i64,
        /// What the number counts: `px`, `%`, `min`. Empty for a bare number.
        unit: String,
        /// Its value when nobody has set it.
        default: i64,
    },
    /// One of a list.
    Choice {
        /// The choices, in the order they are offered.
        option: Vec<Opt>,
        /// The value of the choice taken when nobody has chosen.
        default: String,
    },
}

/// One of a choice's options.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opt {
    /// As the file writes it.
    pub value: String,
    /// As a person reads it.
    pub label: String,
}

/// One thing a screensaver lets you change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Knob {
    /// Its name in the values file: `block`.
    pub key: String,
    /// What it is called.
    pub title: String,
    /// One sentence on what it does.
    pub description: String,
    /// Other words to find it by.
    pub keywords: Vec<String>,
    /// What it takes.
    pub dial: Dial,
}

/// A setting as its file writes it: flat, with the keys each kind needs.
///
/// Read into this and then checked, rather than into [`Dial`] directly, because
/// serde cannot both flatten a tagged enum and refuse unknown fields — and
/// refusing them is most of what makes a mistyped definition say so. Checking
/// by hand also lets the error name the key that is missing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKnob {
    key: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    keywords: Vec<String>,
    kind: String,
    #[serde(default)]
    min: Option<i64>,
    #[serde(default)]
    max: Option<i64>,
    #[serde(default)]
    step: Option<i64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    option: Vec<Opt>,
    default: toml::Value,
}

/// A screensaver's file, before it is checked.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDefinition {
    name: String,
    #[serde(default)]
    description: String,
    exec: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    setting: Vec<RawKnob>,
}

impl RawKnob {
    /// The checked knob this describes.
    fn check(self) -> Result<Knob, String> {
        let key = self.key;
        if key.is_empty() || key.contains(['.', ' ']) {
            return Err(format!("{key:?} is not a key"));
        }
        let need =
            |what: &str, v: Option<i64>| v.ok_or_else(|| format!("{key}: a number needs {what}"));
        let dial = match self.kind.as_str() {
            "switch" => Dial::Switch {
                default: self
                    .default
                    .as_bool()
                    .ok_or_else(|| format!("{key}: a switch defaults to true or false"))?,
            },
            "number" => {
                let min = need("min", self.min)?;
                let max = need("max", self.max)?;
                let step = self.step.unwrap_or(1);
                let default = self
                    .default
                    .as_integer()
                    .ok_or_else(|| format!("{key}: a number defaults to a number"))?;
                if min > max {
                    return Err(format!("{key}: min {min} is above max {max}"));
                }
                if step < 1 {
                    return Err(format!("{key}: a step of {step} never moves"));
                }
                if !(min..=max).contains(&default) {
                    return Err(format!(
                        "{key}: a default of {default} is outside {min} to {max}"
                    ));
                }
                Dial::Number {
                    min,
                    max,
                    step,
                    unit: self.unit.unwrap_or_default(),
                    default,
                }
            }
            "choice" => {
                let default = self
                    .default
                    .as_str()
                    .ok_or_else(|| format!("{key}: a choice defaults to one of its values"))?
                    .to_owned();
                if self.option.is_empty() {
                    return Err(format!("{key}: a choice of nothing"));
                }
                if !self.option.iter().any(|o| o.value == default) {
                    return Err(format!(
                        "{key}: a default of {default:?} is not one of the choices"
                    ));
                }
                Dial::Choice {
                    option: self.option,
                    default,
                }
            }
            other => {
                return Err(format!(
                    "{key}: no kind `{other}`; there is switch, number, choice"
                ));
            }
        };
        Ok(Knob {
            key,
            title: self.title,
            description: self.description,
            keywords: self.keywords,
            dial,
        })
    }
}

/// A screensaver, as its file describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// Its name in files and on the command line, from the file's own name.
    pub id: String,
    /// What it is called.
    pub name: String,
    /// One line on what it draws.
    pub description: String,
    /// The program to run. Found on `PATH`, as everything the desktop starts is.
    pub exec: String,
    /// A Nerd Font glyph for its page, or the screensaver's own when not said.
    pub icon: String,
    /// What it lets you change, in the order its page lists them.
    pub knobs: Vec<Knob>,
}

/// The glyph a screensaver's page carries when its file names none.
pub const ICON: &str = "\u{f0594}";

impl Definition {
    /// Read one, whose id is its file's name without the extension.
    ///
    /// # Errors
    /// When it cannot be read, does not parse, or describes nothing runnable.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{}: no name", path.display()))?;
        Self::parse(id, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Read one from its text, under this id.
    ///
    /// # Errors
    /// When it does not parse, or describes nothing runnable.
    pub fn parse(id: &str, text: &str) -> Result<Self, String> {
        let raw: RawDefinition = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        let mut knobs = Vec::with_capacity(raw.setting.len());
        for knob in raw.setting {
            knobs.push(knob.check()?);
        }
        let def = Self {
            name: raw.name,
            description: raw.description,
            exec: raw.exec,
            icon: raw.icon,
            knobs,
            id: id.to_owned(),
        };
        def.check()?;
        Ok(def)
    }

    /// What must be true of any definition, whoever wrote it.
    fn check(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.contains(['.', '/']) {
            return Err(format!("{:?} is not a name a setting can carry", self.id));
        }
        if self.id == crate::picture::RANDOM {
            return Err(format!(
                "a screensaver cannot be called {:?}: that means all of them",
                crate::picture::RANDOM
            ));
        }
        if self.exec.trim().is_empty() {
            return Err("no exec: nothing to run".into());
        }
        if self.name.trim().is_empty() {
            return Err("no name: nothing to call it in Settings".into());
        }
        let mut keys: Vec<&str> = self.knobs.iter().map(|k| k.key.as_str()).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        if keys.len() != count {
            return Err("two settings share a key".into());
        }
        Ok(())
    }

    /// The glyph its page carries.
    #[must_use]
    pub fn icon(&self) -> &str {
        if self.icon.is_empty() {
            ICON
        } else {
            &self.icon
        }
    }

    /// The area its settings live in: `screensaver-mountains`.
    #[must_use]
    pub fn area(&self) -> String {
        format!("screensaver-{}", self.id)
    }
}

/// Every screensaver installed, by name, in the order Settings lists them.
///
/// A file that does not parse is reported and left out rather than taken down
/// the whole list with it: one badly written third-party screensaver should not
/// stop the others being offered.
#[must_use]
pub fn discover() -> Vec<Definition> {
    let mut found: Vec<Definition> = Vec::new();
    for dir in directories() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            match Definition::read(&path) {
                // A directory earlier in the list has already answered for this
                // name, and the more specific one wins.
                Ok(def) if found.iter().any(|d| d.id == def.id) => {}
                Ok(def) => found.push(def),
                Err(why) => eprintln!("alpymist-screensaver: {why}"),
            }
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

#[cfg(test)]
mod tests {
    use super::{Definition, Dial, ICON};

    const MOUNTAINS: &str = r#"
name = "Mountains"
description = "The ranges from the wallpaper."
exec = "alpymist-saver-mountains"

[[setting]]
key = "block"
title = "How chunky the picture is"
kind = "number"
min = 1
max = 16
default = 6
unit = "px"
"#;

    #[test]
    fn a_definition_reads_as_what_it_says() {
        let def = Definition::parse("mountains", MOUNTAINS).unwrap();
        assert_eq!(def.id, "mountains");
        assert_eq!(def.name, "Mountains");
        assert_eq!(def.exec, "alpymist-saver-mountains");
        assert_eq!(def.area(), "screensaver-mountains");
        assert_eq!(def.icon(), ICON, "the screensaver glyph when none is named");
        assert_eq!(def.knobs.len(), 1);
        assert_eq!(def.knobs[0].key, "block");
        assert_eq!(
            def.knobs[0].dial,
            Dial::Number {
                min: 1,
                max: 16,
                step: 1,
                unit: "px".into(),
                default: 6
            }
        );
    }

    #[test]
    fn a_screensaver_with_no_settings_is_a_screensaver() {
        let def = Definition::parse(
            "plain",
            "name = \"Plain\"\nexec = \"alpymist-saver-plain\"\n",
        )
        .unwrap();
        assert!(def.knobs.is_empty(), "and its page is just the picture");
    }

    #[test]
    fn nothing_runnable_is_refused_rather_than_offered() {
        let missing_exec = "name = \"X\"\n";
        assert!(Definition::parse("x", missing_exec).is_err());
        let empty_exec = "name = \"X\"\nexec = \"  \"\n";
        assert!(Definition::parse("x", empty_exec).is_err());
        let no_name = "exec = \"x\"\n";
        assert!(Definition::parse("x", no_name).is_err());
    }

    #[test]
    fn a_screensaver_may_not_be_called_random() {
        let def = "name = \"X\"\nexec = \"x\"\n";
        assert!(
            Definition::parse("random", def).is_err(),
            "it would be unreachable behind `show = random`"
        );
    }

    #[test]
    fn a_name_that_could_not_be_a_setting_is_refused() {
        let def = "name = \"X\"\nexec = \"x\"\n";
        assert!(Definition::parse("with.dot", def).is_err());
        assert!(Definition::parse("", def).is_err());
    }

    #[test]
    fn a_default_outside_its_own_range_is_refused() {
        let bad = "name = \"X\"\nexec = \"x\"\n\n[[setting]]\nkey = \"k\"\ntitle = \"K\"\n\
                   kind = \"number\"\nmin = 1\nmax = 4\ndefault = 9\n";
        let why = Definition::parse("x", bad).unwrap_err();
        assert!(why.contains("outside"), "{why}");
    }

    #[test]
    fn a_choice_whose_default_is_not_a_choice_is_refused() {
        let bad = "name = \"X\"\nexec = \"x\"\n\n[[setting]]\nkey = \"k\"\ntitle = \"K\"\n\
                   kind = \"choice\"\ndefault = \"c\"\n\
                   [[setting.option]]\nvalue = \"a\"\nlabel = \"A\"\n";
        let why = Definition::parse("x", bad).unwrap_err();
        assert!(why.contains("not one of the choices"), "{why}");
    }

    #[test]
    fn two_settings_sharing_a_key_are_refused() {
        let bad = "name = \"X\"\nexec = \"x\"\n\n[[setting]]\nkey = \"k\"\ntitle = \"A\"\n\
                   kind = \"switch\"\ndefault = true\n\n[[setting]]\nkey = \"k\"\ntitle = \"B\"\n\
                   kind = \"switch\"\ndefault = false\n";
        assert!(Definition::parse("x", bad).unwrap_err().contains("share"));
    }

    #[test]
    fn a_key_that_is_misspelt_is_an_error_not_silence() {
        let typo = "name = \"X\"\nexec = \"x\"\nexce = \"y\"\n";
        assert!(Definition::parse("x", typo).is_err());
    }
}

//! The screensaver: when it comes on, which one, and each one's own page.
//!
//! Two halves. The policy — when the screensaver appears, when the screen goes
//! off, whether it locks, and which screensaver to run — is written here, and
//! lives in `screensaver.toml`.
//!
//! The rest is not written here at all. Every screensaver installed declares
//! what it lets you change, in the file beside its program, and this turns each
//! of those into an area and a page of settings that Settings renders like any
//! other. Nothing in Alpymist knows what a "mountain" is; installing a package
//! that ships a definition and a binary adds a page, and removing it takes the
//! page away.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_screensaver::config::{Config, MAX_MINUTES};
use alpymist_screensaver::definition::{Definition, Dial, discover};
use alpymist_screensaver::idle;
use alpymist_screensaver::picture::{RANDOM, Show};
use alpymist_screensaver::values::{Value as Held, Values};
use std::sync::OnceLock;

/// The account's screensaver file.
pub const CONFIG: &str = "alpymist/screensaver.toml";

/// The id of the setting that shows a screensaver now.
pub const PREVIEW: &str = "preview";

/// What is installed, found once.
///
/// Once per process, and kept for as long as it runs: the ids, titles and
/// descriptions read out of the definition files have to outlive the `Setting`s
/// that name them, and every `Setting` in Alpymist carries `&'static str`
/// because every other one is written in the source. Reading the directory
/// again for each would be both slower and no less permanent.
struct Installed {
    areas: &'static [crate::model::Area],
    settings: &'static [Setting],
    definitions: &'static [Definition],
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// A string that lives as long as the process, for a `Setting` to carry.
fn kept(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every screensaver's area and settings, built from what is installed.
fn installed() -> &'static Installed {
    INSTALLED.get_or_init(|| {
        let definitions: &'static [Definition] = Box::leak(discover().into_boxed_slice());
        let mut areas = Vec::new();
        let mut settings = Vec::new();
        for def in definitions {
            let area = kept(def.area());
            areas.push(crate::model::Area {
                id: area,
                title: kept(def.name.clone()),
                description: kept(if def.description.is_empty() {
                    format!("What {} draws, and how", def.name)
                } else {
                    def.description.clone()
                }),
                icon: kept(def.icon().to_owned()),
                keywords: Box::leak(
                    vec![kept("screensaver".to_owned()), kept(def.id.clone())].into_boxed_slice(),
                ),
            });
            // Every screensaver gets this, whether or not it declares anything
            // else: seeing it is the one thing everybody wants from its page.
            settings.push(Setting {
                id: kept(format!("{area}.{PREVIEW}")),
                title: kept(format!("See {} now", def.name)),
                description: "Show it until a key is pressed or the pointer moves.",
                keywords: &["preview", "try", "show", "test"],
                kind: Kind::Action {
                    label: kept("Preview".to_owned()),
                },
                default: Value::Text(String::new()),
                scope: Scope::Account,
                applies: Applies::Now,
            });
            for knob in &def.knobs {
                settings.push(Setting {
                    id: kept(format!("{area}.{}", knob.key)),
                    title: kept(knob.title.clone()),
                    description: kept(knob.description.clone()),
                    keywords: Box::leak(
                        knob.keywords
                            .iter()
                            .map(|k| kept(k.clone()))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    ),
                    kind: kind_of(&knob.dial),
                    default: value_of(&knob.default()),
                    scope: Scope::Account,
                    applies: Applies::Now,
                });
            }
        }
        Installed {
            areas: Box::leak(areas.into_boxed_slice()),
            settings: Box::leak(settings.into_boxed_slice()),
            definitions,
        }
    })
}

/// An area for every screensaver installed.
#[must_use]
pub fn areas() -> &'static [crate::model::Area] {
    installed().areas
}

/// Every screensaver's own settings.
#[must_use]
pub fn per_screensaver() -> &'static [Setting] {
    installed().settings
}

/// The screensaver an area belongs to, if it belongs to one.
fn definition(area: &str) -> Option<&'static Definition> {
    installed().definitions.iter().find(|d| d.area() == area)
}

/// The area holding the settings of the screensaver `show` names.
///
/// `show` is what `screensaver.show` holds: a screensaver's id, or [`RANDOM`],
/// which names all of them and so none in particular — that gives `None`, as
/// does a name whose package is not installed.
#[must_use]
pub fn area_of(show: &str) -> Option<&'static crate::model::Area> {
    if show == RANDOM {
        return None;
    }
    let def = installed().definitions.iter().find(|d| d.id == show)?;
    installed().areas.iter().find(|a| a.id == def.area())
}

/// Whether `area` is one of the screensavers'.
#[must_use]
pub fn owns(area: &str) -> bool {
    definition(area).is_some()
}

/// What a declared dial is, as Settings draws it.
fn kind_of(dial: &Dial) -> Kind {
    match dial {
        Dial::Switch { .. } => Kind::Switch,
        Dial::Number {
            min,
            max,
            step,
            unit,
            ..
        } => Kind::Number {
            min: *min,
            max: *max,
            step: *step,
            unit: kept(unit.clone()),
        },
        Dial::Choice { option, .. } => Kind::Choice(
            option
                .iter()
                .map(|o| Choice::new(o.value.clone(), o.label.clone()))
                .collect(),
        ),
    }
}

/// A screensaver's value, as Settings holds one.
fn value_of(held: &Held) -> Value {
    match held {
        Held::Bool(v) => Value::Bool(*v),
        Held::Number(v) => Value::Number(*v),
        Held::Text(v) => Value::Text(v.clone()),
    }
}

/// Settings' value, as a screensaver holds one.
fn held_of(value: &Value) -> Held {
    match value {
        Value::Bool(v) => Held::Bool(*v),
        Value::Number(v) => Held::Number(*v),
        Value::Text(v) => Held::Text(v.clone()),
    }
}

/// Minutes, as a setting. Zero is never, and a step of one so a slider over the
/// hour can still be put on any minute of it.
fn minutes(max: u32) -> Kind {
    Kind::Number {
        min: 0,
        max: i64::from(max),
        step: 1,
        unit: "min",
    }
}

/// The policy settings, and every screensaver's own after them.
pub fn settings() -> Vec<Setting> {
    let defaults = Config::default();
    let account = |id, title, description, keywords, kind, default| Setting {
        id,
        title,
        description,
        keywords,
        kind,
        default,
        scope: Scope::Account,
        applies: Applies::Now,
    };
    let mut all = vec![
        account(
            "screensaver.show",
            "Screensaver",
            "Which one appears, or a different one each time.",
            &["picture", "which", "random", "shuffle"],
            Kind::Choice(
                installed()
                    .definitions
                    .iter()
                    .map(|d| Choice::new(d.id.clone(), d.name.clone()))
                    .chain([Choice::new(RANDOM, "A different one each time")])
                    .collect(),
            ),
            Value::Text(defaults.show.id().to_owned()),
        ),
        account(
            "screensaver.after",
            "Show the screensaver after",
            "Minutes of stillness before it appears. Zero never shows it.",
            &["screen saver", "idle", "timeout"],
            minutes(MAX_MINUTES),
            Value::Number(i64::from(defaults.after)),
        ),
        account(
            "screensaver.blank-after",
            "Turn the screen off after",
            "Minutes of stillness before the screen turns off, counted from the last key or click. Zero leaves it on.",
            &["blank", "dpms", "display", "sleep", "battery", "idle"],
            minutes(MAX_MINUTES),
            Value::Number(i64::from(defaults.blank_after)),
        ),
        account(
            "screensaver.lock",
            "Lock when the screen turns off",
            "Ask for the password to get back in.",
            &["password", "security", "lock", "screen lock"],
            Kind::Switch,
            Value::Bool(defaults.lock),
        ),
    ];
    all.extend_from_slice(per_screensaver());
    all
}

fn file(env: &Env) -> std::path::PathBuf {
    env.account(CONFIG)
}

fn load(env: &Env) -> Result<Config, String> {
    Config::read(&file(env))
}

/// A setting's value.
///
/// # Errors
/// When a file is there but does not parse.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    if let Some(def) = definition(setting.area()) {
        let key = key_of(setting);
        if key == PREVIEW {
            // Nothing to read: it is a thing to do, not a thing to be.
            return Ok(Value::Text(String::new()));
        }
        let values = Values::read(def);
        return values
            .get(key)
            .map(value_of)
            .ok_or_else(|| format!("{} has no setting `{key}`", def.name));
    }
    let c = load(env)?;
    Ok(match setting.id {
        "screensaver.show" => Value::Text(c.show.id().to_owned()),
        "screensaver.after" => Value::Number(i64::from(c.after)),
        "screensaver.blank-after" => Value::Number(i64::from(c.blank_after)),
        _ => Value::Bool(c.lock),
    })
}

/// The part of a setting's id after its area: `block` of `screensaver-mountains.block`.
fn key_of(setting: &Setting) -> &'static str {
    setting.id.split_once('.').map_or(setting.id, |(_, k)| k)
}

/// Change a setting, and tell a running watcher to start over with it.
///
/// # Errors
/// When a file cannot be read or written, or a screensaver is named that is not
/// installed.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let value = value.unwrap_or(&setting.default);
    if let Some(def) = definition(setting.area()) {
        let key = key_of(setting);
        if key == PREVIEW {
            return preview(def);
        }
        let values = Values::read(def).with(def, key, held_of(value));
        return crate::generated::replace(
            &env.account("alpymist/screensavers")
                .join(format!("{}.toml", def.id)),
            &values.to_toml(def),
        );
    }
    let number = || u32::try_from(value.as_number().unwrap_or(0)).unwrap_or(0);
    let mut c = load(env)?;
    match setting.id {
        "screensaver.show" => {
            let name = value.as_text().unwrap_or_default();
            if name != RANDOM && !installed().definitions.iter().any(|d| d.id == name) {
                return Err(format!("no screensaver called `{name}` is installed"));
            }
            c.show = Show::of(name);
        }
        "screensaver.after" => c.after = number(),
        "screensaver.blank-after" => c.blank_after = number(),
        _ => c.lock = value.as_bool().unwrap_or(false),
    }
    crate::generated::replace(&file(env), &c.to_toml())?;
    // Nobody watching is not a failure: the desktop may not be running, and
    // the watch reads the file when it starts.
    let _ = idle::reload();
    Ok(())
}

/// Show a screensaver now, and leave it running.
///
/// Started rather than waited for: it goes away by itself at the first key or
/// movement, and Settings has no business sitting still until it does.
fn preview(def: &Definition) -> Result<(), String> {
    std::process::Command::new(&def.exec)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{}: {e}", def.exec))
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW, key_of, kind_of, minutes, value_of};
    use crate::model::{Kind, Setting, Value};
    use alpymist_screensaver::definition::Definition;
    use alpymist_screensaver::values::Value as Held;

    fn setting(id: &'static str) -> Setting {
        Setting {
            id,
            title: "",
            description: "",
            keywords: &[],
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: crate::model::Scope::Account,
            applies: crate::model::Applies::Now,
        }
    }

    #[test]
    fn a_screensaver_setting_carries_its_own_key_after_its_area() {
        assert_eq!(key_of(&setting("screensaver-mountains.block")), "block");
        assert_eq!(
            setting("screensaver-mountains.block").area(),
            "screensaver-mountains"
        );
        assert_eq!(key_of(&setting("screensaver.after")), "after");
    }

    #[test]
    fn the_preview_is_the_one_setting_every_screensaver_has() {
        assert_eq!(key_of(&setting("screensaver-anything.preview")), PREVIEW);
    }

    #[test]
    fn a_declared_dial_becomes_the_control_that_matches_it() {
        let def = Definition::parse(
            "x",
            "name = \"X\"\nexec = \"x\"\n\n\
             [[setting]]\nkey = \"n\"\ntitle = \"N\"\nkind = \"number\"\n\
             min = 2\nmax = 9\nstep = 3\nunit = \"px\"\ndefault = 5\n\n\
             [[setting]]\nkey = \"s\"\ntitle = \"S\"\nkind = \"switch\"\ndefault = true\n\n\
             [[setting]]\nkey = \"c\"\ntitle = \"C\"\nkind = \"choice\"\ndefault = \"a\"\n\
             [[setting.option]]\nvalue = \"a\"\nlabel = \"A\"\n\
             [[setting.option]]\nvalue = \"b\"\nlabel = \"B\"\n",
        )
        .unwrap();
        assert_eq!(
            kind_of(&def.knobs[0].dial),
            Kind::Number {
                min: 2,
                max: 9,
                step: 3,
                unit: "px"
            }
        );
        assert_eq!(kind_of(&def.knobs[1].dial), Kind::Switch);
        let Kind::Choice(choices) = kind_of(&def.knobs[2].dial) else {
            panic!("a choice draws as a choice");
        };
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].value, "a");
        assert_eq!(choices[0].label, "A");
    }

    #[test]
    fn a_screensavers_values_cross_into_settings_unchanged() {
        assert_eq!(value_of(&Held::Bool(true)), Value::Bool(true));
        assert_eq!(value_of(&Held::Number(7)), Value::Number(7));
        assert_eq!(value_of(&Held::Text("a".into())), Value::Text("a".into()));
    }

    #[test]
    fn a_timeout_can_always_be_turned_off_and_never_runs_past_an_hour() {
        let Kind::Number { min, max, .. } = minutes(super::MAX_MINUTES) else {
            panic!("a timeout is a number");
        };
        assert_eq!(min, 0, "zero is never");
        assert_eq!(max, i64::from(super::MAX_MINUTES));
    }
}

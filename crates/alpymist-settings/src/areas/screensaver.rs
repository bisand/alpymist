//! When the screen shows the mountains, when it goes off, and whether it locks:
//! the same `screensaver.toml` the screensaver and its idle watch read.
//!
//! Changing any of these tells a running watcher to read the file again, so a
//! new timeout is the one in force from the moment it is set rather than from
//! the next login.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_screensaver::config::{BLOCK, Config, MAX_MINUTES};
use alpymist_screensaver::idle;

/// The account's screensaver file.
pub const CONFIG: &str = "alpymist/screensaver.toml";

/// Minutes, as a setting: zero is never, which the app shows as such.
fn minutes(max: u32) -> Kind {
    Kind::Number {
        min: 0,
        max: i64::from(max),
        step: 1,
        unit: "min",
    }
}

/// The settings.
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
    vec![
        account(
            "screensaver.after",
            "Show the screensaver after",
            "Minutes of stillness before the mountains appear. Zero never shows them.",
            &["screen saver", "idle", "timeout", "mountains"],
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
            &["password", "security", "swaylock", "screen lock"],
            Kind::Switch,
            Value::Bool(defaults.lock),
        ),
        account(
            "screensaver.block",
            "How chunky the picture is",
            "Physical pixels to one drawn pixel. One is no pixelation at all.",
            &["pixels", "pixelated", "resolution", "blocks"],
            Kind::Number {
                min: i64::from(BLOCK.0),
                max: i64::from(BLOCK.1),
                step: 1,
                unit: "px",
            },
            Value::Number(i64::from(defaults.block)),
        ),
    ]
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
/// When the file is there but does not parse.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    let c = load(env)?;
    Ok(match setting.id {
        "screensaver.after" => Value::Number(i64::from(c.after)),
        "screensaver.blank-after" => Value::Number(i64::from(c.blank_after)),
        "screensaver.lock" => Value::Bool(c.lock),
        _ => Value::Number(i64::from(c.block)),
    })
}

/// Change a setting, and tell a running watcher to start over with it.
///
/// # Errors
/// When the file cannot be read or written.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let value = value.unwrap_or(&setting.default);
    let number = || u32::try_from(value.as_number().unwrap_or(0)).unwrap_or(0);
    let mut c = load(env)?;
    match setting.id {
        "screensaver.after" => c.after = number(),
        "screensaver.blank-after" => c.blank_after = number(),
        "screensaver.lock" => c.lock = value.as_bool().unwrap_or(false),
        _ => c.block = number(),
    }
    crate::generated::replace(&file(env), &c.to_toml())?;
    // Nobody watching is not a failure: the desktop may not be running, and
    // the watch reads the file when it starts.
    let _ = idle::reload();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::settings;
    use crate::model::{Kind, Scope, Value};

    #[test]
    fn every_setting_is_the_accounts_own_and_applies_at_once() {
        for s in settings() {
            assert_eq!(s.scope, Scope::Account, "{}", s.id);
            assert_eq!(s.area(), "screensaver", "{}", s.id);
        }
    }

    #[test]
    fn the_defaults_here_are_the_screensavers_own() {
        let defaults = alpymist_screensaver::config::Config::default();
        let by = |id: &str| settings().into_iter().find(|s| s.id == id).unwrap().default;
        assert_eq!(
            by("screensaver.after"),
            Value::Number(defaults.after.into())
        );
        assert_eq!(by("screensaver.lock"), Value::Bool(defaults.lock));
        assert_eq!(
            by("screensaver.block"),
            Value::Number(defaults.block.into())
        );
    }

    #[test]
    fn zero_is_offered_so_never_is_a_thing_that_can_be_chosen() {
        for id in ["screensaver.after", "screensaver.blank-after"] {
            let s = settings().into_iter().find(|s| s.id == id).unwrap();
            let Kind::Number { min, .. } = s.kind else {
                panic!("{id} is not a number");
            };
            assert_eq!(min, 0, "{id} cannot be turned off");
        }
    }

    #[test]
    fn the_picture_can_never_be_asked_for_a_block_of_no_pixels() {
        let s = settings()
            .into_iter()
            .find(|s| s.id == "screensaver.block")
            .unwrap();
        let Kind::Number { min, .. } = s.kind else {
            panic!("not a number");
        };
        assert!(min >= 1, "a block of zero pixels divides by zero");
    }
}

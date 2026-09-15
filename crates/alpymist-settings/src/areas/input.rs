//! Keyboard repeat, the touchpad and the mouse: the account's, applied to
//! Hyprland.
//!
//! The values are in the account's `settings.toml`. From them comes
//! `~/.config/alpymist/hypr/settings.conf`, which the packaged Hyprland
//! configuration sources, and a change is also handed to the running Hyprland
//! with `hyprctl keyword`, so it shows at once.

use crate::env::Env;
use crate::generated;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use crate::values::Values;
use std::fmt::Write as _;

/// The generated file, under the account's configuration.
pub const HYPR_CONF: &str = "alpymist/hypr/settings.conf";
/// The account's values.
pub const VALUES: &str = "alpymist/settings.toml";

/// Where each setting goes in Hyprland: its keyword, and how a value is
/// written there.
struct Binding {
    id: &'static str,
    keyword: &'static str,
    write: fn(&Value) -> String,
}

fn boolean(v: &Value) -> String {
    v.as_bool().unwrap_or(false).to_string()
}

fn integer(v: &Value) -> String {
    v.as_number().unwrap_or(0).to_string()
}

/// A percentage as Hyprland's fraction: 150 is `1.50`.
fn fraction(v: &Value) -> String {
    let n = v.as_number().unwrap_or(0);
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", n.abs() / 100, n.abs() % 100)
}

fn text(v: &Value) -> String {
    v.as_text().unwrap_or_default().to_owned()
}

const BINDINGS: &[Binding] = &[
    Binding {
        id: "keyboard.repeat-delay",
        keyword: "input:repeat_delay",
        write: integer,
    },
    Binding {
        id: "keyboard.repeat-rate",
        keyword: "input:repeat_rate",
        write: integer,
    },
    Binding {
        id: "mouse.speed",
        keyword: "input:sensitivity",
        write: fraction,
    },
    Binding {
        id: "mouse.acceleration",
        keyword: "input:accel_profile",
        write: text,
    },
    Binding {
        id: "mouse.natural-scroll",
        keyword: "input:natural_scroll",
        write: boolean,
    },
    Binding {
        id: "mouse.left-handed",
        keyword: "input:left_handed",
        write: boolean,
    },
    Binding {
        id: "touchpad.natural-scroll",
        keyword: "input:touchpad:natural_scroll",
        write: boolean,
    },
    Binding {
        id: "touchpad.tap-to-click",
        keyword: "input:touchpad:tap-to-click",
        write: boolean,
    },
    Binding {
        id: "touchpad.disable-while-typing",
        keyword: "input:touchpad:disable_while_typing",
        write: boolean,
    },
    Binding {
        id: "touchpad.scroll-speed",
        keyword: "input:touchpad:scroll_factor",
        write: fraction,
    },
];

/// The settings. Defaults are Hyprland's own, so a generated file changes
/// nothing until something is set.
#[allow(clippy::too_many_lines)] // a table
pub fn settings() -> Vec<Setting> {
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
            "keyboard.repeat-delay",
            "Repeat delay",
            "How long a key is held before it starts repeating.",
            &["hold", "key repeat", "typematic"],
            Kind::Number {
                min: 150,
                max: 1000,
                step: 50,
                unit: "ms",
            },
            Value::Number(600),
        ),
        account(
            "keyboard.repeat-rate",
            "Repeat rate",
            "How many times a second a held key repeats.",
            &["key repeat", "speed", "typematic"],
            Kind::Number {
                min: 10,
                max: 80,
                step: 5,
                unit: "per second",
            },
            Value::Number(25),
        ),
        account(
            "touchpad.natural-scroll",
            "Natural scrolling",
            "Content moves the way your fingers do, as on a phone.",
            &["reverse", "invert", "direction", "trackpad"],
            Kind::Switch,
            Value::Bool(false),
        ),
        account(
            "touchpad.tap-to-click",
            "Tap to click",
            "A tap on the touchpad clicks, without pressing it down.",
            &["tap", "click", "trackpad"],
            Kind::Switch,
            Value::Bool(true),
        ),
        account(
            "touchpad.disable-while-typing",
            "Ignore while typing",
            "The touchpad does nothing for a moment after a key is pressed.",
            &["palm", "typing", "trackpad"],
            Kind::Switch,
            Value::Bool(true),
        ),
        account(
            "touchpad.scroll-speed",
            "Scrolling speed",
            "How far content moves for a two-finger swipe.",
            &["scroll", "speed", "trackpad"],
            Kind::Number {
                min: 25,
                max: 300,
                step: 25,
                unit: "%",
            },
            Value::Number(100),
        ),
        account(
            "mouse.speed",
            "Pointer speed",
            "How fast the pointer moves, for mice and touchpads alike.",
            &["sensitivity", "cursor", "pointer", "trackpad"],
            Kind::Number {
                min: -100,
                max: 100,
                step: 10,
                unit: "",
            },
            Value::Number(0),
        ),
        account(
            "mouse.acceleration",
            "Pointer acceleration",
            "Faster movements carry the pointer further, or every movement counts the same.",
            &["accel", "precision", "gaming", "cursor"],
            Kind::Choice(vec![
                Choice::new("adaptive", "Adaptive"),
                Choice::new("flat", "Flat"),
            ]),
            Value::Text("adaptive".into()),
        ),
        account(
            "mouse.natural-scroll",
            "Natural scrolling for mice",
            "The wheel moves content the other way round.",
            &["reverse", "invert", "wheel"],
            Kind::Switch,
            Value::Bool(false),
        ),
        account(
            "mouse.left-handed",
            "Left-handed buttons",
            "Swap the left and right buttons.",
            &["swap", "buttons", "left hand"],
            Kind::Switch,
            Value::Bool(false),
        ),
    ]
}

/// The value of one of these settings.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    Ok(Values::load(&env.account(VALUES))?
        .get(setting.id)
        .unwrap_or_else(|| setting.default.clone()))
}

/// Set one, or reset it with `None`, and write the Hyprland file.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>, force: bool) -> Result<(), String> {
    let path = env.account(VALUES);
    let mut values = Values::load(&path)?;
    match value {
        Some(v) => values.set(setting.id, v),
        None => values.remove(setting.id),
    }
    // The Hyprland file first: if it was edited by hand, nothing changes.
    write_conf(env, &values, force)?;
    values.save(&path)
}

/// Hand a value to the running Hyprland.
pub fn live(env: &Env, setting: &Setting, value: &Value) -> Result<(), String> {
    if !env.hyprland {
        return Ok(());
    }
    let Some(b) = BINDINGS.iter().find(|b| b.id == setting.id) else {
        return Ok(());
    };
    env.run(&["hyprctl", "keyword", b.keyword, &(b.write)(value)])
        .map(drop)
}

/// The Hyprland file's body for `values`.
fn conf(values: &Values) -> String {
    let all = settings();
    let value = |id: &str| {
        values
            .get(id)
            .or_else(|| all.iter().find(|s| s.id == id).map(|s| s.default.clone()))
            .unwrap_or(Value::Bool(false))
    };
    let mut outer = String::new();
    let mut touchpad = String::new();
    for b in BINDINGS {
        let line = (b.write)(&value(b.id));
        if let Some(key) = b.keyword.strip_prefix("input:touchpad:") {
            let _ = writeln!(touchpad, "        {key} = {line}");
        } else if let Some(key) = b.keyword.strip_prefix("input:") {
            let _ = writeln!(outer, "    {key} = {line}");
        }
    }
    format!("input {{\n{outer}    touchpad {{\n{touchpad}    }}\n}}\n")
}

fn write_conf(env: &Env, values: &Values, force: bool) -> Result<(), String> {
    generated::write(
        &env.account(HYPR_CONF),
        "#",
        "~/.config/alpymist/settings.toml",
        &conf(values),
        |_| false,
        force,
    )
}

/// Make sure the Hyprland file exists before Hyprland reads its
/// configuration, which sources it and refuses a missing file. Left alone
/// when edited by hand.
///
/// # Errors
/// The values could not be read or the file written.
pub fn prepare_session(env: &Env) -> Result<(), String> {
    let path = env.account(HYPR_CONF);
    match generated::state(&path, "#", |_| false)? {
        generated::State::HandEdited => Ok(()),
        _ => write_conf(env, &Values::load(&env.account(VALUES))?, false),
    }
}

#[cfg(test)]
mod tests {
    use super::{conf, fraction};
    use crate::model::Value;
    use crate::values::Values;

    #[test]
    fn percentages_become_hyprlands_fractions() {
        assert_eq!(fraction(&Value::Number(150)), "1.50");
        assert_eq!(fraction(&Value::Number(-30)), "-0.30");
        assert_eq!(fraction(&Value::Number(5)), "0.05");
    }

    #[test]
    fn the_file_has_every_setting_in_its_section() {
        let mut v = Values::default();
        v.set("touchpad.natural-scroll", &Value::Bool(true));
        v.set("mouse.speed", &Value::Number(-20));
        let text = conf(&v);
        assert!(
            text.starts_with("input {\n    repeat_delay = 600\n"),
            "{text}"
        );
        assert!(text.contains("    sensitivity = -0.20\n"), "{text}");
        assert!(
            text.contains("    touchpad {\n        natural_scroll = true\n"),
            "{text}"
        );
        assert!(text.contains("        tap-to-click = true\n"), "{text}");
        assert!(text.ends_with("    }\n}\n"), "{text}");
    }
}

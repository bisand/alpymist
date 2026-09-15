//! Every area, and which one answers for a setting.

pub mod appearance;
pub mod input;
pub mod keyboard;
pub mod power;
pub mod updates;
pub mod wifi;

use crate::env::Env;
use crate::model::{Area, Setting, Value};

/// The areas, in the order the app lists them.
pub const AREAS: &[Area] = &[
    Area {
        id: "appearance",
        title: "Appearance",
        description: "Light or dark, the accent colour and the text size",
        icon: "\u{f03d8}",
        keywords: &["theme", "look", "colours", "dark mode"],
    },
    Area {
        id: "keyboard",
        title: "Keyboard",
        description: "The layout, and how held keys repeat",
        icon: "\u{f030c}",
        keywords: &["typing", "language", "keymap"],
    },
    Area {
        id: "touchpad",
        title: "Touchpad",
        description: "Tapping, scrolling and typing with a touchpad",
        icon: "\u{f0cc3}",
        keywords: &["trackpad", "gestures"],
    },
    Area {
        id: "mouse",
        title: "Mouse & pointer",
        description: "Pointer speed, acceleration and buttons",
        icon: "\u{f037d}",
        keywords: &["cursor", "pointer", "wheel"],
    },
    Area {
        id: "wifi",
        title: "Wi-Fi",
        description: "Wireless networks",
        icon: "\u{f05a9}",
        keywords: &["network", "wireless", "internet"],
    },
    Area {
        id: "power",
        title: "Power",
        description: "The lid, the power button, the battery and power modes",
        icon: "\u{f0079}",
        keywords: &["battery", "suspend", "sleep", "laptop"],
    },
    Area {
        id: "updates",
        title: "Updates",
        description: "The release channel Alpymist follows",
        icon: "\u{f06b0}",
        keywords: &["upgrade", "channel", "packages"],
    },
];

/// Every setting, in each area's order.
pub fn all() -> Vec<Setting> {
    [
        appearance::settings(),
        keyboard::settings(),
        input::settings(),
        wifi::settings(),
        power::settings(),
        updates::settings(),
    ]
    .concat()
}

/// A setting's current value.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    match s.area() {
        "appearance" => Ok(appearance::get(env, s)),
        "keyboard" if s.id == "keyboard.layout" => keyboard::get(env, s),
        "keyboard" | "touchpad" | "mouse" => input::get(env, s),
        "wifi" => wifi::get(env),
        "power" => power::get(env, s),
        "updates" => updates::get(env),
        other => Err(format!("no area `{other}`")),
    }
}

/// Write a value, or the default with `None`. Returns notes worth telling.
pub fn set(
    env: &Env,
    s: &Setting,
    value: Option<&Value>,
    force: bool,
) -> Result<Vec<String>, String> {
    let none = |r: Result<(), String>| r.map(|()| Vec::new());
    match s.area() {
        "appearance" => none(appearance::set(env, s, value)),
        "keyboard" if s.id == "keyboard.layout" => keyboard::set(env, s, value, force),
        "keyboard" | "touchpad" | "mouse" => none(input::set(env, s, value, force)),
        "wifi" => none(wifi::set(env, value)),
        "power" => none(power::set(env, s, value)),
        "updates" => none(updates::set(env, value)),
        other => Err(format!("no area `{other}`")),
    }
}

/// Show a change in the running session, from the person's own process.
pub fn live(env: &Env, s: &Setting, value: &Value) -> Result<(), String> {
    match s.area() {
        "keyboard" if s.id == "keyboard.layout" => keyboard::live(env, value),
        "keyboard" | "touchpad" | "mouse" => input::live(env, s, value),
        _ => Ok(()),
    }
}

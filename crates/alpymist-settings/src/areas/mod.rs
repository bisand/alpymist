//! Every area, and which one answers for a setting.

pub mod appearance;
pub mod input;
pub mod keyboard;
pub mod power;
pub mod screensaver;
pub mod updates;
pub mod wallpaper;
pub mod wifi;

use crate::env::Env;
use crate::model::{Area, Setting, Value};
use std::sync::OnceLock;

/// The areas written here, in the order the app lists them.
///
/// This is also, exactly, the list of pages the settings app puts down its
/// side. Every screensaver installed has an area of its own as well, read from
/// the file it ships rather than written anywhere in Alpymist — but those are
/// not pages: they are reached through the Screensaver page, which is where
/// choosing a screensaver already happens. [`areas`] is the two together, for
/// everything that resolves a setting's id; [`pages`] is this list alone.
pub const AREAS: &[Area] = &[
    Area {
        id: "appearance",
        title: "Appearance",
        description: "Light or dark, the accent colour, the text size and the wallpaper",
        icon: "\u{f03d8}",
        keywords: &[
            "theme",
            "look",
            "colours",
            "dark mode",
            "wallpaper",
            "background",
        ],
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
        icon: "\u{f0322}",
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
        id: "screensaver",
        title: "Screensaver",
        description: "Which screensaver appears when nobody is there, and when the screen turns off",
        icon: "\u{f0594}",
        keywords: &["screen saver", "idle", "blank", "lock", "timeout"],
    },
    Area {
        id: "updates",
        title: "Updates",
        description: "The release channel Alpymist follows",
        icon: "\u{f06b0}",
        keywords: &["upgrade", "channel", "packages"],
    },
];

/// Every area: those written here, and one for each screensaver installed.
///
/// Found once and kept: the screensavers' come from reading a directory, and
/// doing that again for every caller would be both slower and no less
/// permanent, since an `Area` holds `&'static str`.
pub fn areas() -> &'static [Area] {
    static ALL: OnceLock<&'static [Area]> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<Area> = AREAS.to_vec();
        // After Updates and About would be odd; the screensavers belong beside
        // the screensaver settings that choose between them.
        let at = all
            .iter()
            .position(|a| a.id == "screensaver")
            .map_or(all.len(), |i| i + 1);
        for (offset, area) in screensaver::areas().iter().enumerate() {
            all.insert(at + offset, area.clone());
        }
        Box::leak(all.into_boxed_slice())
    })
}

/// The areas the settings app lists as pages, in order.
///
/// The screensavers' are left out on purpose. Each is reached from the
/// Screensaver page, beside the list that chooses between them, rather than
/// standing in the side list as a page of its own — which is what installing
/// five screensavers would otherwise do to it.
pub const fn pages() -> &'static [Area] {
    AREAS
}

/// Every setting, in each area's order.
pub fn all() -> Vec<Setting> {
    [
        appearance::settings(),
        wallpaper::settings(),
        keyboard::settings(),
        input::settings(),
        wifi::settings(),
        power::settings(),
        screensaver::settings(),
        updates::settings(),
    ]
    .concat()
}

/// A setting's current value.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    match s.area() {
        "appearance" if s.id == wallpaper::ID => Ok(wallpaper::get(env)),
        "appearance" => Ok(appearance::get(env, s)),
        "keyboard" if s.id == "keyboard.layout" => keyboard::get(env, s),
        "keyboard" | "touchpad" | "mouse" => input::get(env, s),
        "wifi" => wifi::get(env),
        "power" => power::get(env, s),
        a if a == "screensaver" || screensaver::owns(a) => screensaver::get(env, s),
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
        "appearance" if s.id == wallpaper::ID => wallpaper::set(env, s, value),
        "appearance" => none(appearance::set(env, s, value)),
        "keyboard" if s.id == "keyboard.layout" => keyboard::set(env, s, value, force),
        "keyboard" | "touchpad" | "mouse" => none(input::set(env, s, value, force)),
        "wifi" => none(wifi::set(env, value)),
        "power" => none(power::set(env, s, value)),
        a if a == "screensaver" || screensaver::owns(a) => none(screensaver::set(env, s, value)),
        "updates" => none(updates::set(env, value)),
        other => Err(format!("no area `{other}`")),
    }
}

/// Show a change in the running session, from the person's own process.
pub fn live(env: &Env, s: &Setting, value: &Value) -> Result<(), String> {
    match s.area() {
        "appearance" if s.id == wallpaper::ID => wallpaper::live(env),
        "keyboard" if s.id == "keyboard.layout" => keyboard::live(env, value),
        "keyboard" | "touchpad" | "mouse" => input::live(env, s, value),
        _ => Ok(()),
    }
}

//! Every area, and which one answers for a setting.

pub mod ai;
pub mod appearance;
pub mod bluetooth;
pub mod clipboard;
pub mod datetime;
pub mod default_apps;
pub mod displays;
pub mod input;
pub mod keyboard;
pub mod language;
pub mod notifications;
pub mod power;
pub mod screensaver;
pub mod sound;
pub mod ssh;
pub mod startup;
pub mod system;
pub mod themed;
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
        id: "displays",
        title: "Displays",
        description: "Where each screen is, and its resolution, scale and rotation",
        icon: "\u{f0379}",
        keywords: &[
            "screen",
            "screens",
            "monitor",
            "monitors",
            "resolution",
            "refresh",
            "scale",
            "rotation",
            "arrange",
            "dock",
            "projector",
            "external",
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
        id: "language",
        title: "Language",
        description: "The language programs speak, and their translations",
        icon: "\u{f05ca}",
        keywords: &["locale", "translation", "region", "lang"],
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
        id: "sound",
        title: "Sound",
        description: "Speakers, headphones and microphones, and how loud",
        icon: "\u{f057e}",
        keywords: &["audio", "volume", "speakers", "microphone", "pipewire"],
    },
    Area {
        id: "wifi",
        title: "Wi-Fi",
        description: "Wireless networks",
        icon: "\u{f05a9}",
        keywords: &["network", "wireless", "internet"],
    },
    Area {
        id: "bluetooth",
        title: "Bluetooth",
        description: "Headphones, keyboards and other wireless devices",
        icon: "\u{f00af}",
        keywords: &["bluez", "pair", "headphones", "wireless"],
    },
    Area {
        id: "ssh",
        title: "SSH",
        description: "Logging in to this computer from another one",
        icon: "\u{f08c0}",
        keywords: &["sshd", "openssh", "remote", "server"],
    },
    Area {
        id: "power",
        title: "Power",
        description: "The lid, the power button, the battery and power modes",
        icon: "\u{f0079}",
        keywords: &["battery", "suspend", "sleep", "laptop"],
    },
    Area {
        id: "notifications",
        title: "Notifications",
        description: "Do not disturb, and where and for how long notifications show",
        icon: "\u{f009a}",
        keywords: &["mako", "popups", "alerts", "dnd", "do not disturb"],
    },
    Area {
        id: "ai",
        title: "AI usage",
        description: "Which AI subscriptions and API budgets the bar watches, and when it warns",
        icon: "\u{f06a9}",
        keywords: &[
            "ai",
            "claude",
            "openai",
            "openrouter",
            "subscription",
            "api",
            "limit",
            "usage",
            "budget",
            "tokens",
        ],
    },
    Area {
        id: "clipboard",
        title: "Clipboard",
        description: "Copy and paste on Super+C and Super+V, and a history of what was copied",
        icon: "\u{f0147}",
        keywords: &["copy", "paste", "clipboard history", "super+v"],
    },
    Area {
        id: "default",
        title: "Default applications",
        description: "Which application opens links, files and folders, and which terminal runs commands",
        icon: "\u{f003b}",
        keywords: &[
            "default",
            "browser",
            "open with",
            "mime",
            "file associations",
            "terminal",
        ],
    },
    Area {
        id: "startup",
        title: "Startup",
        description: "The programs that start when you log in",
        icon: "\u{f14de}",
        keywords: &["autostart", "login", "start at login", "launch", "programs"],
    },
    Area {
        id: "screensaver",
        title: "Screensaver",
        description: "Which screensaver appears when nobody is there, and when the screen turns off",
        icon: "\u{f0594}",
        keywords: &["screen saver", "idle", "blank", "lock", "timeout"],
    },
    Area {
        id: "datetime",
        title: "Date & time",
        description: "The time zone, and keeping the clock right",
        icon: "\u{f0150}",
        keywords: &["time zone", "timezone", "clock", "ntp", "date"],
    },
    Area {
        id: "updates",
        title: "Updates",
        description: "The release channel Alpymist follows",
        icon: "\u{f06b0}",
        keywords: &["upgrade", "channel", "packages"],
    },
    Area {
        id: "system",
        title: "System",
        description: "What this computer is called, your password, and who administers it",
        icon: "\u{f01c4}",
        keywords: &[
            "computer",
            "hostname",
            "name",
            "account",
            "password",
            "administrator",
        ],
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
        displays::settings(),
        keyboard::settings(),
        language::settings(),
        input::settings(),
        sound::settings(),
        wifi::settings(),
        bluetooth::settings(),
        ssh::settings(),
        power::settings(),
        notifications::settings(),
        ai::settings(),
        clipboard::settings(),
        default_apps::settings(),
        startup::settings(),
        screensaver::settings(),
        datetime::settings(),
        updates::settings(),
        system::settings(),
    ]
    .concat()
}

/// A setting's current value.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    match s.area() {
        "appearance" if s.id == wallpaper::ID => Ok(wallpaper::get(env)),
        "appearance" => Ok(appearance::get(env, s)),
        "displays" => Ok(displays::get(env, s)),
        "keyboard" if s.id == "keyboard.layout" => keyboard::get(env, s),
        "language" => language::get(env, s),
        "keyboard" | "touchpad" | "mouse" => input::get(env, s),
        "sound" => sound::get(env, s),
        "wifi" => wifi::get(env),
        "bluetooth" => Ok(bluetooth::get(env)),
        "ssh" => Ok(ssh::get(env)),
        "power" => power::get(env, s),
        "notifications" => notifications::get(env, s),
        "ai" => Ok(ai::get(env, s)),
        "clipboard" => Ok(clipboard::get(env, s)),
        "default" => default_apps::get(env, s),
        "startup" => startup::get(env, s),
        a if a == "screensaver" || screensaver::owns(a) => screensaver::get(env, s),
        "datetime" => datetime::get(env, s),
        "updates" => updates::get(env),
        "system" => system::get(env, s),
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
        "appearance" => appearance::set(env, s, value),
        "displays" => displays::set(env, s, value),
        "keyboard" if s.id == "keyboard.layout" => keyboard::set(env, s, value, force),
        "language" => language::set(env, s, value),
        "keyboard" | "touchpad" | "mouse" => none(input::set(env, s, value, force)),
        "sound" => none(sound::set(env, s, value)),
        "wifi" => none(wifi::set(env, value)),
        "bluetooth" => none(bluetooth::set(env, value)),
        "ssh" => ssh::set(env, value),
        "power" => none(power::set(env, s, value)),
        "notifications" => notifications::set(env, s, value, force),
        "ai" => ai::set(env, s, value),
        "clipboard" => clipboard::set(env, s, value),
        "default" => default_apps::set(env, s, value),
        "startup" => none(startup::set(env, s, value)),
        a if a == "screensaver" || screensaver::owns(a) => none(screensaver::set(env, s, value)),
        "datetime" => none(datetime::set(env, s, value, force)),
        "updates" => none(updates::set(env, value)),
        "system" => none(system::set(env, s, value)),
        other => Err(format!("no area `{other}`")),
    }
}

/// Show a change in the running session, from the person's own process.
pub fn live(env: &Env, s: &Setting, value: &Value) -> Result<(), String> {
    match s.area() {
        "appearance" if s.id == wallpaper::ID => wallpaper::live(env),
        "appearance" => {
            appearance::live(env, s);
            Ok(())
        }
        "displays" => displays::live(env),
        "keyboard" if s.id == "keyboard.layout" => keyboard::live(env, value),
        "keyboard" | "touchpad" | "mouse" => input::live(env, s, value),
        "notifications" => notifications::live(env, s),
        "clipboard" => clipboard::live(env, s),
        "datetime" => {
            datetime::live(env, s);
            Ok(())
        }
        _ => Ok(()),
    }
}

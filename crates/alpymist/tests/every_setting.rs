//! The manual's "Every setting" page, checked against the registry.
//!
//! `docs/manual/26-every-setting.md` lists every setting with what it means,
//! what it can be and what it starts as. Nothing ties that page to the
//! registry it describes, so this does: the page is made here from
//! `alpymist list --json`, and the test fails when the file in the repository
//! is not what that makes. A setting added, renamed or given another default
//! then fails the build until the page says so too.
//!
//! To write the page again after changing a setting:
//!
//! ```sh
//! ALPYMIST_BLESS=1 cargo test -p alpymist --test every_setting
//! ```
//!
//! The list is asked of the real program, run with nothing of this machine's
//! in its environment: the screensavers and AI usage providers are the ones
//! this commit ships, read from `desktop/`, and the home directory is an
//! empty one. What still differs from one machine to the next, such as the
//! wallpapers installed or the applications that open a PDF, is described on
//! the page rather than listed.

use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The page, from the repository's root.
const PAGE: &str = "docs/manual/26-every-setting.md";

/// What Default applications adds to a description when nothing installed
/// opens that kind of thing, as is so in the empty home this runs in.
const NOTHING_INSTALLED: &str =
    " Nothing installed opens these yet; the store has applications that do.";

/// The areas' headings. One not named here is headed by its id, so a new area
/// shows on the page without this list being touched.
const HEADINGS: [(&str, &str); 22] = [
    ("appearance", "Appearance"),
    ("displays", "Displays"),
    ("keyboard", "Keyboard"),
    ("language", "Language"),
    ("touchpad", "Touchpad"),
    ("mouse", "Mouse and pointer"),
    ("sound", "Sound"),
    ("wifi", "Wi-Fi"),
    ("bluetooth", "Bluetooth"),
    ("ssh", "SSH"),
    ("power", "Power"),
    ("notifications", "Notifications"),
    ("ai", "AI usage"),
    ("clipboard", "Clipboard"),
    ("default", "Default applications"),
    ("startup", "Startup"),
    ("screensaver", "Screensaver"),
    ("screensaver-mountains", "Screensaver: Mountains"),
    ("screensaver-starfield", "Screensaver: Starfield"),
    ("datetime", "Date and time"),
    ("updates", "Updates"),
    ("system", "System"),
];

/// Settings whose choices are this machine's, and what the page says instead
/// of listing them.
const BY_MACHINE: [(&str, &str); 8] = [
    ("appearance.wallpaper", "The pictures installed"),
    ("keyboard.layout", "Every XKB layout"),
    ("language.language", "The languages there are"),
    ("sound.output", "Automatic, or a device"),
    ("sound.input", "Automatic, or a device"),
    ("datetime.timezone", "Every time zone"),
    ("screensaver.main-screen", "The screens connected"),
    ("startup.add", "The applications installed"),
];

/// Settings that ask for an administrator's password although they are the
/// account's own, which the registry's scope does not say.
const ASKS: [&str; 1] = ["power.charge-limit"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every setting, as `alpymist list --json` gives them here. Asked once: the
/// tests run side by side and share the scratch directory.
fn settings() -> &'static [Value] {
    static SETTINGS: OnceLock<Vec<Value>> = OnceLock::new();
    SETTINGS.get_or_init(list)
}

fn list() -> Vec<Value> {
    let root = root();
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("every-setting");
    let _ = std::fs::remove_dir_all(&scratch);
    // Screensavers are found under XDG_DATA_HOME; the shipped ones are put
    // where an account's own would be.
    let savers = scratch.join("data/alpymist/screensavers");
    std::fs::create_dir_all(&savers).expect("a scratch directory");
    std::fs::create_dir_all(scratch.join("home")).expect("a scratch home");
    for entry in std::fs::read_dir(root.join("desktop/screensavers")).expect("desktop/screensavers")
    {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|e| e == "toml") {
            let name = path.file_name().expect("a file name");
            std::fs::copy(&path, savers.join(name)).expect("copying a definition");
        }
    }
    let output = Command::new(env!("CARGO_BIN_EXE_alpymist"))
        .args(["list", "--json"])
        .env_clear()
        .env("HOME", scratch.join("home"))
        .env("XDG_CONFIG_HOME", scratch.join("home/.config"))
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("ALPYMIST_AI_USAGE_DIR", root.join("desktop/ai-usage"))
        .output()
        .expect("running alpymist");
    assert!(
        output.status.success(),
        "alpymist list --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("alpymist list --json prints JSON")
}

fn text<'a>(setting: &'a Value, key: &str) -> &'a str {
    setting[key].as_str().unwrap_or_default()
}

/// A cell of a table: a `|` in it would end the cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// What a setting can be.
fn values(setting: &Value) -> String {
    let id = text(setting, "id");
    if let Some((_, says)) = BY_MACHINE.iter().find(|(known, _)| *known == id) {
        return (*says).to_owned();
    }
    if id.starts_with("default.") {
        return "The applications that open these".to_owned();
    }
    let kind = &setting["kind"];
    match text(kind, "type") {
        "switch" => "`true`, `false`".to_owned(),
        "choice" => kind["choices"]
            .as_array()
            .map(|choices| {
                choices
                    .iter()
                    .map(|c| text(c, "value"))
                    .filter(|v| !v.is_empty())
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        "number" => {
            let unit = text(kind, "unit");
            let unit = if unit.is_empty() {
                String::new()
            } else {
                format!(" {unit}")
            };
            format!("{} to {}{unit}", kind["min"], kind["max"])
        }
        "text" => format!("Text, up to {} characters", kind["max"]),
        "action" => "An action".to_owned(),
        other => other.to_owned(),
    }
}

/// What a setting starts as, where that is the same on every machine.
fn default(setting: &Value) -> String {
    let id = text(setting, "id");
    if text(&setting["kind"], "type") == "action"
        || id.starts_with("default.")
        || matches!(id, "startup.add" | "screensaver.main-screen")
    {
        return String::new();
    }
    match &setting["default"] {
        Value::Bool(b) => format!("`{b}`"),
        Value::Number(n) => format!("`{n}`"),
        Value::String(s) if !s.is_empty() => format!("`{s}`"),
        _ => String::new(),
    }
}

/// What follows a setting's id: whose it is, and when a change shows.
fn notes(setting: &Value) -> String {
    let mut notes = Vec::new();
    if text(setting, "scope") == "system" {
        notes.push("System");
    }
    if ASKS.contains(&text(setting, "id")) {
        notes.push("Asks for an administrator's password");
    }
    match text(setting, "applies") {
        "new-windows" => notes.push("New windows"),
        "next-login" => notes.push("Next login"),
        "next-update" => notes.push("Next update"),
        _ => {}
    }
    notes.iter().fold(String::new(), |mut out, note| {
        let _ = write!(out, " · {note}");
        out
    })
}

/// The page.
fn page(settings: &[Value]) -> String {
    let mut out = String::from(
        "# Every setting\n\
         <!-- group: Reference -->\n\
         \n\
         Every setting Alpymist has, by area. Each can be changed in the Settings app,\n\
         found by its title in the menu's search, or read and changed from a terminal:\n\
         \n\
         ```sh\n\
         alpymist get power.lid\n\
         alpymist set power.lid lock\n\
         alpymist reset power.lid\n\
         alpymist list power.lid      # its value, and what it can be\n\
         ```\n\
         \n\
         **System** marks a setting that changes the machine for everyone and asks for\n\
         an administrator's password; the rest are your account's own. Where a change\n\
         does not apply at once, the setting says when.\n\
         \n\
         This page is made from the settings themselves, and a test keeps it so: it\n\
         is what `alpymist list` prints on a system with nothing added. Yours also\n\
         lists the settings that packages you installed have added.\n",
    );
    // Areas in the order their first setting comes in the registry.
    let mut areas: Vec<&str> = Vec::new();
    for setting in settings {
        let area = text(setting, "area");
        if !areas.contains(&area) {
            areas.push(area);
        }
    }
    for area in areas {
        let heading = HEADINGS
            .iter()
            .find(|(id, _)| *id == area)
            .map_or(area, |(_, heading)| heading);
        let _ = write!(out, "\n## {heading}\n\n");
        match area {
            "startup" => out.push_str(
                "Each program that a package installs to start at login also has a switch\n\
                 here, named `startup.<program>`.\n\n",
            ),
            "ai" => out.push_str(
                "Each provider installed has a switch, `ai.provider-<id>`; these are the ones\n\
                 shipped.\n\n",
            ),
            _ => {}
        }
        out.push_str("| Setting | Values | Default |\n|---|---|---|\n");
        for setting in settings.iter().filter(|s| text(s, "area") == area) {
            let id = text(setting, "id");
            // A program's switch under Startup is one machine's, not the
            // registry's.
            if area == "startup" && id != "startup.add" {
                continue;
            }
            let description = text(setting, "description");
            let description = description
                .strip_suffix(NOTHING_INSTALLED)
                .unwrap_or(description);
            let _ = writeln!(
                out,
                "| **{}**<br>{}<br>`{id}`{} | {} | {} |",
                cell(text(setting, "title")),
                cell(description),
                notes(setting),
                values(setting),
                default(setting),
            );
        }
    }
    out
}

#[test]
fn the_manual_lists_every_setting_as_it_is() {
    let made = page(settings());
    let path = root().join(PAGE);
    if std::env::var_os("ALPYMIST_BLESS").is_some() {
        std::fs::write(&path, &made).expect("writing the page");
        return;
    }
    let kept = std::fs::read_to_string(&path).expect("docs/manual/26-every-setting.md");
    // The first line that differs says more than two pages of text would.
    let differs = made
        .lines()
        .zip(kept.lines())
        .enumerate()
        .find(|(_, (made, kept))| made != kept);
    let message = match differs {
        Some((n, (made, kept))) => format!(
            "line {}:\n  the settings say: {made}\n  the page says:    {kept}",
            n + 1
        ),
        None => format!(
            "the settings make {} lines and the page has {}",
            made.lines().count(),
            kept.lines().count()
        ),
    };
    assert!(
        made == kept,
        "{PAGE} is not what the settings are now.\n{message}\n\
         Write it again with: ALPYMIST_BLESS=1 cargo test -p alpymist --test every_setting"
    );
}

#[test]
fn every_area_has_a_heading_of_its_own() {
    // An area headed by its bare id still shows; this only says so, so the
    // heading can be given a proper name in the same change.
    let settings = settings();
    let unnamed: Vec<&str> = settings
        .iter()
        .map(|s| text(s, "area"))
        .filter(|area| !HEADINGS.iter().any(|(id, _)| id == area))
        .collect();
    assert!(
        unnamed.is_empty(),
        "no heading in HEADINGS for: {unnamed:?}"
    );
}

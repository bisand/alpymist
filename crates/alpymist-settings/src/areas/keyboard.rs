//! The keyboard layout: the system's, for the console and every desktop.
//!
//! The value is a console keymap from Alpine's `kbd-bkeymaps`, as the installer
//! offers them (`no-mac`). Setting it writes `/etc/alpymist/settings.toml`, and
//! from it the files the installer wrote: `hyprland-keyboard.conf` for
//! Hyprland, `session.env` for labwc and i3, and the console's keymap through
//! `setup-keymap`. The person's running Hyprland is told with
//! `hyprctl keyword`.

use crate::env::Env;
use crate::generated;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use crate::values::Values;
use alpymist_core::catalog::{self, xkb_variant};

/// The system's values.
pub const VALUES: &str = "etc/alpymist/settings.toml";
/// Hyprland's keyboard block, sourced by every account's configuration.
pub const HYPRLAND: &str = "etc/alpymist/hyprland-keyboard.conf";
/// What the login's PAM reads into the session.
pub const SESSION_ENV: &str = "etc/alpymist/session.env";

/// The settings.
pub fn settings() -> Vec<Setting> {
    let choices = catalog::keymaps()
        .iter()
        .map(|k| Choice::new(k.variant, format!("{} ({})", k.name, k.variant)))
        .collect();
    vec![Setting {
        id: "keyboard.layout",
        title: "Keyboard layout",
        description: "The layout every desktop, the login screen and the console type with.",
        keywords: &["language", "keymap", "xkb", "qwerty", "azerty", "dvorak"],
        kind: Kind::Choice(choices),
        default: Value::Text(catalog::DEFAULT_KEYMAP.1.into()),
        scope: Scope::System,
        applies: Applies::Now,
    }]
}

/// The layout and xkb variant for a keymap value: `no-mac` is `no`, `mac`.
fn xkb(keymap: &str) -> Option<(&'static str, &'static str)> {
    let k = catalog::keymaps().iter().find(|k| k.variant == keymap)?;
    Some((k.layout, xkb_variant(k.layout, k.variant)))
}

/// The layout set, else the one the installer recorded, else US.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    if let Some(v) = Values::load(&env.system(VALUES))?.get(setting.id) {
        return Ok(v);
    }
    let recorded = std::fs::read_to_string(env.system(SESSION_ENV)).unwrap_or_default();
    let field = |key: &str| {
        recorded
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_owned())
    };
    if let Some(layout) = field("XKB_DEFAULT_LAYOUT=") {
        let variant = field("XKB_DEFAULT_VARIANT=").unwrap_or_default();
        // The plain layout when there is no variant; otherwise the keymap whose
        // variant it is.
        let found = catalog::keymaps().iter().find(|k| {
            k.layout == layout
                && xkb_variant(k.layout, k.variant) == variant
                && (!variant.is_empty() || k.variant == k.layout)
        });
        if let Some(k) = found {
            return Ok(Value::Text(k.variant.into()));
        }
    }
    Ok(setting.default.clone())
}

/// The installer's own `hyprland-keyboard.conf`, or the package's default:
/// only comments and the layout lines.
fn adopt_hyprland(text: &str) -> bool {
    text.contains("Alpymist installer")
        && text.lines().all(|l| {
            let l = l.trim();
            l.is_empty()
                || l.starts_with('#')
                || matches!(l, "input {" | "}")
                || l.starts_with("kb_layout")
                || l.starts_with("kb_variant")
        })
}

/// The installer's `session.env`: the layout and nothing else.
fn adopt_session(text: &str) -> bool {
    text.lines()
        .all(|l| l.trim().is_empty() || l.starts_with("XKB_DEFAULT_"))
}

/// Write the layout, as root.
pub fn set(
    env: &Env,
    setting: &Setting,
    value: Option<&Value>,
    force: bool,
) -> Result<Vec<String>, String> {
    let keymap = value
        .and_then(Value::as_text)
        .unwrap_or(catalog::DEFAULT_KEYMAP.1);
    let (layout, variant) = xkb(keymap).ok_or_else(|| format!("no keymap `{keymap}`"))?;
    let source = "/etc/alpymist/settings.toml";
    generated::write(
        &env.system(HYPRLAND),
        "#",
        source,
        &format!("input {{\n    kb_layout = {layout}\n    kb_variant = {variant}\n}}\n"),
        adopt_hyprland,
        force,
    )?;
    generated::write(
        &env.system(SESSION_ENV),
        "#",
        source,
        &format!("XKB_DEFAULT_LAYOUT={layout}\nXKB_DEFAULT_VARIANT={variant}\n"),
        adopt_session,
        force,
    )?;
    let path = env.system(VALUES);
    let mut values = Values::load(&path)?;
    match value {
        Some(v) => values.set(setting.id, v),
        None => values.remove(setting.id),
    }
    values.save(&path)?;
    // The console matters less than the desktop: say so rather than fail.
    let mut notes = Vec::new();
    if let Err(e) = env.run(&["setup-keymap", layout, keymap]) {
        notes.push(format!(
            "The console keeps its layout until the next boot: {e}"
        ));
    }
    Ok(notes)
}

/// Tell the running Hyprland.
pub fn live(env: &Env, value: &Value) -> Result<(), String> {
    if !env.hyprland {
        return Ok(());
    }
    let (layout, variant) = value.as_text().and_then(xkb).ok_or("not a keymap")?;
    env.run(&[
        "hyprctl",
        "--batch",
        &format!("keyword input:kb_layout {layout} ; keyword input:kb_variant {variant}"),
    ])
    .map(drop)
}

#[cfg(test)]
mod tests {
    use super::{adopt_hyprland, adopt_session, xkb};

    #[test]
    fn keymaps_name_their_xkb_layout() {
        assert_eq!(xkb("no-mac"), Some(("no", "mac")));
        assert_eq!(xkb("us"), Some(("us", "")));
        assert_eq!(xkb("klingon"), None);
    }

    #[test]
    fn what_the_installer_wrote_is_taken_over_and_nothing_else() {
        let installer = "# Written by the Alpymist installer: the keyboard layout chosen at install.\n\
                         input {\n    kb_layout = no\n    kb_variant = \n}\n";
        assert!(adopt_hyprland(installer));
        let package = include_str!("../../../../desktop/hypr/hyprland-keyboard.conf");
        assert!(adopt_hyprland(package));
        assert!(!adopt_hyprland(&format!(
            "{installer}bind = SUPER, K, exec, x\n"
        )));
        assert!(adopt_session(
            "XKB_DEFAULT_LAYOUT=no\nXKB_DEFAULT_VARIANT=\n"
        ));
        assert!(!adopt_session("XKB_DEFAULT_LAYOUT=no\nLANG=nb_NO.UTF-8\n"));
    }

    #[test]
    fn every_keymap_value_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for k in alpymist_core::catalog::keymaps() {
            assert!(seen.insert(k.variant), "{} appears twice", k.variant);
        }
    }
}

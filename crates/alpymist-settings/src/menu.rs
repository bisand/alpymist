//! The menu fragment: every area and setting as a menu entry that opens the
//! settings app there (ADR 0007 §6).
//!
//! Generated when the settings app is packaged and installed to
//! `/usr/share/alpymist/menu.d/settings.toml`, so what the menu finds is always
//! what the installed app has.

use crate::Settings;
use std::fmt::Write as _;

/// A TOML string.
fn quote(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

/// The fragment's text: a `[menu.settings]` of areas, and a submenu per area
/// of its settings.
#[must_use]
pub fn fragment(settings: &Settings) -> String {
    let mut out = String::from(
        "# Generated from alpymist-settings' registry when it was packaged: every\n\
         # setting, found by the menu's search, opening Settings at it.\n\n\
         [menu.settings]\ntitle = \"Settings\"\nitems = [\n",
    );
    for area in settings.areas() {
        let _ = writeln!(
            out,
            "  {{ name = {}, icon = {}, menu = {}, detail = {}, keywords = [{}] }},",
            quote(area.title),
            quote(area.icon),
            quote(&format!("settings-{}", area.id)),
            quote(area.description),
            area.keywords
                .iter()
                .map(|k| quote(k))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    out.push_str("]\n");
    for area in settings.areas() {
        let _ = write!(
            out,
            "\n[menu.settings-{}]\ntitle = {}\nitems = [\n  {{ name = {}, icon = {}, exec = {}, detail = \"All of it\" }},\n",
            area.id,
            quote(area.title),
            quote(&format!("{} settings", area.title)),
            quote(area.icon),
            quote(&format!("alpymist-settings {}", area.id)),
        );
        for s in settings.in_area(area.id) {
            let _ = writeln!(
                out,
                "  {{ name = {}, icon = {}, exec = {}, keywords = [{}] }},",
                quote(s.title),
                quote(area.icon),
                quote(&format!("alpymist-settings {}", s.id)),
                s.keywords
                    .iter()
                    .map(|k| quote(k))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        out.push_str("]\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::fragment;
    use crate::Settings;

    #[test]
    fn the_fragment_is_toml_with_an_entry_per_setting() {
        let settings = Settings::new();
        let text = fragment(&settings);
        let parsed: toml::Table = text.parse().unwrap_or_else(|e| panic!("{e}\n{text}"));
        let menus = parsed["menu"].as_table().unwrap();
        assert_eq!(
            menus["settings"]["items"].as_array().unwrap().len(),
            settings.areas().len()
        );
        let touchpad = menus["settings-touchpad"]["items"].as_array().unwrap();
        assert!(
            touchpad.iter().any(|i| {
                i["exec"].as_str() == Some("alpymist-settings touchpad.natural-scroll")
            })
        );
    }
}

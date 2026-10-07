//! GTK 3 in the theme's colours.
//!
//! GTK's own theme, Adwaita, is a grey of its own and a blue of its own,
//! whatever is chosen in Settings › Appearance, and a file manager drawn in
//! it sits in the desktop like a window from another system. This is a
//! stylesheet for `~/.config/gtk-3.0/gtk.css`, which GTK 3 reads after its
//! theme: the page, the lines, the text and the accent of the account's
//! theme, on the things GTK 3 programs here are made of. It is Adwaita
//! underneath, with its sizes and its shapes, so what this does not name
//! still looks like something.
//!
//! GTK 4 is not written for: its programs here are libadwaita's, which
//! follow the scheme and the accent through `gsettings` and are styled by
//! libadwaita, not by a stylesheet like this one.

use alpymist_theme::{ThemeFile, hex, mix};
use std::fmt::Write as _;

/// The stylesheet, under the account's configuration.
pub const CSS: &str = "gtk-3.0/gtk.css";

/// What each rule is written in: the theme's colours by name, so the rules
/// read as what they are and a theme is nine lines.
const RULES: &str = r"
@define-color theme_bg_color @alpymist_page;
@define-color theme_fg_color @alpymist_text;
@define-color theme_base_color @alpymist_page;
@define-color theme_text_color @alpymist_text;
@define-color theme_selected_bg_color @alpymist_border;
@define-color theme_selected_fg_color @alpymist_text;
@define-color borders @alpymist_border;

* { outline-color: alpha(@alpymist_accent, 0.6); text-shadow: none; -gtk-icon-shadow: none; }

window, .background, dialog, messagedialog { background-color: @alpymist_page; color: @alpymist_text; }
label { color: inherit; }
label:disabled, .dim-label { color: @alpymist_muted; }
*:link, button.link { color: @alpymist_accent; }

.view, treeview.view, iconview, list, textview text {
  background-color: @alpymist_page; color: @alpymist_text;
}
.view:selected, treeview.view:selected, iconview:selected, list row:selected,
textview text selection, entry selection, label selection {
  background-color: @alpymist_border; color: @alpymist_text;
}
treeview.view:hover { background-color: @alpymist_well; }
treeview.view:selected:hover { background-color: @alpymist_border; }

treeview.view header button {
  background-color: @alpymist_page; background-image: none; color: @alpymist_dim;
  border: none; border-bottom: 1px solid @alpymist_border; border-radius: 0;
  box-shadow: none; font-weight: normal; padding: 4px 6px;
}
treeview.view header button:hover { color: @alpymist_text; background-color: @alpymist_well; }

.sidebar, .sidebar .view, .sidebar treeview.view, .sidebar scrolledwindow, placessidebar, placessidebar list {
  background-color: @alpymist_side; color: @alpymist_text;
}
.sidebar .view:selected, .sidebar treeview.view:selected, placessidebar row:selected {
  background-color: @alpymist_border; color: @alpymist_text;
}

toolbar, .toolbar, menubar, .menubar, headerbar, .titlebar, statusbar, .statusbar, actionbar {
  background-color: @alpymist_page; background-image: none; color: @alpymist_text;
  border-color: @alpymist_border; box-shadow: none;
}
statusbar, .statusbar { color: @alpymist_dim; }
menubar > menuitem:hover, .menubar > menuitem:hover { background-color: @alpymist_raised; border-radius: 6px; }

button {
  background-color: transparent; background-image: none; color: @alpymist_text;
  border: 1px solid transparent; border-radius: 8px; box-shadow: none;
}
button:hover { background-color: @alpymist_raised; }
button:active, button:checked { background-color: @alpymist_border; }
button:disabled { color: @alpymist_muted; background-color: transparent; }
button.text-button, dialog button, .dialog-action-area button {
  border-color: @alpymist_border; padding: 4px 12px;
}
button.suggested-action { background-color: @alpymist_accent; color: @alpymist_page; border-color: @alpymist_accent; }
button.destructive-action { border-color: @alpymist_error; color: @alpymist_error; }

entry, spinbutton {
  background-color: @alpymist_well; background-image: none; color: @alpymist_text;
  border: 1px solid @alpymist_border; border-radius: 8px; box-shadow: none; caret-color: @alpymist_accent;
}
entry:focus, spinbutton:focus { border-color: @alpymist_accent; box-shadow: none; }

.linked > button:not(:first-child):not(:last-child), .linked > entry:not(:first-child):not(:last-child) { border-radius: 0; }
.linked > button:first-child, .linked > entry:first-child { border-radius: 8px 0 0 8px; }
.linked > button:last-child, .linked > entry:last-child { border-radius: 0 8px 8px 0; }
.linked > button:only-child, .linked > entry:only-child { border-radius: 8px; }

menu, .menu, .context-menu, popover, popover.background {
  background-color: @alpymist_well; color: @alpymist_text;
  border: 1px solid @alpymist_border; border-radius: 10px; padding: 4px; box-shadow: none;
}
menuitem, modelbutton { border-radius: 6px; padding: 5px 10px; color: @alpymist_text; }
menuitem:hover, modelbutton:hover { background-color: @alpymist_border; color: @alpymist_text; }
menuitem:disabled { color: @alpymist_muted; }
menu separator, popover separator, separator { background-color: @alpymist_border; min-height: 1px; min-width: 1px; }
menuitem accelerator { color: @alpymist_dim; }

paned > separator { background-color: @alpymist_border; background-image: none; min-width: 1px; min-height: 1px; }

scrollbar { background-color: transparent; border: none; }
scrollbar slider { background-color: @alpymist_border; border-radius: 8px; min-width: 6px; min-height: 6px; border: 3px solid transparent; }
scrollbar slider:hover { background-color: @alpymist_muted; }
scrollbar slider:active { background-color: @alpymist_accent; }

check, radio { background-color: @alpymist_well; background-image: none; border: 1px solid @alpymist_border; box-shadow: none; }
check:checked, radio:checked { background-color: @alpymist_accent; border-color: @alpymist_accent; color: @alpymist_page; }
switch { background-color: @alpymist_border; border: none; border-radius: 14px; }
switch:checked { background-color: @alpymist_accent; }
switch slider { background-color: @alpymist_text; border: none; border-radius: 14px; box-shadow: none; }
progressbar trough, scale trough { background-color: @alpymist_border; border: none; border-radius: 4px; }
progressbar progress, scale highlight { background-color: @alpymist_accent; border: none; border-radius: 4px; }

notebook, notebook > header, notebook > stack { background-color: @alpymist_page; border-color: @alpymist_border; }
notebook > header tab { color: @alpymist_dim; border-radius: 8px 8px 0 0; padding: 4px 12px; }
notebook > header tab:checked { color: @alpymist_text; background-color: @alpymist_well; box-shadow: inset 0 -2px @alpymist_accent; }

tooltip, tooltip.background { background-color: @alpymist_page; color: @alpymist_text; border: 1px solid @alpymist_border; border-radius: 8px; }
frame > border, .frame { border-color: @alpymist_border; }
infobar, .info { background-color: @alpymist_well; color: @alpymist_text; }
";

/// The theme's colours by the names [`RULES`] uses. Beside the palette's
/// own: `side`, a file manager's sidebar, a shade off the page; and
/// `raised`, what the pointer is over.
fn colours(theme: &ThemeFile) -> Vec<(&'static str, String)> {
    let p = theme.palette();
    [
        ("page", p.page),
        ("well", p.well),
        ("side", mix(p.page, p.border, 30)),
        ("raised", mix(p.page, p.border, 45)),
        ("border", p.border),
        ("text", p.text),
        ("dim", p.dim),
        ("accent", p.accent),
        ("muted", p.muted),
        ("error", p.error),
    ]
    .into_iter()
    .map(|(name, colour)| (name, hex(colour)))
    .collect()
}

/// The stylesheet for `theme`.
#[must_use]
pub fn css(theme: &ThemeFile) -> String {
    let mut out = String::with_capacity(RULES.len() + 400);
    for (name, colour) in colours(theme) {
        let _ = writeln!(out, "@define-color alpymist_{name} #{colour};");
    }
    out.push_str(RULES);
    out
}

#[cfg(test)]
mod tests {
    use super::{RULES, colours, css};
    use alpymist_theme::{Accent, Scheme, ThemeFile, hex};

    #[test]
    fn every_colour_a_rule_names_is_defined_and_every_one_defined_is_used() {
        let theme = ThemeFile::default();
        let defined: Vec<&str> = colours(&theme).iter().map(|(n, _)| *n).collect();
        let mut used = std::collections::BTreeSet::new();
        for part in RULES.split("@alpymist_").skip(1) {
            let name: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            assert!(defined.contains(&name.as_str()), "{name} is not defined");
            used.insert(name);
        }
        for name in defined {
            assert!(used.contains(name), "{name} is defined and never used");
        }
    }

    #[test]
    fn the_stylesheet_is_the_themes_colours_and_balanced() {
        for scheme in Scheme::ALL {
            for accent in Accent::ALL {
                let theme = ThemeFile {
                    scheme,
                    accent,
                    ..ThemeFile::default()
                };
                let p = theme.palette();
                let text = css(&theme);
                assert!(text.contains(&format!("@define-color alpymist_page #{};", hex(p.page))));
                assert!(text.contains(&format!(
                    "@define-color alpymist_accent #{};",
                    hex(p.accent)
                )));
                assert_eq!(text.matches('{').count(), text.matches('}').count());
                // Colours only by name below the definitions: a theme
                // changes nine lines and nothing else.
                assert!(!super::RULES.contains('#'));
            }
        }
    }
}

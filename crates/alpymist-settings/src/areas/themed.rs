//! The theme beyond Alpymist's own windows: the top bar, the terminal,
//! Hyprland's borders and GTK (ADR 0007 §7).
//!
//! Each gets a file of Alpymist's own, written from the account's theme:
//!
//! - `~/.config/alpymist/waybar/colours.css`, which the account's bar style
//!   imports after the packaged one, whose colours are named for it to
//!   redefine;
//! - `~/.config/alpymist/foot/theme.ini`, which the account's `foot.ini`
//!   includes: both palettes, and which one foot starts in;
//! - `~/.config/gtk-3.0/settings.ini` and `gtk-4.0`'s, GTK's dark preference,
//!   unless the account already has its own;
//! - Hyprland's border colours, in the `settings.conf` the input settings
//!   write.
//!
//! waybar and foot refuse to start when a file they import is missing, so
//! `alpymist session` writes these before the desktop starts, every login.
//!
//! The accounts' own files from before this imported nothing of the kind, and
//! Hyprland's set its own border colours after Alpymist's. The first time an
//! account changes its theme, the line each needs is added, or Hyprland's two
//! colour lines taken out, each file kept as it was beside it (ADR 0007's
//! addendum of 2026-09-27). New accounts start with the lines there.
//!
//! GTK and the portal learn the scheme and the accent from `gsettings`, which
//! libadwaita and Flatpak's apps follow at once; the bar reloads and the
//! terminals change palette on a signal.

use crate::env::Env;
use crate::generated;
use alpymist_theme::{Accent, Scheme, ThemeFile, hex};
use std::fmt::Write as _;

/// The bar's colours.
pub const WAYBAR_COLOURS: &str = "alpymist/waybar/colours.css";
/// The terminal's palettes.
pub const FOOT_THEME: &str = "alpymist/foot/theme.ini";
/// GTK 3's and GTK 4's settings.
pub const GTK: [&str; 2] = ["gtk-3.0/settings.ini", "gtk-4.0/settings.ini"];
/// The account's bar style, its terminal's configuration and Hyprland's.
pub const WAYBAR_STYLE: &str = "waybar/style.css";
/// The account's terminal configuration.
pub const FOOT_INI: &str = "foot/foot.ini";
/// The account's Hyprland configuration.
pub const HYPRLAND: &str = "hypr/hyprland.conf";
/// What a file Alpymist changed is kept beside it as.
pub const KEPT: &str = ".bak-theme";
/// A file of the account's, what it is to a person, and the change it needs.
type Edit = (&'static str, &'static str, fn(&str) -> Option<String>);

/// Where each generated file says it comes from.
const SOURCE: &str = "~/.config/alpymist/theme.toml";

/// The line in the account's bar style that imports the packaged one.
const PACKAGED_STYLE: &str = "@import url(\"file:///usr/share/alpymist/waybar/style.css\");";
/// The line that imports the colours, relative to the account's bar style.
pub const COLOURS_IMPORT: &str = "@import url(\"../alpymist/waybar/colours.css\");";
/// The line that includes the palettes in the account's `foot.ini`.
pub const FOOT_INCLUDE: &str = "include=~/.config/alpymist/foot/theme.ini";
/// Hyprland's border colours as every account's configuration had them.
const HYPRLAND_BORDERS: [&str; 2] = [
    "col.active_border = rgb(7fb8d9)",
    "col.inactive_border = rgb(3a4c63)",
];

/// The terminal's sixteen colours, then its background and text: dark as
/// every account's `foot.ini` had them, light from the light accents.
const TERMINAL: [(Scheme, [&str; 18]); 2] = [
    (
        Scheme::Dark,
        [
            "070c14", "d98f7f", "9fcf9f", "d9a07f", "7fb8d9", "b79fd9", "afc2d6", "9aabbd",
            "3a4c63", "e6a89a", "b8e0b8", "e6bb9a", "a3cde6", "cbb6e6", "d0dde9", "eaf0f6",
            "0b121e", "eaf0f6",
        ],
    ),
    (
        Scheme::Light,
        [
            "0b121e", "a14a3a", "3c7a3c", "9a5a2f", "2f6f95", "6a4f9a", "1f7a70", "c9d3de",
            "4e5f73", "b85c4a", "4d8f4d", "b06d3c", "3b82ad", "7d61b0", "2a8f83", "e6ecf2",
            "f4f7fa", "0b121e",
        ],
    ),
];

/// The bar's colours, each a name the packaged style uses.
fn colours_css(theme: &ThemeFile) -> String {
    let p = theme.palette();
    let mut out = String::new();
    for (name, colour) in [
        ("page", p.page),
        ("text", p.text),
        ("border", p.border),
        ("dim", p.dim),
        ("accent", p.accent),
        ("warning", p.warning),
        ("error", p.error),
        ("muted", p.muted),
    ] {
        let _ = writeln!(out, "@define-color alpymist-{name} #{};", hex(colour));
    }
    out
}

/// The terminal's palettes, and the one it starts in.
fn foot_theme(theme: &ThemeFile) -> String {
    let mut out = format!(
        "[main]\ninitial-color-theme={}\n",
        match theme.scheme {
            Scheme::Dark => "dark",
            Scheme::Light => "light",
        }
    );
    for (scheme, colours) in TERMINAL {
        let _ = write!(out, "\n[colors-{}]\n", scheme.id());
        let _ = writeln!(out, "background={}", colours[16]);
        let _ = writeln!(out, "foreground={}", colours[17]);
        for (i, c) in colours[..16].iter().enumerate() {
            let (kind, n) = if i < 8 {
                ("regular", i)
            } else {
                ("bright", i - 8)
            };
            let _ = writeln!(out, "{kind}{n}={c}");
        }
    }
    out
}

/// GTK's dark preference.
fn gtk(theme: &ThemeFile) -> String {
    format!(
        "[Settings]\ngtk-application-prefer-dark-theme={}\n",
        theme.scheme == Scheme::Dark
    )
}

/// Hyprland's borders, for the input settings' `settings.conf`.
#[must_use]
pub fn hyprland(theme: &ThemeFile) -> String {
    let p = theme.palette();
    format!(
        "general {{\n    col.active_border = rgb({})\n    col.inactive_border = rgb({})\n}}\n",
        hex(p.accent),
        hex(p.border)
    )
}

/// The accent as GNOME names the nearest of its own.
const fn gnome_accent(accent: Accent) -> &'static str {
    match accent {
        Accent::Mist => "blue",
        Accent::Fjord => "teal",
        Accent::Moss => "green",
        Accent::Amber => "orange",
        Accent::Heather => "purple",
        Accent::Rose => "red",
    }
}

/// Write the files, leaving any edited by hand. Returns what was left.
pub fn write(env: &Env, theme: &ThemeFile) -> Vec<String> {
    let mut left = Vec::new();
    let mut put = |path: &str, comment: &str, body: String| {
        let path = env.account(path);
        if let Err(e) = generated::write(&path, comment, SOURCE, &body, |_| false, false) {
            left.push(e);
        }
    };
    put(WAYBAR_COLOURS, "/*", colours_css(theme));
    put(FOOT_THEME, "#", foot_theme(theme));
    for file in GTK {
        // An account's own settings.ini is its own: a note, not a failure.
        put(file, "#", gtk(theme));
    }
    left
}

/// Give the account's own files the lines that make them follow the theme,
/// keeping each as it was beside it. Says what it did.
pub fn take_over(env: &Env) -> Vec<String> {
    let mut changed = Vec::new();
    let edits: [Edit; 3] = [
        (WAYBAR_STYLE, "the top bar", import_colours),
        (FOOT_INI, "the terminal", include_palettes),
        (HYPRLAND, "Hyprland's borders", drop_borders),
    ];
    for (file, what, edit) in edits {
        let path = env.account(file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(new) = edit(&text) else {
            continue;
        };
        let kept = generated::beside(&path, KEPT);
        if std::fs::write(&kept, &text).is_ok() && generated::replace(&path, &new).is_ok() {
            changed.push(format!("{what} ({} as it was)", kept.display()));
        }
    }
    if changed.is_empty() {
        Vec::new()
    } else {
        vec![format!("Now following the theme: {}.", changed.join("; "))]
    }
}

/// The bar style with the colours imported after the packaged style.
fn import_colours(style: &str) -> Option<String> {
    if style.contains(COLOURS_IMPORT) || !style.lines().any(|l| l.trim() == PACKAGED_STYLE) {
        return None;
    }
    let mut out = String::new();
    for line in style.lines() {
        out.push_str(line);
        out.push('\n');
        if line.trim() == PACKAGED_STYLE {
            out.push_str(COLOURS_IMPORT);
            out.push('\n');
        }
    }
    Some(out)
}

/// `foot.ini` with the palettes included first in `[main]`, so what the
/// account sets below wins.
fn include_palettes(ini: &str) -> Option<String> {
    if ini.lines().any(|l| l.trim() == FOOT_INCLUDE) || !ini.lines().any(|l| l.trim() == "[main]") {
        return None;
    }
    let mut out = String::new();
    let mut done = false;
    for line in ini.lines() {
        out.push_str(line);
        out.push('\n');
        if !done && line.trim() == "[main]" {
            out.push_str(FOOT_INCLUDE);
            out.push('\n');
            done = true;
        }
    }
    Some(out)
}

/// Hyprland's configuration without the border colours every account had,
/// which came after Alpymist's and so won. Colours of its own are its own.
fn drop_borders(conf: &str) -> Option<String> {
    let kept: Vec<&str> = conf
        .lines()
        .filter(|l| !HYPRLAND_BORDERS.contains(&l.trim()))
        .collect();
    (kept.len() != conf.lines().count()).then(|| {
        let mut out = kept.join("\n");
        out.push('\n');
        out
    })
}

/// Tell the running session: GTK and the portal, the bar, the terminals and
/// Hyprland. Whatever is not running has nothing to be told.
pub fn live(env: &Env, theme: &ThemeFile) {
    let scheme = match theme.scheme {
        Scheme::Dark => "prefer-dark",
        Scheme::Light => "prefer-light",
    };
    let interface = "org.gnome.desktop.interface";
    let _ = env.run(&["gsettings", "set", interface, "color-scheme", scheme]);
    let _ = env.run(&[
        "gsettings",
        "set",
        interface,
        "accent-color",
        gnome_accent(theme.accent),
    ]);
    let _ = env.run(&["pkill", "-USR2", "-x", "waybar"]);
    let signal = match theme.scheme {
        Scheme::Dark => "-USR1",
        Scheme::Light => "-USR2",
    };
    let _ = env.run(&["pkill", signal, "-x", "foot"]);
    if env.hyprland {
        let p = theme.palette();
        let active = format!("rgb({})", hex(p.accent));
        let inactive = format!("rgb({})", hex(p.border));
        let _ = env.run(&["hyprctl", "keyword", "general:col.active_border", &active]);
        let _ = env.run(&[
            "hyprctl",
            "keyword",
            "general:col.inactive_border",
            &inactive,
        ]);
    }
}

/// Before the desktop starts: the files, and GTK told, since D-Bus is up and
/// the theme may have changed where no session could hear it. A file edited
/// by hand is the account's to keep, and no reason to hold up a login.
pub fn prepare_session(env: &Env, theme: &ThemeFile) {
    let _ = write(env, theme);
    let interface = "org.gnome.desktop.interface";
    let scheme = match theme.scheme {
        Scheme::Dark => "prefer-dark",
        Scheme::Light => "prefer-light",
    };
    let _ = env.run(&["gsettings", "set", interface, "color-scheme", scheme]);
    let _ = env.run(&[
        "gsettings",
        "set",
        interface,
        "accent-color",
        gnome_accent(theme.accent),
    ]);
}

#[cfg(test)]
mod tests {
    use super::{
        COLOURS_IMPORT, FOOT_INCLUDE, colours_css, drop_borders, foot_theme, hyprland,
        import_colours, include_palettes,
    };
    use super::{FOOT_INI, FOOT_THEME, GTK, HYPRLAND, KEPT, WAYBAR_COLOURS, WAYBAR_STYLE};
    use crate::env::Env;
    use crate::{Settings, Value};
    use alpymist_theme::{Scheme, ThemeFile};
    use std::sync::Mutex;

    const SKEL_STYLE: &str =
        include_str!("../../../../desktop/skel/wayland/.config/waybar/style.css");
    const SKEL_FOOT: &str = include_str!("../../../../desktop/skel/common/.config/foot/foot.ini");
    const SKEL_HYPR: &str =
        include_str!("../../../../desktop/skel/full/.config/hypr/hyprland.conf");
    const PACKAGED: &str = include_str!("../../../../desktop/waybar/style.css");

    /// With the default theme, the bar looks as it did: every colour the
    /// packaged style names, it names as the generated file defines it.
    #[test]
    fn the_default_colours_are_the_packaged_styles_own() {
        let generated = colours_css(&ThemeFile::default());
        for line in generated.lines() {
            assert!(PACKAGED.contains(line), "the packaged style lacks {line}");
        }
        let defined = PACKAGED.matches("@define-color").count();
        assert_eq!(defined, generated.lines().count());
    }

    /// Dark is exactly what every account's foot.ini had, and still has
    /// where it was made before this: its own section wins over the included
    /// one, and says the same.
    #[test]
    fn the_dark_palette_is_the_one_accounts_had() {
        const HAD: &str = "[colors-dark]\nbackground=0b121e\nforeground=eaf0f6\n\
            regular0=070c14\nregular1=d98f7f\nregular2=9fcf9f\nregular3=d9a07f\n\
            regular4=7fb8d9\nregular5=b79fd9\nregular6=afc2d6\nregular7=9aabbd\n\
            bright0=3a4c63\nbright1=e6a89a\nbright2=b8e0b8\nbright3=e6bb9a\n\
            bright4=a3cde6\nbright5=cbb6e6\nbright6=d0dde9\nbright7=eaf0f6\n";
        assert!(foot_theme(&ThemeFile::default()).contains(HAD));
        let light = ThemeFile {
            scheme: Scheme::Light,
            ..ThemeFile::default()
        };
        assert!(foot_theme(&light).starts_with("[main]\ninitial-color-theme=light\n"));
        assert!(foot_theme(&light).contains("[colors-light]\nbackground=f4f7fa\n"));
        assert_eq!(
            hyprland(&ThemeFile::default()),
            "general {\n    col.active_border = rgb(7fb8d9)\n    col.inactive_border = rgb(3a4c63)\n}\n"
        );
    }

    /// An older account's first change: the files, its own three given their
    /// lines and kept as they were, a note, and the session told. The second
    /// finds nothing left to change.
    #[test]
    fn an_older_accounts_first_change_takes_its_files_over_once() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-themed-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, false, &RAN);
        let own = |file: &str, text: &str| {
            let path = env.account(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        own(
            WAYBAR_STYLE,
            "@import url(\"file:///usr/share/alpymist/waybar/style.css\");\n",
        );
        own(FOOT_INI, "[main]\nfont=x\n");
        own(
            HYPRLAND,
            "general {\n    col.active_border = rgb(7fb8d9)\n    col.inactive_border = rgb(3a4c63)\n}\n",
        );
        own(GTK[0], "[Settings]\ngtk-theme-name=Mine\n");

        let settings = Settings::new();
        let changed = settings
            .set(&env, "appearance.scheme", "light", false)
            .unwrap();
        let notes = changed.notes.join("\n");
        assert!(notes.contains("Now following the theme"), "{notes}");
        assert!(
            notes.contains("gtk-3.0"),
            "a settings.ini of its own is said: {notes}"
        );
        for file in [WAYBAR_STYLE, FOOT_INI, HYPRLAND] {
            let mut kept = env.account(file).into_os_string();
            kept.push(KEPT);
            assert!(std::path::Path::new(&kept).exists(), "{file} not kept");
        }
        let css = std::fs::read_to_string(env.account(WAYBAR_COLOURS)).unwrap();
        assert!(
            css.contains("@define-color alpymist-page #f4f7fa;"),
            "{css}"
        );
        assert!(
            std::fs::read_to_string(env.account(FOOT_THEME))
                .unwrap()
                .contains("initial-color-theme=light")
        );
        assert!(
            std::fs::read_to_string(env.account(GTK[1]))
                .unwrap()
                .contains("gtk-application-prefer-dark-theme=false")
        );
        assert_eq!(
            std::fs::read_to_string(env.account(GTK[0])).unwrap(),
            "[Settings]\ngtk-theme-name=Mine\n"
        );

        settings
            .live(&env, "appearance.scheme", &Value::Text("light".into()))
            .unwrap();
        let ran = RAN.lock().unwrap().clone();
        assert!(
            ran.contains(
                &"gsettings set org.gnome.desktop.interface color-scheme prefer-light".to_owned()
            ),
            "{ran:?}"
        );
        assert!(ran.contains(&"pkill -USR2 -x foot".to_owned()), "{ran:?}");

        let again = settings
            .set(&env, "appearance.accent", "moss", false)
            .unwrap();
        assert!(
            !again.notes.join("\n").contains("Now following"),
            "{:?}",
            again.notes
        );
        std::fs::remove_dir_all(&d).ok();
    }

    /// New accounts start with the lines, so there is nothing to take over.
    #[test]
    fn a_new_account_has_the_lines_already() {
        assert!(SKEL_STYLE.contains(COLOURS_IMPORT));
        assert!(SKEL_FOOT.contains(FOOT_INCLUDE));
        assert_eq!(import_colours(SKEL_STYLE), None);
        assert_eq!(include_palettes(SKEL_FOOT), None);
        assert_eq!(drop_borders(SKEL_HYPR), None);
    }

    #[test]
    fn an_older_account_is_given_the_lines_and_nothing_else() {
        let style = "/* mine */\n@import url(\"file:///usr/share/alpymist/waybar/style.css\");\n#clock { color: red; }\n";
        assert_eq!(
            import_colours(style).unwrap(),
            format!(
                "/* mine */\n@import url(\"file:///usr/share/alpymist/waybar/style.css\");\n{COLOURS_IMPORT}\n#clock {{ color: red; }}\n"
            )
        );
        assert_eq!(import_colours("#clock { color: red; }\n"), None);

        let ini = "# mine\n[main]\nfont=x\n[colors-dark]\nbackground=000000\n";
        assert_eq!(
            include_palettes(ini).unwrap(),
            format!("# mine\n[main]\n{FOOT_INCLUDE}\nfont=x\n[colors-dark]\nbackground=000000\n")
        );

        let conf = "general {\n    gaps_in = 4\n    col.active_border = rgb(7fb8d9)\n    col.inactive_border = rgb(3a4c63)\n}\n";
        assert_eq!(
            drop_borders(conf).unwrap(),
            "general {\n    gaps_in = 4\n}\n"
        );
        assert_eq!(
            drop_borders("general {\n    col.active_border = rgb(ff0000)\n}\n"),
            None
        );
    }
}

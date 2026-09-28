//! Installed applications, read from their desktop entries.
//!
//! This is the freedesktop.org Desktop Entry spec, the part of it a launcher
//! needs: find `applications/` under every XDG data directory, let the first
//! file with a given id hide the rest (so `~/.local/share/applications` can
//! override or hide a system entry), and keep what is meant to be launched —
//! `Type=Application`, not `NoDisplay`, not `Hidden`, shown in this desktop,
//! and with its `TryExec` actually installed. The reading itself is
//! [`alpymist_core::desktop_entry`], which Settings shares.
//!
//! Reading is one pass over a few dozen small files, well under the time of a
//! frame; there is no cache to go stale.

use crate::exec::Launch;
use alpymist_core::desktop_entry::{Entry, applications, list};
pub use alpymist_core::desktop_entry::{Environment, data_dirs, exec_argv};
use std::path::PathBuf;

/// One application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The desktop file id, `org.gnome.Nautilus.desktop` and the like. Stable
    /// across renames and translations, so it is what history is keyed by.
    pub id: String,
    /// The name, in the user's language where the entry has one.
    pub name: String,
    /// `GenericName`, else `Comment`: what kind of thing it is.
    pub detail: Option<String>,
    /// `Keywords`, plus the generic name and the comment, for search.
    pub keywords: Vec<String>,
    /// A Nerd Font glyph chosen from the entry's categories.
    pub icon: &'static str,
    /// What to run.
    pub launch: Launch,
}

/// Everything that can be launched, sorted by name.
#[must_use]
pub fn discover() -> Vec<App> {
    let env = Environment::current();
    discover_in(&data_dirs(), &env)
}

/// As [`discover`], in given directories.
#[must_use]
pub fn discover_in(dirs: &[PathBuf], env: &Environment) -> Vec<App> {
    let mut apps: Vec<App> = applications(dirs)
        .into_iter()
        .filter_map(|found| from_entry(&found.id, &found.entry, env))
        .collect();
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}

/// Parse one desktop entry. `None` for anything that should not be listed.
#[must_use]
pub fn parse(id: &str, text: &str, env: &Environment) -> Option<App> {
    from_entry(id, &Entry::parse(text), env)
}

fn from_entry(id: &str, entry: &Entry, env: &Environment) -> Option<App> {
    let localised = |key: &str| entry.localised(key, &env.locales);
    if entry.raw("Type") != Some("Application")
        || entry.yes("NoDisplay")
        || entry.yes("Hidden")
        || !entry.applies(env)
    {
        return None;
    }

    let name = localised("Name").filter(|n| !n.is_empty())?;
    let argv = entry.argv()?;
    let generic = localised("GenericName").filter(|g| !g.is_empty() && *g != name);
    let comment = localised("Comment").filter(|c| !c.is_empty());
    let mut keywords: Vec<String> = localised("Keywords")
        .map(|k| list(&k).map(str::to_owned).collect())
        .unwrap_or_default();
    keywords.extend(generic.iter().cloned());
    keywords.extend(comment.iter().cloned());

    Some(App {
        id: id.to_owned(),
        name,
        detail: generic.or(comment),
        keywords,
        icon: icon_for(entry.raw("Categories").unwrap_or("")),
        launch: Launch::Argv {
            argv,
            terminal: entry.yes("Terminal"),
        },
    })
}

/// A glyph for an application, from its main category.
fn icon_for(categories: &str) -> &'static str {
    const BY_CATEGORY: &[(&str, &str)] = &[
        ("TerminalEmulator", "\u{e795}"),
        ("WebBrowser", "󰖟"),
        ("Email", "󰇮"),
        ("Chat", "󰭹"),
        ("InstantMessaging", "󰭹"),
        ("FileManager", "󰉋"),
        ("TextEditor", "󰷈"),
        ("IDE", "󰨞"),
        ("Development", "󰨞"),
        ("Audio", "󰎆"),
        ("Music", "󰎆"),
        ("Video", "󰕧"),
        ("AudioVideo", "󰕧"),
        ("Graphics", "󰏘"),
        ("Photography", "󰄀"),
        ("Office", "󰈙"),
        ("Game", "󰊴"),
        ("Education", "󰑴"),
        ("Science", "󰙴"),
        ("Monitor", "󰍛"),
        ("Settings", "\u{f013}"),
        ("System", "\u{f085}"),
        ("Network", "󰖩"),
        ("Utility", "󰦬"),
    ];
    let cats: Vec<&str> = list(categories).collect();
    BY_CATEGORY
        .iter()
        .find(|(c, _)| cats.contains(c))
        .map_or("󰣆", |(_, glyph)| glyph)
}

#[cfg(test)]
mod tests {
    use super::{Environment, discover_in, parse};
    use crate::exec::Launch;
    use alpymist_core::desktop_entry::locale_keys;

    fn env() -> Environment {
        Environment {
            desktops: vec!["Hyprland".into()],
            locales: locale_keys("nb_NO.UTF-8"),
            path: vec!["/bin".into(), "/usr/bin".into()],
        }
    }

    fn argv(app: &super::App) -> &[String] {
        match &app.launch {
            Launch::Argv { argv, .. } => argv,
            Launch::Shell { .. } => panic!("desktop entries launch argv"),
        }
    }

    const FOOT: &str = "[Desktop Entry]\n\
        Type=Application\n\
        Name=Foot\n\
        Name[nb]=Fot\n\
        GenericName=Terminal\n\
        Comment=A wayland native terminal emulator\n\
        Keywords=shell;prompt;command;commandline;\n\
        Exec=foot\n\
        Categories=System;TerminalEmulator;\n\
        \n\
        [Desktop Action new-window]\n\
        Name=Should not replace the name\n\
        Exec=foot --other\n";

    #[test]
    fn a_plain_entry_parses() {
        let app = parse("foot.desktop", FOOT, &env()).unwrap();
        assert_eq!(app.name, "Fot");
        assert_eq!(app.detail.as_deref(), Some("Terminal"));
        assert_eq!(argv(&app), ["foot"]);
        assert!(app.keywords.iter().any(|k| k == "commandline"));
        assert_eq!(app.icon, "\u{e795}");
    }

    #[test]
    fn without_a_matching_locale_the_plain_name_is_used() {
        let mut e = env();
        e.locales = locale_keys("C.UTF-8");
        assert_eq!(parse("foot.desktop", FOOT, &e).unwrap().name, "Foot");
    }

    #[test]
    fn hidden_and_non_applications_are_left_out() {
        for extra in [
            "NoDisplay=true",
            "Hidden=true",
            "OnlyShowIn=GNOME;KDE;",
            "NotShowIn=Hyprland;",
        ] {
            let text = format!("[Desktop Entry]\nType=Application\nName=X\nExec=x\n{extra}\n");
            assert!(parse("x.desktop", &text, &env()).is_none(), "{extra}");
        }
        let link = "[Desktop Entry]\nType=Link\nName=X\nURL=https://example.com\n";
        assert!(parse("x.desktop", link, &env()).is_none());
    }

    #[test]
    fn only_show_in_this_desktop_is_kept() {
        let text = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nOnlyShowIn=Hyprland;\n";
        assert!(parse("x.desktop", text, &env()).is_some());
    }

    #[test]
    fn try_exec_must_exist() {
        let missing =
            "[Desktop Entry]\nType=Application\nName=X\nExec=x\nTryExec=/no/such/program\n";
        assert!(parse("x.desktop", missing, &env()).is_none());
        let present = "[Desktop Entry]\nType=Application\nName=X\nExec=sh\nTryExec=sh\n";
        assert!(parse("x.desktop", present, &env()).is_some());
    }

    #[test]
    fn the_first_directory_with_an_id_wins_even_when_it_hides_it() {
        let base = std::env::temp_dir().join(format!("alpymist-apps-{}", std::process::id()));
        let user = base.join("user/applications");
        let system = base.join("system/applications/sub");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(
            user.join("foot.desktop"),
            "[Desktop Entry]\nType=Application\nName=Foot\nExec=foot\nHidden=true\n",
        )
        .unwrap();
        std::fs::write(base.join("system/applications/foot.desktop"), FOOT).unwrap();
        std::fs::write(
            system.join("tool.desktop"),
            "[Desktop Entry]\nType=Application\nName=Tool\nExec=tool\n",
        )
        .unwrap();

        let apps = discover_in(&[base.join("user"), base.join("system")], &env());
        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["sub-tool.desktop"]);
        std::fs::remove_dir_all(base).ok();
    }
}

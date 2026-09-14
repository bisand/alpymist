//! Installed applications, read from their desktop entries.
//!
//! This is the freedesktop.org Desktop Entry spec, the part of it a launcher
//! needs: find `applications/` under every XDG data directory, let the first
//! file with a given id hide the rest (so `~/.local/share/applications` can
//! override or hide a system entry), and keep what is meant to be launched —
//! `Type=Application`, not `NoDisplay`, not `Hidden`, shown in this desktop,
//! and with its `TryExec` actually installed.
//!
//! Flatpak's export directories are searched whether or not `XDG_DATA_DIRS`
//! mentions them: that variable is set by a profile script a Hyprland session
//! started from greetd may never have sourced, and an application installed
//! from Flathub that does not appear in the menu looks like a failed install.
//!
//! Reading is one pass over a few dozen small files, well under the time of a
//! frame; there is no cache to go stale.

use crate::exec::Launch;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

/// The directories to look in, most important first.
#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());

    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(data_home.clone());
    if let Some(data_home) = &data_home {
        dirs.push(data_home.join("flatpak/exports/share"));
    }
    dirs.extend(
        system
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
    );
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));

    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Everything that can be launched, sorted by name.
#[must_use]
pub fn discover() -> Vec<App> {
    let env = Environment::current();
    discover_in(&data_dirs(), &env)
}

/// What decides whether an entry applies here.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// `XDG_CURRENT_DESKTOP`, split on `:`.
    pub desktops: Vec<String>,
    /// Locale keys to try, most specific first: `nb_NO`, `nb`.
    pub locales: Vec<String>,
    /// `PATH`, for `TryExec`.
    pub path: Vec<PathBuf>,
}

impl Environment {
    /// The running session's.
    #[must_use]
    pub fn current() -> Self {
        let desktops = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default();
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Self {
            desktops,
            locales: locale_keys(&locale),
            path,
        }
    }
}

/// `nb_NO.UTF-8@euro` → `["nb_NO@euro", "nb_NO", "nb@euro", "nb"]`, the
/// order the spec says to try them in.
fn locale_keys(locale: &str) -> Vec<String> {
    let (rest, modifier) = match locale.split_once('@') {
        Some((r, m)) => (r, Some(m)),
        None => (locale, None),
    };
    let rest = rest.split('.').next().unwrap_or("");
    if rest.is_empty() || rest == "C" || rest == "POSIX" {
        return Vec::new();
    }
    let (lang, country) = match rest.split_once('_') {
        Some((l, c)) => (l, Some(c)),
        None => (rest, None),
    };
    let mut keys = Vec::new();
    if let (Some(c), Some(m)) = (country, modifier) {
        keys.push(format!("{lang}_{c}@{m}"));
    }
    if let Some(c) = country {
        keys.push(format!("{lang}_{c}"));
    }
    if let Some(m) = modifier {
        keys.push(format!("{lang}@{m}"));
    }
    keys.push(lang.to_owned());
    keys
}

/// As [`discover`], in given directories.
#[must_use]
pub fn discover_in(dirs: &[PathBuf], env: &Environment) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let root = dir.join("applications");
        let mut files = Vec::new();
        collect(&root, &root, &mut files);
        // Directory order is arbitrary; sorting keeps a duplicate id within
        // one directory resolved the same way every time.
        files.sort();
        for (id, path) in files {
            // The first occurrence of an id decides, even when it is hidden:
            // that is how a user hides a system entry.
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse(&id, &text, env) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}

/// Every `.desktop` file under `dir`, with its id: the path below
/// `applications/`, slashes turned to dashes.
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Symlinks are followed: Flatpak's exports are nothing but symlinks.
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "desktop")
            && let Ok(rel) = path.strip_prefix(root)
        {
            let id = rel.to_string_lossy().replace('/', "-");
            out.push((id, path));
        }
    }
}

/// Parse one desktop entry. `None` for anything that should not be listed.
#[must_use]
pub fn parse(id: &str, text: &str, env: &Environment) -> Option<App> {
    let mut in_entry = false;
    let mut fields: Vec<(&str, &str)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Only the main group; actions and vendor groups come after it.
            if in_entry {
                break;
            }
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            fields.push((k.trim(), v.trim()));
        }
    }
    let raw = |key: &str| fields.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    let localised = |key: &str| {
        env.locales
            .iter()
            .find_map(|l| raw(&format!("{key}[{l}]")))
            .or_else(|| raw(key))
            .map(unescape)
    };
    let yes = |key: &str| raw(key) == Some("true");

    if raw("Type") != Some("Application") || yes("NoDisplay") || yes("Hidden") {
        return None;
    }
    if let Some(only) = raw("OnlyShowIn")
        && !list(only).any(|d| env.desktops.iter().any(|e| e == d))
    {
        return None;
    }
    if let Some(not) = raw("NotShowIn")
        && list(not).any(|d| env.desktops.iter().any(|e| e == d))
    {
        return None;
    }
    if let Some(try_exec) = raw("TryExec")
        && !installed(&unescape(try_exec), &env.path)
    {
        return None;
    }

    let name = localised("Name").filter(|n| !n.is_empty())?;
    let argv = exec_argv(&unescape(raw("Exec")?))?;
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
        icon: icon_for(raw("Categories").unwrap_or("")),
        launch: Launch::Argv {
            argv,
            terminal: yes("Terminal"),
        },
    })
}

/// A `;`-separated list, which may or may not end in `;`.
fn list(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').map(str::trim).filter(|s| !s.is_empty())
}

/// The escapes the spec allows in string values.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                // `\;` and `\\` survive into list and Exec parsing as escapes.
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Whether a `TryExec` program exists: an absolute path, or a name on `PATH`.
fn installed(program: &str, path: &[PathBuf]) -> bool {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.is_file();
    }
    path.iter().any(|dir| dir.join(program).is_file())
}

/// Split an `Exec` value into arguments, dropping field codes.
///
/// Quoting is the spec's, not the shell's: double quotes only, with `\"`,
/// `` \` ``, `\$` and `\\` escaped inside them. The menu opens nothing, so
/// `%f`, `%u` and their plural forms expand to nothing; `%%` is a percent sign.
/// `None` when nothing is left to run.
#[must_use]
pub fn exec_argv(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    started = true;
                }
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            // Field codes expand to nothing, and an argument that was only
            // a field code is dropped rather than passed empty.
            '%' => {
                if chars.next() == Some('%') {
                    current.push('%');
                    started = true;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    (!args.is_empty()).then_some(args)
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
    use super::{Environment, discover_in, exec_argv, locale_keys, parse};
    use crate::exec::Launch;

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
    fn locale_keys_go_from_specific_to_general() {
        assert_eq!(
            locale_keys("nb_NO.UTF-8@euro"),
            ["nb_NO@euro", "nb_NO", "nb@euro", "nb"]
        );
        assert_eq!(locale_keys("en"), ["en"]);
        assert!(locale_keys("POSIX").is_empty());
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
    fn field_codes_are_dropped() {
        assert_eq!(exec_argv("librewolf %u").unwrap(), ["librewolf"]);
        assert_eq!(
            exec_argv("app --files %F --x").unwrap(),
            ["app", "--files", "--x"]
        );
        assert_eq!(exec_argv("printf 100%%").unwrap(), ["printf", "100%"]);
    }

    #[test]
    fn quoted_arguments_stay_whole() {
        assert_eq!(
            exec_argv(r#"sh -c "echo \"hi there\" \$HOME""#).unwrap(),
            ["sh", "-c", r#"echo "hi there" $HOME"#]
        );
        assert_eq!(exec_argv(r#"prog """#).unwrap(), ["prog", ""]);
    }

    #[test]
    fn flatpak_exec_lines_parse() {
        let exec = "/usr/bin/flatpak run --branch=stable --arch=aarch64 --command=gimp-3 --file-forwarding org.gimp.GIMP @@ %F @@";
        let args = exec_argv(exec).unwrap();
        assert_eq!(args[0], "/usr/bin/flatpak");
        assert_eq!(args.last().unwrap(), "@@");
    }

    #[test]
    fn an_empty_exec_is_not_an_application() {
        assert!(exec_argv("  %U ").is_none());
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

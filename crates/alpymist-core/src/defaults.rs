//! Default applications: which one opens a kind of thing, and which terminal
//! a command runs in.
//!
//! The first is the freedesktop.org MIME applications spec: `mimeapps.list`
//! files, the account's before the system's, each saying under `[Default
//! Applications]` which desktop entry opens a type; failing all of them, any
//! installed application that says it opens that type. GIO, the portals and
//! Settings all read it this way, so they agree.
//!
//! The second has no spec of the same standing. `xdg-terminals.list`, one
//! desktop entry id a line and the first installed one winning, is what
//! `xdg-terminal-exec` reads, and so what this reads too; with nothing chosen
//! it is foot, the terminal Alpymist ships.

use crate::desktop_entry::{self, Entry, Found, list, unescape};
use std::path::{Path, PathBuf};

/// The terminal when nothing else is chosen: the one Alpymist ships.
pub const FALLBACK_TERMINAL: &str = "foot";

/// Where the lists and the applications are.
#[derive(Debug, Clone)]
pub struct Places {
    /// `~/.config`.
    pub config_home: PathBuf,
    /// `/etc/xdg`, and whatever else `XDG_CONFIG_DIRS` names.
    pub config_dirs: Vec<PathBuf>,
    /// Where applications are, most important first: see
    /// [`desktop_entry::data_dirs`].
    pub data_dirs: Vec<PathBuf>,
    /// The desktops a list may be named for: `Hyprland` reads
    /// `hyprland-mimeapps.list` before `mimeapps.list`.
    pub desktops: Vec<String>,
}

impl Places {
    /// This session's. The desktop is Hyprland whatever this was started
    /// from — a terminal over SSH has no `XDG_CURRENT_DESKTOP` — since
    /// Alpymist is one desktop.
    #[must_use]
    pub fn current() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config")))
            .unwrap_or_else(|| PathBuf::from("/nonexistent/.config"));
        let config_dirs = std::env::var("XDG_CONFIG_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/etc/xdg".into())
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        Self {
            config_home,
            config_dirs,
            data_dirs: desktop_entry::data_dirs(),
            desktops: vec!["Hyprland".into()],
        }
    }

    /// Every `mimeapps.list` to read, in the order they count.
    #[must_use]
    pub fn mimeapps(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = vec![self.config_home.clone()];
        dirs.extend(self.config_dirs.iter().cloned());
        dirs.extend(self.data_dirs.iter().map(|d| d.join("applications")));
        self.named(&dirs, "mimeapps.list")
    }

    /// Every `xdg-terminals.list` to read, in the order they count.
    #[must_use]
    pub fn terminal_lists(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = vec![self.config_home.clone()];
        dirs.extend(self.config_dirs.iter().cloned());
        self.named(&dirs, "xdg-terminals.list")
    }

    /// `name` in each of `dirs`, each desktop's own before the plain one.
    fn named(&self, dirs: &[PathBuf], name: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in dirs {
            for desktop in &self.desktops {
                files.push(dir.join(format!("{}-{name}", desktop.to_lowercase())));
            }
            files.push(dir.join(name));
        }
        files
    }

    /// Every application installed, hidden ones and all.
    #[must_use]
    pub fn applications(&self) -> Vec<Found> {
        desktop_entry::applications(&self.data_dirs)
    }
}

/// Whether an entry can be started: an application, not hidden, with
/// something to run.
#[must_use]
pub fn usable(entry: &Entry) -> bool {
    entry.raw("Type") == Some("Application") && !entry.yes("Hidden") && entry.argv().is_some()
}

/// Whether an entry says it opens `mime`.
#[must_use]
pub fn handles(entry: &Entry, mime: &str) -> bool {
    entry
        .raw("MimeType")
        .is_some_and(|m| list(m).any(|t| t.eq_ignore_ascii_case(mime)))
}

/// Whether an entry is a terminal.
#[must_use]
pub fn is_terminal(entry: &Entry) -> bool {
    entry
        .raw("Categories")
        .is_some_and(|c| list(c).any(|c| c == "TerminalEmulator"))
}

/// The desktop entry ids `files` name as the default for `mime`, in order.
#[must_use]
pub fn listed(files: &[PathBuf], mime: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let mut in_defaults = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_defaults = line == "[Default Applications]";
                continue;
            }
            if !in_defaults || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=')
                && k.trim().eq_ignore_ascii_case(mime)
            {
                ids.extend(list(v).map(str::to_owned));
            }
        }
    }
    ids
}

/// The application that opens `mime`: the first one a list names that is
/// installed, else the first installed that says it opens it.
#[must_use]
pub fn default_for<'a>(places: &Places, apps: &'a [Found], mime: &str) -> Option<&'a Found> {
    listed(&places.mimeapps(), mime)
        .iter()
        .find_map(|id| apps.iter().find(|f| &f.id == id && usable(&f.entry)))
        .or_else(|| {
            apps.iter()
                .find(|f| usable(&f.entry) && handles(&f.entry, mime))
        })
}

/// The ids `xdg-terminals.list` files name, in order. A line may name an
/// action after a colon, which is not needed here.
#[must_use]
pub fn listed_terminals(files: &[PathBuf]) -> Vec<String> {
    let mut ids = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim().trim_start_matches('+');
            if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
                continue;
            }
            ids.push(line.split(':').next().unwrap_or(line).to_owned());
        }
    }
    ids
}

/// The terminal: the first one a list names that is installed, else foot,
/// else any terminal at all.
#[must_use]
pub fn terminal<'a>(places: &Places, apps: &'a [Found]) -> Option<&'a Found> {
    let ok = |f: &&Found| usable(&f.entry) && is_terminal(&f.entry);
    listed_terminals(&places.terminal_lists())
        .iter()
        .find_map(|id| apps.iter().filter(ok).find(|f| &f.id == id))
        .or_else(|| {
            apps.iter()
                .filter(ok)
                .find(|f| f.id == format!("{FALLBACK_TERMINAL}.desktop"))
        })
        .or_else(|| apps.iter().find(ok))
}

/// What goes before a command to run it in the terminal whose entry is
/// `entry`: its `Exec`, then the flag it takes before a command. The flag is
/// its `X-TerminalArgExec`, or `-e` as nearly every terminal takes; foot takes
/// the command with no flag at all.
#[must_use]
pub fn prefix(entry: &Entry) -> Option<Vec<String>> {
    let mut argv = entry.argv()?;
    let flag = entry.raw("X-TerminalArgExec").map_or_else(
        || {
            let program = Path::new(&argv[0]).file_name().and_then(|n| n.to_str());
            if matches!(program, Some("foot" | "footclient")) {
                String::new()
            } else {
                "-e".to_owned()
            }
        },
        unescape,
    );
    if !flag.is_empty() {
        argv.push(flag);
    }
    Some(argv)
}

/// How to run `command` in the terminal whose entry is `entry`, or to open
/// it with no command.
#[must_use]
pub fn in_terminal(entry: &Entry, command: &[String]) -> Option<Vec<String>> {
    if command.is_empty() {
        return entry.argv();
    }
    let mut argv = prefix(entry)?;
    argv.extend(command.iter().cloned());
    Some(argv)
}

/// What goes before a command to run it in the terminal chosen here: foot's
/// when nothing installed says it is a terminal.
#[must_use]
pub fn terminal_prefix(places: &Places) -> Vec<String> {
    let apps = places.applications();
    terminal(places, &apps)
        .and_then(|t| prefix(&t.entry))
        .unwrap_or_else(|| vec![FALLBACK_TERMINAL.to_owned()])
}

/// The whole command line to run `command` in the terminal chosen here, or
/// to open one with no command. foot when nothing installed says it is a
/// terminal.
#[must_use]
pub fn terminal_argv(places: &Places, command: &[String]) -> Vec<String> {
    let apps = places.applications();
    terminal(places, &apps)
        .and_then(|t| in_terminal(&t.entry, command))
        .unwrap_or_else(|| {
            let mut argv = vec![FALLBACK_TERMINAL.to_owned()];
            argv.extend(command.iter().cloned());
            argv
        })
}

#[cfg(test)]
mod tests {
    use super::{Places, default_for, in_terminal, listed_terminals, terminal};
    use crate::desktop_entry::Entry;
    use std::path::{Path, PathBuf};

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn places(name: &str) -> (PathBuf, Places) {
        let d =
            std::env::temp_dir().join(format!("alpymist-defaults-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let apps = d.join("share/applications");
        put(
            &apps.join("librewolf.desktop"),
            "[Desktop Entry]\nType=Application\nName=LibreWolf\nExec=librewolf %u\n\
             MimeType=text/html;x-scheme-handler/https;application/pdf;\n",
        );
        put(
            &apps.join("org.example.Browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser %U\n\
             MimeType=text/html;x-scheme-handler/https;\n",
        );
        put(
            &apps.join("foot.desktop"),
            "[Desktop Entry]\nType=Application\nName=Foot\nExec=foot\n\
             Categories=System;TerminalEmulator;\n",
        );
        put(
            &apps.join("kitty.desktop"),
            "[Desktop Entry]\nType=Application\nName=kitty\nExec=kitty\n\
             Categories=System;TerminalEmulator;\n",
        );
        let places = Places {
            config_home: d.join("config"),
            config_dirs: vec![d.join("etc/xdg")],
            data_dirs: vec![d.join("share")],
            desktops: vec!["Hyprland".into()],
        };
        (d, places)
    }

    #[test]
    fn the_account_list_comes_before_the_systems() {
        let (d, places) = places("mime");
        let apps = places.applications();
        let pick = |mime| default_for(&places, &apps, mime).map(|f| f.id.clone());
        // Nothing listed: whatever says it opens the type.
        assert!(pick("text/html").is_some());
        assert_eq!(pick("image/png"), None);

        put(
            &d.join("etc/xdg/mimeapps.list"),
            "[Default Applications]\ntext/html=librewolf.desktop\n",
        );
        assert_eq!(pick("text/html").as_deref(), Some("librewolf.desktop"));
        put(
            &d.join("config/mimeapps.list"),
            "[Added Associations]\ntext/html=nothing.desktop\n\
             [Default Applications]\ntext/html=missing.desktop;org.example.Browser.desktop;\n",
        );
        assert_eq!(
            pick("text/html").as_deref(),
            Some("org.example.Browser.desktop"),
            "the first installed one it names"
        );
        put(
            &d.join("config/hyprland-mimeapps.list"),
            "[Default Applications]\ntext/html=librewolf.desktop\n",
        );
        assert_eq!(pick("text/html").as_deref(), Some("librewolf.desktop"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn the_terminal_is_the_first_listed_else_foot() {
        let (d, places) = places("terminal");
        let apps = places.applications();
        let pick = || terminal(&places, &apps).map(|f| f.id.clone());
        assert_eq!(pick().as_deref(), Some("foot.desktop"));
        put(
            &d.join("config/xdg-terminals.list"),
            "# mine\nmissing.desktop\nkitty.desktop:new-window\n",
        );
        assert_eq!(
            listed_terminals(&places.terminal_lists()),
            ["missing.desktop", "kitty.desktop"]
        );
        assert_eq!(pick().as_deref(), Some("kitty.desktop"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_command_follows_the_terminals_own_flag() {
        let command = ["passwd".to_owned()];
        let entry = |text: &str| Entry::parse(&format!("[Desktop Entry]\n{text}"));
        assert_eq!(
            in_terminal(&entry("Exec=foot\n"), &command).unwrap(),
            ["foot", "passwd"]
        );
        assert_eq!(
            in_terminal(&entry("Exec=kitty\n"), &command).unwrap(),
            ["kitty", "-e", "passwd"]
        );
        assert_eq!(
            in_terminal(
                &entry("Exec=gnome-terminal\nX-TerminalArgExec=--\n"),
                &command
            )
            .unwrap(),
            ["gnome-terminal", "--", "passwd"]
        );
        assert_eq!(in_terminal(&entry("Exec=kitty\n"), &[]).unwrap(), ["kitty"]);
    }
}

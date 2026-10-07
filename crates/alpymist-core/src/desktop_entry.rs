//! Desktop entries: the freedesktop.org files that say what an application is
//! called and how to start it.
//!
//! The part of the Desktop Entry spec that more than one program here needs.
//! The menu lists the entries under every `applications/` directory; Settings
//! offers them for starting at login, and `alpymist autostart` starts the ones
//! under `autostart/`. Each reads the same way: the main group only, a
//! translated value where the entry has one, `OnlyShowIn`, `NotShowIn` and
//! `TryExec` deciding whether it applies here at all.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The main group of one desktop entry, `[Desktop Entry]`, as keys and values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    fields: Vec<(String, String)>,
}

impl Entry {
    /// Read the main group out of a file's text. Actions and vendor groups,
    /// which come after it, are not read.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut in_entry = false;
        let mut fields = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
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
                fields.push((k.trim().to_owned(), v.trim().to_owned()));
            }
        }
        Self { fields }
    }

    /// A value as written, escapes and all.
    #[must_use]
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// A string value in the first of `locales` the entry has, else the plain
    /// one, with its escapes undone.
    #[must_use]
    pub fn localised(&self, key: &str, locales: &[String]) -> Option<String> {
        locales
            .iter()
            .find_map(|l| self.raw(&format!("{key}[{l}]")))
            .or_else(|| self.raw(key))
            .map(unescape)
    }

    /// Whether a boolean key is `true`.
    #[must_use]
    pub fn yes(&self, key: &str) -> bool {
        self.raw(key) == Some("true")
    }

    /// Whether it is to be run in a terminal: `Terminal`, unless the entry is
    /// also `DBusActivatable`. A desktop that starts such a program over the
    /// bus gives it no terminal whatever `Terminal` says, so a graphical
    /// program can say both and never be seen with one; some do. We run its
    /// `Exec`, and would be the only desktop that opened a terminal beside
    /// its window.
    #[must_use]
    pub fn wants_terminal(&self) -> bool {
        self.yes("Terminal") && !self.yes("DBusActivatable")
    }

    /// Whether it applies to this desktop and its program is installed:
    /// `OnlyShowIn`, `NotShowIn` and `TryExec`.
    #[must_use]
    pub fn applies(&self, env: &Environment) -> bool {
        if let Some(only) = self.raw("OnlyShowIn")
            && !list(only).any(|d| env.desktops.iter().any(|e| e == d))
        {
            return false;
        }
        if let Some(not) = self.raw("NotShowIn")
            && list(not).any(|d| env.desktops.iter().any(|e| e == d))
        {
            return false;
        }
        if let Some(try_exec) = self.raw("TryExec")
            && !installed(&unescape(try_exec), &env.path)
        {
            return false;
        }
        true
    }

    /// What to run: `Exec`, split into arguments, without its field codes.
    #[must_use]
    pub fn argv(&self) -> Option<Vec<String>> {
        exec_argv(&unescape(self.raw("Exec")?))
    }
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
#[must_use]
pub fn locale_keys(locale: &str) -> Vec<String> {
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

/// The data directories applications are found under, most important first.
///
/// Flatpak's export directories are searched whether or not `XDG_DATA_DIRS`
/// mentions them: that variable is set by a profile script a Hyprland session
/// started from greetd may never have sourced, and an application installed
/// from Flathub that cannot be found looks like a failed install.
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

/// One application's entry, and where it was found.
#[derive(Debug, Clone)]
pub struct Found {
    /// The desktop file id: `org.gnome.Nautilus.desktop`, or `sub-tool.desktop`
    /// for `applications/sub/tool.desktop`.
    pub id: String,
    /// The file.
    pub path: PathBuf,
    /// What it says.
    pub entry: Entry,
}

/// The entry for every id under the `applications/` directory of each of
/// `dirs`, the first directory's winning.
///
/// The first file with an id decides even when it hides the application:
/// that is how `~/.local/share/applications` hides a system entry. So an entry
/// here may still be `Hidden` or `NoDisplay`, which the caller decides about.
#[must_use]
pub fn applications(dirs: &[PathBuf]) -> Vec<Found> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for dir in dirs {
        let root = dir.join("applications");
        let mut files = Vec::new();
        collect(&root, &root, &mut files);
        // Directory order is arbitrary; sorting keeps a duplicate id within
        // one directory resolved the same way every time.
        files.sort();
        for (id, path) in files {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            found.push(Found {
                id,
                path,
                entry: Entry::parse(&text),
            });
        }
    }
    found
}

/// Every `.desktop` file under `dir`, with its id: the path below `root`,
/// slashes turned to dashes.
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

/// A `;`-separated list, which may or may not end in `;`.
pub fn list(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').map(str::trim).filter(|s| !s.is_empty())
}

/// The escapes the spec allows in string values.
#[must_use]
pub fn unescape(value: &str) -> String {
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

/// Whether a `TryExec` program exists: an absolute path, or a name on `path`.
#[must_use]
pub fn installed(program: &str, path: &[PathBuf]) -> bool {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.is_file();
    }
    path.iter().any(|dir| dir.join(program).is_file())
}

/// Split an `Exec` value into arguments, dropping field codes.
///
/// Quoting is the spec's, not the shell's: double quotes only, with `\"`,
/// `` \` ``, `\$` and `\\` escaped inside them. Nothing is opened, so `%f`,
/// `%u` and their plural forms expand to nothing; `%%` is a percent sign.
/// `None` when nothing is left to run.
#[must_use]
pub fn exec_argv(exec: &str) -> Option<Vec<String>> {
    exec_with(exec, &[]).into_iter().next()
}

/// What to run to open `args` — files or URLs — with an `Exec` value: one
/// command line, or one for each when it takes a single one (`%f`, `%u`).
///
/// `%F` and `%U` take all of them. An `Exec` without either kind gets them
/// on the end, which is what a program that forgot to say it takes them
/// usually wants. Empty when nothing is left to run.
#[must_use]
pub fn exec_with(exec: &str, args: &[String]) -> Vec<Vec<String>> {
    let words = words(exec);
    let has = |codes: &[char]| {
        words.iter().any(|w| match w {
            Word::Code(c) | Word::Inside(_, c, _) => codes.contains(c),
            Word::Text(_) => false,
        })
    };
    let fill = |one: Option<&String>, all: bool| -> Vec<String> {
        let mut line = Vec::new();
        for w in &words {
            match w {
                Word::Text(t) => line.push(t.clone()),
                Word::Code('F' | 'U') if all => line.extend(args.iter().cloned()),
                Word::Code('f' | 'u') => line.extend(one.cloned()),
                Word::Code(_) => {}
                Word::Inside(before, code, after) => {
                    let file = match code {
                        'f' | 'u' => one.map_or("", String::as_str),
                        _ => "",
                    };
                    line.push(format!("{before}{file}{after}"));
                }
            }
        }
        line
    };
    let runs: Vec<Vec<String>> = if args.is_empty() {
        vec![fill(None, false)]
    } else if has(&['F', 'U']) {
        vec![fill(None, true)]
    } else if has(&['f', 'u']) {
        args.iter().map(|a| fill(Some(a), false)).collect()
    } else {
        let mut line = fill(None, false);
        line.extend(args.iter().cloned());
        vec![line]
    };
    runs.into_iter().filter(|line| !line.is_empty()).collect()
}

/// One argument of an `Exec` value.
enum Word {
    /// Text, its quoting and escapes undone.
    Text(String),
    /// An argument that is a field code and nothing else: `%u`.
    Code(char),
    /// An argument with a field code inside it, `--file=%f`: what comes
    /// before it, the code, and what comes after.
    Inside(String, char, String),
}

fn words(exec: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current = String::new();
    // The code inside `current`, and where: only the first one counts.
    let mut inside: Option<(usize, char)> = None;
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    let finish = |current: &mut String, inside: &mut Option<(usize, char)>| {
        let text = std::mem::take(current);
        match inside.take() {
            Some((at, code)) => Word::Inside(text[..at].to_owned(), code, text[at..].to_owned()),
            None => Word::Text(text),
        }
    };
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
                    words.push(finish(&mut current, &mut inside));
                    started = false;
                }
            }
            '%' => match chars.next() {
                Some('%') => {
                    current.push('%');
                    started = true;
                }
                // A code standing alone is where the files go, as is the
                // first inside other text; any other expands to nothing, and
                // an argument that was only codes is dropped rather than
                // passed empty.
                Some(code)
                    if !started && !quoted && chars.peek().is_none_or(|n| n.is_whitespace()) =>
                {
                    words.push(Word::Code(code));
                }
                Some(code) if inside.is_none() && "fFuU".contains(code) => {
                    inside = Some((current.len(), code));
                    started = true;
                }
                _ => {}
            },
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(finish(&mut current, &mut inside));
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{Entry, Environment, applications, exec_argv, exec_with, locale_keys};

    fn env() -> Environment {
        Environment {
            desktops: vec!["Hyprland".into()],
            locales: locale_keys("nb_NO.UTF-8"),
            path: vec!["/bin".into(), "/usr/bin".into()],
        }
    }

    #[test]
    fn only_the_main_group_is_read() {
        let e = Entry::parse(
            "# a comment\n[Desktop Entry]\nName=Foot\nName[nb]=Fot\nExec=foot\n\
             [Desktop Action new-window]\nName=Other\nExec=foot --other\n",
        );
        assert_eq!(e.localised("Name", &env().locales).as_deref(), Some("Fot"));
        assert_eq!(e.localised("Name", &[]).as_deref(), Some("Foot"));
        assert_eq!(e.argv().unwrap(), ["foot"]);
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
    fn an_entry_applies_to_its_desktops_and_when_installed() {
        for (extra, applies) in [
            ("", true),
            ("OnlyShowIn=GNOME;KDE;", false),
            ("OnlyShowIn=Hyprland;", true),
            ("NotShowIn=Hyprland;", false),
            ("TryExec=/no/such/program", false),
            ("TryExec=sh", true),
        ] {
            let e = Entry::parse(&format!("[Desktop Entry]\nExec=sh\n{extra}\n"));
            assert_eq!(e.applies(&env()), applies, "{extra}");
        }
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
    fn files_and_urls_go_where_the_field_codes_say() {
        let two = ["a b.txt".to_owned(), "c.txt".to_owned()];
        assert_eq!(
            exec_with("librewolf %u", &two[..1]),
            [["librewolf", "a b.txt"]]
        );
        assert_eq!(
            exec_with("squint %f", &two),
            [["squint", "a b.txt"], ["squint", "c.txt"]]
        );
        assert_eq!(
            exec_with("viewer --x %F", &two),
            [["viewer", "--x", "a b.txt", "c.txt"]]
        );
        assert_eq!(exec_with("plain", &two), [["plain", "a b.txt", "c.txt"]]);
        assert_eq!(exec_with("librewolf %u", &[]), [["librewolf"]]);
        assert_eq!(
            exec_with("app --file=%f", &two[..1]),
            [["app", "--file=a b.txt"]]
        );
        assert_eq!(exec_with("app --file=%f", &[]), [["app", "--file="]]);
    }

    #[test]
    fn an_empty_exec_is_nothing_to_run() {
        assert!(exec_argv("  %U ").is_none());
    }

    #[test]
    fn the_first_directory_with_an_id_wins() {
        let base = std::env::temp_dir().join(format!("alpymist-entries-{}", std::process::id()));
        let user = base.join("user/applications");
        let system = base.join("system/applications/sub");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(
            user.join("foot.desktop"),
            "[Desktop Entry]\nName=Foot\nHidden=true\n",
        )
        .unwrap();
        std::fs::write(
            base.join("system/applications/foot.desktop"),
            "[Desktop Entry]\nName=Foot\n",
        )
        .unwrap();
        std::fs::write(system.join("tool.desktop"), "[Desktop Entry]\nName=Tool\n").unwrap();

        let found = applications(&[base.join("user"), base.join("system")]);
        let ids: Vec<(&str, bool)> = found
            .iter()
            .map(|f| (f.id.as_str(), f.entry.yes("Hidden")))
            .collect();
        assert_eq!(ids, [("foot.desktop", true), ("sub-tool.desktop", false)]);
        std::fs::remove_dir_all(base).ok();
    }
}

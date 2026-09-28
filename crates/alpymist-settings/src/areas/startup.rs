//! Startup: the programs that start when you log in.
//!
//! These are XDG autostart entries, as every other desktop reads them: the
//! ones packages install in `/etc/xdg/autostart`, and the account's own in
//! `~/.config/autostart`, where a file with the same name overrides the
//! system's. `alpymist autostart`, from the Hyprland configuration the
//! package ships, starts them at login.
//!
//! Each program listed gets a switch. Off writes the account's copy of its
//! entry with `Hidden=true`, which the spec says makes it as good as not there;
//! on takes that out again, and the copy with it when nothing else in it
//! differs. "Add a program" copies an installed application's entry into
//! `~/.config/autostart`, and "Remove a program" deletes one added that way.
//!
//! Alpymist's own parts — the bar, notifications, the screensaver, sound —
//! are not here. They are the desktop, not programs to opt out of, and start
//! from `hyprland.conf`. An entry the desktop already starts that way is
//! skipped rather than started twice.
//!
//! Nothing starts because of this that was not installed to: a package puts an
//! entry in `/etc/xdg/autostart` for that reason, and anything else is added by
//! hand.

use crate::env::Env;
use crate::io_error;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::desktop_entry::{self, Entry, Environment};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where packages put what starts at login.
pub const SYSTEM: &str = "etc/xdg/autostart";
/// Where the account's own entries are, under `~/.config`.
pub const ACCOUNT: &str = "autostart";
/// The choice that adds a program.
pub const ADD: &str = "startup.add";
/// The choice that removes a program that was added.
pub const REMOVE: &str = "startup.remove";

/// Entries the desktop starts itself, from `hyprland.conf`. The sound
/// server's launcher kills the sound server it finds running before starting its own,
/// so starting it a second time would take the sound out from under the bar.
const OWN: &[&str] = &["pipewire.desktop"];

/// One program that starts at login.
#[derive(Debug, Clone)]
pub struct Program {
    /// The entry's file name: `nm-applet.desktop`.
    pub file: String,
    /// What it is called.
    pub name: String,
    /// What it says, the account's copy where there is one.
    pub entry: Entry,
    /// The system's entry's text, when a package installed one.
    pub system: Option<String>,
    /// The account's copy, when there is one.
    pub account: Option<PathBuf>,
}

impl Program {
    /// Whether it starts: not hidden, and not turned off the GNOME way.
    #[must_use]
    pub fn enabled(&self) -> bool {
        !self.entry.yes("Hidden") && self.entry.raw("X-GNOME-Autostart-enabled") != Some("false")
    }

    /// Whether it is listed in Settings. `NoDisplay` entries — setting up the
    /// account's folders, say — still start, but are not programs a person
    /// would recognise.
    #[must_use]
    pub fn listed(&self) -> bool {
        !self.entry.yes("NoDisplay")
    }

    /// Whether it was added by hand rather than installed to start.
    #[must_use]
    pub fn added(&self) -> bool {
        self.system.is_none()
    }

    /// Its setting's id: `startup.nm-applet`.
    fn id(&self) -> String {
        format!("startup.{}", stem(&self.file))
    }

    /// What to run, in the terminal chosen in Settings › Default
    /// applications when it wants one.
    #[must_use]
    pub fn argv(&self) -> Option<Vec<String>> {
        let argv = self.entry.argv()?;
        if self.entry.yes("Terminal") {
            return Some(alpymist_core::defaults::terminal_argv(
                &alpymist_core::defaults::Places::current(),
                &argv,
            ));
        }
        Some(argv)
    }
}

/// `nm-applet` from `nm-applet.desktop`.
fn stem(file: &str) -> &str {
    file.strip_suffix(".desktop").unwrap_or(file)
}

/// What decides whether an entry applies: Alpymist is Hyprland, whatever
/// this process was started from, and the rest is the session's.
fn here() -> Environment {
    Environment {
        desktops: vec!["Hyprland".into()],
        ..Environment::current()
    }
}

/// The `.desktop` files directly in `dir`, by name, with their text.
fn read(dir: &Path) -> BTreeMap<String, (PathBuf, String)> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if Path::new(&name).extension().is_none_or(|e| e != "desktop") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.insert(name, (path, text));
        }
    }
    out
}

/// Every program that starts at login here, or would but is turned off, in
/// order of name.
#[must_use]
pub fn programs(env: &Env) -> Vec<Program> {
    programs_for(env, &here())
}

fn programs_for(env: &Env, here: &Environment) -> Vec<Program> {
    let system = read(&env.system(SYSTEM));
    let mut account = read(&env.account(ACCOUNT));
    let mut files: Vec<String> = system.keys().chain(account.keys()).cloned().collect();
    files.sort();
    files.dedup();
    let mut programs = Vec::new();
    for file in files {
        if OWN.contains(&file.as_str()) {
            continue;
        }
        let system_text = system.get(&file).map(|(_, t)| t.clone());
        let own = account.remove(&file);
        let Some(text) = own.as_ref().map(|(_, t)| t).or(system_text.as_ref()) else {
            continue;
        };
        let entry = Entry::parse(text);
        // A system entry hidden by its package is not there at all; one the
        // account hid is listed, turned off.
        if own.is_none() && entry.yes("Hidden") {
            continue;
        }
        if entry.raw("Type").is_some_and(|t| t != "Application") || !entry.applies(here) {
            continue;
        }
        let name = entry
            .localised("Name", &here.locales)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| stem(&file).to_owned());
        programs.push(Program {
            file,
            name,
            entry,
            system: system_text,
            account: own.map(|(p, _)| p),
        });
    }
    programs.sort_by_cached_key(|p| p.name.to_lowercase());
    programs
}

/// An installed application that could be added: its file id and where its
/// entry is.
struct Installable {
    file: String,
    name: String,
    path: PathBuf,
}

/// Every application the menu would list, by name.
fn installable(dirs: &[PathBuf], here: &Environment) -> Vec<Installable> {
    let mut apps: Vec<Installable> = desktop_entry::applications(dirs)
        .into_iter()
        .filter(|f| {
            f.entry.raw("Type") == Some("Application")
                && !f.entry.yes("NoDisplay")
                && !f.entry.yes("Hidden")
                && f.entry.applies(here)
                && f.entry.argv().is_some()
        })
        .filter_map(|f| {
            let name = f
                .entry
                .localised("Name", &here.locales)
                .filter(|n| !n.is_empty())?;
            Some(Installable {
                file: f.id,
                name,
                path: f.path,
            })
        })
        .collect();
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}

/// A string that lives as long as the process, for a `Setting` to carry.
///
/// Settings are made again after a program is added or removed, so this is
/// a few bytes more each time; the alternative is a registry that never learns
/// what was added.
fn kept(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The settings: a switch for each program listed, then adding and removing.
#[must_use]
pub fn settings() -> Vec<Setting> {
    settings_for(&Env::detect(), &desktop_entry::data_dirs(), &here())
}

fn settings_for(env: &Env, dirs: &[PathBuf], here: &Environment) -> Vec<Setting> {
    let programs = programs_for(env, here);
    let mut out = Vec::new();
    for p in programs.iter().filter(|p| p.listed()) {
        let id = p.id();
        // `startup.add` and `startup.remove` are taken.
        if id == ADD || id == REMOVE {
            continue;
        }
        let what = p
            .entry
            .localised("Comment", &here.locales)
            .filter(|c| !c.is_empty())
            .or_else(|| {
                p.entry
                    .localised("GenericName", &here.locales)
                    .filter(|g| !g.is_empty())
            });
        let how = if p.added() {
            "You added it."
        } else {
            "Installed to start."
        };
        out.push(Setting {
            id: kept(id),
            title: kept(p.name.clone()),
            description: kept(what.map_or_else(|| how.to_owned(), |w| format!("{w}. {how}"))),
            keywords: &["autostart", "login", "startup"],
            kind: Kind::Switch,
            default: Value::Bool(true),
            scope: Scope::Account,
            applies: Applies::NextLogin,
        });
    }

    let mut add = vec![Choice::new("", "Choose a program…")];
    add.extend(
        installable(dirs, here)
            .into_iter()
            .filter(|a| !programs.iter().any(|p| p.file == a.file))
            .map(|a| Choice::new(stem(&a.file), a.name)),
    );
    out.push(Setting {
        id: ADD,
        title: "Add a program",
        description: "Start an installed application every time you log in.",
        keywords: &["autostart", "login", "startup", "launch", "open at login"],
        kind: Kind::Choice(add),
        default: Value::Text(String::new()),
        scope: Scope::Account,
        applies: Applies::NextLogin,
    });

    let added: Vec<Choice> = programs
        .iter()
        .filter(|p| p.added())
        .map(|p| Choice::new(stem(&p.file), p.name.clone()))
        .collect();
    if !added.is_empty() {
        let mut remove = vec![Choice::new("", "Choose a program…")];
        remove.extend(added);
        out.push(Setting {
            id: REMOVE,
            title: "Remove a program",
            description: "Stop starting one you added. Programs installed to start \
                          can only be switched off.",
            keywords: &["autostart", "login", "startup", "delete"],
            kind: Kind::Choice(remove),
            default: Value::Text(String::new()),
            scope: Scope::Account,
            applies: Applies::NextLogin,
        });
    }
    out
}

/// Whether a setting is one program's switch, which only this account's
/// files decide: not something to list anywhere made ahead of time.
#[must_use]
pub fn is_program(id: &str) -> bool {
    id.starts_with("startup.") && id != ADD && id != REMOVE
}

/// The program a switch is for.
fn program(env: &Env, id: &str) -> Result<Program, String> {
    let wanted = id.strip_prefix("startup.").unwrap_or(id);
    programs(env)
        .into_iter()
        .find(|p| stem(&p.file) == wanted)
        .ok_or_else(|| format!("nothing called `{wanted}` starts at login"))
}

/// A setting's value.
///
/// # Errors
/// The program it is for no longer starts at login, or never did.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    if s.id == ADD || s.id == REMOVE {
        return Ok(Value::Text(String::new()));
    }
    program(env, s.id).map(|p| Value::Bool(p.enabled()))
}

/// Change one: turn a program on or off, add one, or remove one.
///
/// # Errors
/// No such program or application, or its entry could not be written.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<(), String> {
    set_in(env, s, value, &desktop_entry::data_dirs(), &here())
}

fn set_in(
    env: &Env,
    s: &Setting,
    value: Option<&Value>,
    dirs: &[PathBuf],
    here: &Environment,
) -> Result<(), String> {
    let text = value.and_then(Value::as_text).unwrap_or_default();
    match s.id {
        // Nothing chosen: the list's first line, which only says to choose.
        ADD | REMOVE if text.is_empty() => Ok(()),
        ADD => add(env, text, dirs, here),
        REMOVE => remove(env, text, here),
        _ => {
            let on = value.and_then(Value::as_bool).unwrap_or(true);
            switch(env, &program(env, s.id)?, on)
        }
    }
}

/// Copy an installed application's entry into the account's autostart.
fn add(env: &Env, stem_wanted: &str, dirs: &[PathBuf], here: &Environment) -> Result<(), String> {
    let app = installable(dirs, here)
        .into_iter()
        .find(|a| stem(&a.file) == stem_wanted)
        .ok_or_else(|| format!("no application `{stem_wanted}` is installed"))?;
    if programs_for(env, here).iter().any(|p| p.file == app.file) {
        return Err(format!(
            "{} already starts at login; its switch is on the Startup page",
            app.name
        ));
    }
    let text = std::fs::read_to_string(&app.path).map_err(|e| io_error(&app.path, &e))?;
    write(&env.account(ACCOUNT).join(&app.file), &text)
}

/// Delete an entry that was added, not installed.
fn remove(env: &Env, stem_wanted: &str, here: &Environment) -> Result<(), String> {
    let p = programs_for(env, here)
        .into_iter()
        .find(|p| stem(&p.file) == stem_wanted)
        .ok_or_else(|| format!("nothing called `{stem_wanted}` starts at login"))?;
    match (&p.system, &p.account) {
        (None, Some(path)) => std::fs::remove_file(path).map_err(|e| io_error(path, &e)),
        _ => Err(format!(
            "{} was installed to start; it can be switched off instead",
            p.name
        )),
    }
}

/// Turn a program on or off in the account's copy of its entry.
fn switch(env: &Env, p: &Program, on: bool) -> Result<(), String> {
    let path = p
        .account
        .clone()
        .unwrap_or_else(|| env.account(ACCOUNT).join(&p.file));
    let current = match &p.account {
        Some(path) => std::fs::read_to_string(path).map_err(|e| io_error(path, &e))?,
        None => p.system.clone().unwrap_or_default(),
    };
    if on {
        let mut text = with_key(&current, "Hidden", None);
        if Entry::parse(&text)
            .raw("X-GNOME-Autostart-enabled")
            .is_some()
        {
            text = with_key(&text, "X-GNOME-Autostart-enabled", Some("true"));
        }
        // Nothing of the account's left in it: the package's entry is as good.
        if p.system.as_deref() == Some(text.as_str()) {
            if p.account.is_some() {
                std::fs::remove_file(&path).map_err(|e| io_error(&path, &e))?;
            }
            return Ok(());
        }
        if text == current && p.account.is_some() {
            return Ok(());
        }
        write(&path, &text)
    } else {
        write(&path, &with_key(&current, "Hidden", Some("true")))
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_error(dir, &e))?;
    }
    std::fs::write(path, text).map_err(|e| io_error(path, &e))
}

/// `text` with `key` in its main group set to `value`, or taken out.
fn with_key(text: &str, key: &str, value: Option<&str>) -> String {
    crate::ini::with_key(text, "Desktop Entry", key, value)
}

#[cfg(test)]
mod tests {
    use super::{ACCOUNT, ADD, REMOVE, SYSTEM, programs_for, set_in, settings_for};
    use crate::env::Env;
    use crate::model::{Kind, Value};
    use alpymist_core::desktop_entry::Environment;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn here() -> Environment {
        Environment {
            desktops: vec!["Hyprland".into()],
            locales: Vec::new(),
            path: vec!["/bin".into(), "/usr/bin".into()],
        }
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    const APPLET: &str = "[Desktop Entry]\nType=Application\nName=Network\n\
                          Comment=Networks in the tray\nExec=nm-applet\n";

    /// A system with what Alpine installs to start, an applet, and one
    /// application to add.
    fn setup(name: &str) -> (PathBuf, Env, Vec<PathBuf>) {
        let d =
            std::env::temp_dir().join(format!("alpymist-startup-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, false, &RAN);
        let system = env.system(SYSTEM);
        put(
            &system.join("pipewire.desktop"),
            "[Desktop Entry]\nType=Application\nName=PipeWire\nExec=/usr/libexec/pipewire-launcher\n",
        );
        put(
            &system.join("at-spi-dbus-bus.desktop"),
            "[Desktop Entry]\nType=Application\nName=AT-SPI\nExec=at-spi-bus-launcher\n\
             OnlyShowIn=GNOME;Unity;\nNoDisplay=true\n",
        );
        put(
            &system.join("xdg-user-dirs.desktop"),
            "[Desktop Entry]\nType=Application\nName=User folders\nExec=xdg-user-dirs-update\n\
             NoDisplay=true\n",
        );
        put(&system.join("nm-applet.desktop"), APPLET);
        let share = d.join("share");
        put(
            &share.join("applications/org.example.Notes.desktop"),
            "[Desktop Entry]\nType=Application\nName=Notes\nExec=notes %U\n",
        );
        put(
            &share.join("applications/hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=x\nNoDisplay=true\n",
        );
        (d, env, vec![share])
    }

    fn ids(env: &Env, dirs: &[PathBuf]) -> Vec<&'static str> {
        settings_for(env, dirs, &here())
            .iter()
            .map(|s| s.id)
            .collect()
    }

    #[test]
    fn what_the_desktop_starts_or_another_desktop_wants_is_left_out() {
        let (d, env, dirs) = setup("listed");
        let files: Vec<String> = programs_for(&env, &here())
            .into_iter()
            .map(|p| p.file)
            .collect();
        // PipeWire is the desktop's own; AT-SPI is GNOME's.
        assert_eq!(files, ["nm-applet.desktop", "xdg-user-dirs.desktop"]);
        // The folders still start, but only the applet has a switch.
        assert_eq!(ids(&env, &dirs), ["startup.nm-applet", ADD]);
        let settings = settings_for(&env, &dirs, &here());
        assert_eq!(
            settings[0].description,
            "Networks in the tray. Installed to start."
        );
        let Kind::Choice(choices) = &settings[1].kind else {
            panic!("adding is a choice")
        };
        let offered: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(offered, ["", "org.example.Notes"]);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn off_hides_the_system_entry_and_on_puts_it_back() {
        let (d, env, dirs) = setup("switch");
        let settings = settings_for(&env, &dirs, &here());
        let applet = &settings[0];
        let copy = env.account(ACCOUNT).join("nm-applet.desktop");

        set_in(&env, applet, Some(&Value::Bool(false)), &dirs, &here()).unwrap();
        let hidden = std::fs::read_to_string(&copy).unwrap();
        assert_eq!(
            hidden,
            APPLET.replace("[Desktop Entry]\n", "[Desktop Entry]\nHidden=true\n")
        );
        assert!(!programs_for(&env, &here())[0].enabled());

        set_in(&env, applet, Some(&Value::Bool(true)), &dirs, &here()).unwrap();
        assert!(!copy.exists(), "nothing of the account's is left in it");
        assert!(programs_for(&env, &here())[0].enabled());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn an_added_program_can_be_switched_off_and_removed() {
        let (d, env, dirs) = setup("add");
        let settings = settings_for(&env, &dirs, &here());
        let add = settings.iter().find(|s| s.id == ADD).unwrap();
        set_in(
            &env,
            add,
            Some(&Value::Text("org.example.Notes".into())),
            &dirs,
            &here(),
        )
        .unwrap();
        let copy = env.account(ACCOUNT).join("org.example.Notes.desktop");
        assert!(
            std::fs::read_to_string(&copy)
                .unwrap()
                .contains("Exec=notes %U")
        );
        assert!(
            set_in(
                &env,
                add,
                Some(&Value::Text("org.example.Notes".into())),
                &dirs,
                &here()
            )
            .unwrap_err()
            .contains("already starts")
        );

        let settings = settings_for(&env, &dirs, &here());
        assert_eq!(
            settings.iter().map(|s| s.id).collect::<Vec<_>>(),
            [
                "startup.nm-applet",
                "startup.org.example.Notes",
                ADD,
                REMOVE
            ]
        );
        let notes = &settings[1];
        assert_eq!(notes.description, "You added it.");
        set_in(&env, notes, Some(&Value::Bool(false)), &dirs, &here()).unwrap();
        assert!(
            std::fs::read_to_string(&copy)
                .unwrap()
                .contains("Hidden=true\n")
        );
        set_in(&env, notes, None, &dirs, &here()).unwrap();
        assert!(!std::fs::read_to_string(&copy).unwrap().contains("Hidden"));

        let remove = settings.iter().find(|s| s.id == REMOVE).unwrap();
        assert!(
            set_in(
                &env,
                remove,
                Some(&Value::Text("nm-applet".into())),
                &dirs,
                &here()
            )
            .unwrap_err()
            .contains("switched off")
        );
        set_in(
            &env,
            remove,
            Some(&Value::Text("org.example.Notes".into())),
            &dirs,
            &here(),
        )
        .unwrap();
        assert!(!copy.exists());
        assert_eq!(ids(&env, &dirs), ["startup.nm-applet", ADD]);
        std::fs::remove_dir_all(d).ok();
    }
}

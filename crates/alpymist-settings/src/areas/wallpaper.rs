//! The desktop's wallpaper: which picture, and keeping it on the screen.
//!
//! The pictures are whatever is installed in [`DIR`]. `alpymist-wallpapers`
//! puts Alpymist's there, and anything else that ships a JPEG or PNG beside
//! them is offered the same way, with nothing here naming any of them but the
//! [`DEFAULT`]. The choice is the account's, in [`CONFIG`].
//!
//! Neither compositor draws a wallpaper itself. `swaybg` does, on Hyprland and
//! labwc alike, and it takes its picture on the command line and never looks
//! again, so [`show`] starts a new one on the chosen picture and only then
//! stops the old: a change shows with no moment of bare desktop between the
//! two. On X11, `feh` sets the root window's picture and exits. `alpymist
//! wallpaper` runs [`show`] at login, and Settings after every change.
//!
//! An account made before this started `swaybg` or `feh` itself, from a file
//! that is its own. The first time its wallpaper is changed, that line becomes
//! `alpymist wallpaper`, with the file as it was kept beside it. Otherwise the
//! change would last until the next login and then quietly undo itself.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

/// The setting's id.
pub const ID: &str = "appearance.wallpaper";

/// Where the pictures are installed, under the system root.
pub const DIR: &str = "usr/share/backgrounds/alpymist";

/// The account's choice.
pub const CONFIG: &str = "alpymist/wallpaper.toml";

/// The picture an account starts with, in [`DIR`].
pub const DEFAULT: &str = "blue-hour.jpg";

/// What the file kept from before a takeover is called: the original's name
/// with this after it.
pub const KEPT: &str = ".bak-wallpaper";

/// How long a new `swaybg` gets to cover the screen before the old one goes.
///
/// It has a picture to decode first, through gdk-pixbuf's sandboxed loaders,
/// and the Atom this is meant for is not fast at either. Too short shows the
/// bare desktop for a moment; too long costs nothing anyone sees, since the
/// old picture stays up until then.
const COVER: Duration = Duration::from_millis(800);

/// Whether `path` names a picture this can offer: a JPEG or a PNG, which is
/// what `swaybg` and `feh` both read.
fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["jpg", "jpeg", "png"].contains(&e.to_ascii_lowercase().as_str()))
}

/// A picture's name as a person reads it: `blue-hour.jpg` is "Blue hour".
#[must_use]
pub fn label(file: &str) -> String {
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file);
    let spaced = stem.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The path a choice names, as `swaybg` is given it: a picture in [`DIR`] by
/// its file name, which is what the choices hold, or any picture by its
/// whole path.
#[must_use]
pub fn path_of(choice: &str) -> String {
    if choice.starts_with('/') {
        choice.to_owned()
    } else {
        format!("/{DIR}/{choice}")
    }
}

/// Where a choice is under `env`'s root.
fn on_disk(env: &Env, choice: &str) -> std::path::PathBuf {
    env.system(path_of(choice).trim_start_matches('/'))
}

/// Every picture installed: its file name, and what it is called.
#[must_use]
pub fn choices(env: &Env) -> Vec<Choice> {
    let Ok(entries) = std::fs::read_dir(env.system(DIR)) else {
        return Vec::new();
    };
    let mut found: Vec<Choice> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_picture(p) && p.is_file())
        .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
        .map(|file| Choice::new(file.clone(), label(&file)))
        .collect();
    found.sort_by(|a, b| a.label.cmp(&b.label));
    found
}

/// The setting.
///
/// Its choices are found once, from the real system: a `Setting` lives as
/// long as the process, as the screensavers' do, and the pictures installed
/// do not change under a running Settings.
pub fn settings() -> Vec<Setting> {
    static INSTALLED: OnceLock<Vec<Choice>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(|| {
        let mut found = choices(&Env::detect());
        // Always one of the choices, as a default has to be; choosing it where
        // it is not installed says so, like choosing anything else that is not.
        if !found.iter().any(|c| c.value == DEFAULT) {
            found.push(Choice::new(DEFAULT, label(DEFAULT)));
        }
        found
    });
    vec![Setting {
        id: ID,
        title: "Wallpaper",
        description: "The picture behind the desktop.",
        keywords: &["background", "picture", "desktop", "image", "photo"],
        kind: Kind::Choice(installed.clone()),
        default: Value::Text(DEFAULT.to_owned()),
        scope: Scope::Account,
        applies: Applies::Now,
    }]
}

/// The picture the account chose, if it is still installed.
fn chosen(env: &Env) -> Option<String> {
    let text = std::fs::read_to_string(env.account(CONFIG)).ok()?;
    let table: toml::Table = text.parse().ok()?;
    let picture = table.get("picture")?.as_str()?;
    on_disk(env, picture).is_file().then(|| picture.to_owned())
}

/// The picture to show, as a choice names it: the account's, or the default
/// while there is none or it has been uninstalled, or failing that whatever
/// is there.
#[must_use]
pub fn current(env: &Env) -> Option<String> {
    chosen(env)
        .or_else(|| on_disk(env, DEFAULT).is_file().then(|| DEFAULT.to_owned()))
        .or_else(|| choices(env).into_iter().next().map(|c| c.value))
}

/// The setting's value.
#[must_use]
pub fn get(env: &Env) -> Value {
    Value::Text(current(env).unwrap_or_default())
}

/// Choose a picture, or the default with `None`, and make sure the account
/// starts it at login. Returns notes worth telling.
///
/// # Errors
/// When there is no picture at the path given, or the choice cannot be saved.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    let value = value.unwrap_or(&setting.default);
    let picture = value.as_text().unwrap_or_default();
    if !is_picture(Path::new(picture)) || !on_disk(env, picture).is_file() {
        return Err(format!("there is no picture at {}", path_of(picture)));
    }
    let line = format!("picture = {}\n", toml::Value::String(picture.to_owned()));
    crate::generated::replace(&env.account(CONFIG), &line)?;
    Ok(take_over(env))
}

/// Put the choice on the screen, from the person's own session.
///
/// # Errors
/// When `alpymist wallpaper` cannot be run or says it failed.
pub fn live(env: &Env) -> Result<(), String> {
    env.run(&["alpymist", "wallpaper"]).map(drop)
}

/// A file an account starts its wallpaper from, and how it says so.
struct Launcher {
    /// Under the account's configuration directory.
    file: &'static str,
    /// Which desktop it is, for the note.
    desktop: &'static str,
    /// Whether a line, trimmed, is the one that starts the wallpaper.
    starts_wallpaper: fn(&str) -> bool,
    /// What it becomes.
    replacement: &'static str,
}

/// The first word of a command, after an `exec-once =` or an `exec` and its
/// flags, if the line is one.
fn program<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?;
    rest.trim_start()
        .trim_start_matches('=')
        .split_whitespace()
        .find(|w| !w.starts_with('-'))
}

const LAUNCHERS: [Launcher; 3] = [
    Launcher {
        file: "hypr/hyprland.conf",
        desktop: "Hyprland",
        starts_wallpaper: |l| program(l, "exec-once").is_some_and(|p| p == "swaybg"),
        replacement: "exec-once = alpymist wallpaper",
    },
    Launcher {
        file: "labwc/autostart",
        desktop: "labwc",
        starts_wallpaper: |l| l.split_whitespace().next() == Some("swaybg"),
        replacement: "alpymist wallpaper >/dev/null 2>&1 &",
    },
    Launcher {
        file: "i3/config",
        desktop: "i3",
        starts_wallpaper: |l| program(l, "exec").is_some_and(|p| p == "feh") && l.contains("--bg-"),
        replacement: "exec --no-startup-id alpymist wallpaper",
    },
];

/// `text` with every line that starts a wallpaper itself turned into
/// `launcher`'s replacement, or `None` if there was none.
fn rewrite(text: &str, launcher: &Launcher) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if (launcher.starts_wallpaper)(trimmed) {
                changed = true;
                let indent = &line[..line.len() - trimmed.len()];
                format!("{indent}{}", launcher.replacement)
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !changed {
        return None;
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// Where to keep `path` as it was: beside it with [`KEPT`] after its name, or
/// a number after that if something is there already, which is never
/// replaced.
fn keeping(path: &Path) -> std::path::PathBuf {
    let named = |n: u32| {
        let mut name = path.as_os_str().to_owned();
        name.push(KEPT);
        if n > 1 {
            name.push(format!("-{n}"));
        }
        std::path::PathBuf::from(name)
    };
    (1..=u32::MAX)
        .map(named)
        .find(|p| !p.exists())
        .unwrap_or_else(|| named(1))
}

/// Make each of the account's desktops start the wallpaper with `alpymist
/// wallpaper`, keeping each file as it was beside it.
///
/// Says what it did, and what it could not: a note is all a failure here is,
/// since the picture has been chosen and is on the screen either way.
fn take_over(env: &Env) -> Vec<String> {
    let mut notes = Vec::new();
    for launcher in &LAUNCHERS {
        let path = env.account(launcher.file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(new) = rewrite(&text, launcher) else {
            continue;
        };
        let kept = keeping(&path);
        let saved = std::fs::write(&kept, &text).is_ok();
        let written = saved && crate::generated::replace(&path, &new).is_ok();
        notes.push(if written {
            format!(
                "{} now starts the wallpaper from Settings; its configuration as it was is in {}.",
                launcher.desktop,
                kept.display()
            )
        } else {
            format!(
                "{} starts a wallpaper of its own from {}, which could not be changed: \
                 this one lasts until the next login.",
                launcher.desktop,
                path.display()
            )
        });
    }
    notes
}

/// Put the account's wallpaper on the screen, replacing whatever was there.
///
/// # Errors
/// When no picture is installed, there is no graphical session, or the
/// program that draws it cannot be started.
pub fn show(env: &Env) -> Result<(), String> {
    let picture = path_of(&current(env).ok_or("no wallpaper is installed")?);
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    if set("WAYLAND_DISPLAY") {
        let before = running("swaybg");
        Command::new("swaybg")
            .args(["-m", "fill", "-i", &picture])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("swaybg: {e}"))?;
        if !before.is_empty() {
            std::thread::sleep(COVER);
            for pid in before {
                // Gone already is as good as stopped.
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
        Ok(())
    } else if set("DISPLAY") {
        let status = Command::new("feh")
            .args(["--no-fehbg", "--bg-fill", &picture])
            .status()
            .map_err(|e| format!("feh: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("feh could not show {picture}"))
        }
    } else {
        Err("there is no graphical session to show a wallpaper in".into())
    }
}

/// The processes called `name` that belong to this account.
fn running(name: &str) -> Vec<u32> {
    let uid = |pid: &str| -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))?
            .split_whitespace()
            .next()
            .map(str::to_owned)
    };
    let Some(mine) = uid("self") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .is_ok_and(|comm| comm.trim() == name)
                && uid(&pid.to_string()).as_deref() == Some(mine.as_str())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CONFIG, DIR, KEPT, LAUNCHERS, choices, current, label, rewrite, take_over};
    use crate::env::Env;
    use std::sync::Mutex;

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// A test system with `pictures` installed.
    fn system(name: &str, pictures: &[&str]) -> (std::path::PathBuf, Env) {
        let dir =
            std::env::temp_dir().join(format!("alpymist-wallpaper-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        std::fs::create_dir_all(env.system(DIR)).unwrap();
        for p in pictures {
            std::fs::write(env.system(DIR).join(p), b"").unwrap();
        }
        (dir, env)
    }

    #[test]
    fn a_picture_is_named_for_its_file() {
        assert_eq!(label("blue-hour.jpg"), "Blue hour");
        assert_eq!(label("milky_way.png"), "Milky way");
        assert_eq!(label("alpymist.png"), "Alpymist");
    }

    #[test]
    fn every_picture_installed_is_offered_and_nothing_else() {
        let (dir, env) = system("offered", &["red-sun.jpg", "alpymist.png", "notes.txt"]);
        let found = choices(&env);
        let labels: Vec<&str> = found.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Alpymist", "Red sun"]);
        assert_eq!(
            super::path_of(&found[1].value),
            "/usr/share/backgrounds/alpymist/red-sun.jpg"
        );
        assert_eq!(found[1].value, "red-sun.jpg");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_default_stands_until_a_choice_and_returns_if_that_goes() {
        let (dir, env) = system("default", &["blue-hour.jpg", "red-sun.jpg"]);
        let blue = "blue-hour.jpg";
        assert_eq!(current(&env).as_deref(), Some(blue));

        let red = "red-sun.jpg";
        std::fs::create_dir_all(env.account("alpymist")).unwrap();
        std::fs::write(env.account(CONFIG), format!("picture = \"{red}\"\n")).unwrap();
        assert_eq!(current(&env).as_deref(), Some(red));

        std::fs::remove_file(env.system(DIR).join("red-sun.jpg")).unwrap();
        assert_eq!(current(&env).as_deref(), Some(blue), "uninstalled");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_hyprland_configuration_hands_its_wallpaper_over() {
        let conf = "$wallpaper = /x.png\n\
                    exec-once = dbus-update-activation-environment WAYLAND_DISPLAY\n\
                    exec-once = swaybg -m fill -i $wallpaper\n\
                    exec-once = waybar\n";
        let new = rewrite(conf, &LAUNCHERS[0]).unwrap();
        assert!(new.contains("exec-once = alpymist wallpaper\n"));
        assert!(!new.contains("swaybg"));
        assert!(new.contains("exec-once = waybar\n"), "the rest is left");
        assert!(rewrite(&new, &LAUNCHERS[0]).is_none(), "once is enough");
        assert!(rewrite("exec-once=swaybg -i /x.png\n", &LAUNCHERS[0]).is_some());
    }

    #[test]
    fn labwc_and_i3_hand_theirs_over_too() {
        let autostart = "/usr/libexec/pipewire-launcher >/dev/null 2>&1 &\n\
                         swaybg -m fill -i /x.png >/dev/null 2>&1 &\n";
        let new = rewrite(autostart, &LAUNCHERS[1]).unwrap();
        assert!(new.ends_with("alpymist wallpaper >/dev/null 2>&1 &\n"));

        let i3 = "exec --no-startup-id xsetroot -cursor_name left_ptr\n\
                  exec --no-startup-id feh --no-fehbg --bg-fill /x.png\n";
        let new = rewrite(i3, &LAUNCHERS[2]).unwrap();
        assert!(new.ends_with("exec --no-startup-id alpymist wallpaper\n"));
        assert!(new.contains("xsetroot"));
    }

    #[test]
    fn a_line_that_only_mentions_the_programs_is_left_alone() {
        let conf = "# exec-once = swaybg is how this used to be done\n\
                    bind = SUPER, W, exec, swaybg -i /y.png\n";
        assert!(rewrite(conf, &LAUNCHERS[0]).is_none());
    }

    #[test]
    fn a_takeover_keeps_the_file_as_it_was() {
        let (dir, env) = system("takeover", &["blue-hour.jpg"]);
        let conf = env.account("hypr/hyprland.conf");
        std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
        let before = "exec-once = swaybg -m fill -i /x.png\n";
        std::fs::write(&conf, before).unwrap();

        let notes = take_over(&env);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(
            std::fs::read_to_string(&conf).unwrap(),
            "exec-once = alpymist wallpaper\n"
        );
        let mut kept = conf.into_os_string();
        kept.push(KEPT);
        assert_eq!(std::fs::read_to_string(kept).unwrap(), before);
        assert!(take_over(&env).is_empty(), "nothing left to take over");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_file_already_kept_is_never_replaced() {
        let (dir, env) = system("kept-twice", &["blue-hour.jpg"]);
        let conf = env.account("hypr/hyprland.conf");
        std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
        let mut first = conf.clone().into_os_string();
        first.push(KEPT);
        std::fs::write(&first, "something older\n").unwrap();
        std::fs::write(&conf, "exec-once = swaybg -i /x.png\n").unwrap();

        let notes = take_over(&env);
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "something older\n"
        );
        let mut second = first.clone();
        second.push("-2");
        assert_eq!(
            std::fs::read_to_string(&second).unwrap(),
            "exec-once = swaybg -i /x.png\n"
        );
        assert!(notes[0].contains("bak-wallpaper-2"), "{notes:?}");
        std::fs::remove_dir_all(dir).ok();
    }
}

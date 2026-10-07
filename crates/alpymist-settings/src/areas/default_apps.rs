//! Default applications: which one opens each kind of thing, and which
//! terminal commands run in.
//!
//! One category — Web browser, Images, Terminal — is one setting, and one
//! choice sets every type it covers, so choosing a browser changes links,
//! web pages and `https:` together. The types live in [`CATEGORIES`] and
//! nowhere else.
//!
//! A choice is written to `~/.config/mimeapps.list`, one key at a time. That
//! file is not Alpymist's alone: browsers write it when asked to be the
//! default, and so do GIO and Flatpak. So it is edited in place rather than
//! generated (ADR 0007's addendum of 2026-09-28): the category's keys change,
//! and every other line stays. Automatic takes the keys out again, and the
//! system's choice or whatever says it opens the type decides.
//!
//! The terminal has no type. It is chosen in `~/.config/xdg-terminals.list`,
//! as `xdg-terminal-exec` reads it, and `alpymist open terminal` runs it.
//!
//! Super+B and Super+Return run `$browser` and `$terminal` from the account's
//! own `hyprland.conf`, which named `librewolf` and `foot` outright until new
//! accounts were given `alpymist open`. The first time an older account
//! changes its browser or terminal here, that one line is changed to
//! `alpymist open …` if it is still as every account started with it, the
//! file kept as it was beside it, and Hyprland reloaded, as the theme does
//! with its lines (ADR 0007). A line of the account's own is left alone.

use crate::env::Env;
use crate::io_error;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::defaults::{self, Places};
use alpymist_core::desktop_entry::{self, Environment, Found};
use std::fmt::Write as _;
use std::path::Path;

/// The group of `mimeapps.list` that says what opens what.
const GROUP: &str = "Default Applications";
/// The account's list.
const MIMEAPPS: &str = "mimeapps.list";
/// The account's terminal list.
const TERMINALS: &str = "xdg-terminals.list";
/// The account's Hyprland configuration.
const HYPRLAND: &str = "hypr/hyprland.conf";
/// What `hyprland.conf` is kept beside it as, before a key is changed.
pub const KEPT: &str = ".bak-defaults";
/// The variables Hyprland's keys run, what every account was given at first,
/// and the category each follows once changed.
const KEYS: &[(&str, &str, &str)] = &[
    ("$browser", "librewolf", "browser"),
    ("$terminal", "foot", "terminal"),
];

/// A kind of thing to open.
#[derive(Debug)]
pub struct Category {
    /// The last part of its setting's id, and what `alpymist open` takes:
    /// `browser`.
    pub id: &'static str,
    /// Its name: `Web browser`.
    pub title: &'static str,
    /// What it opens.
    pub description: &'static str,
    /// Other words to find it by.
    pub keywords: &'static [&'static str],
    /// The MIME types and URL schemes it covers, the one that decides what is
    /// shown first. None for the terminal.
    pub types: &'static [&'static str],
}

/// Every category, in the order the page lists them.
pub const CATEGORIES: &[Category] = &[
    Category {
        id: "browser",
        title: "Web browser",
        description: "Opens links and web pages.",
        keywords: &["web", "internet", "links", "http", "html"],
        types: &[
            "x-scheme-handler/https",
            "x-scheme-handler/http",
            "text/html",
            "application/xhtml+xml",
        ],
    },
    Category {
        id: "mail",
        title: "Mail",
        description: "Opens email addresses and messages.",
        keywords: &["email", "mailto", "e-mail"],
        types: &["x-scheme-handler/mailto", "message/rfc822"],
    },
    Category {
        id: "files",
        title: "File manager",
        description: "Opens folders.",
        keywords: &["folders", "directory", "browse files"],
        types: &["inode/directory"],
    },
    Category {
        id: "editor",
        title: "Text editor",
        description: "Opens text files, configuration and code.",
        keywords: &["text", "code", "notes", "editor"],
        types: &[
            "text/plain",
            "text/x-log",
            "text/markdown",
            "text/csv",
            "text/xml",
            "text/x-rust",
            "text/x-python",
            "text/x-shellscript",
            "application/json",
            "application/xml",
            "application/x-yaml",
            "application/toml",
            "application/x-shellscript",
        ],
    },
    Category {
        id: "terminal",
        title: "Terminal",
        description: "Runs commands, and the programs that run in one.",
        keywords: &["console", "shell", "command line"],
        types: &[],
    },
    Category {
        id: "images",
        title: "Images",
        description: "Opens pictures and photos.",
        keywords: &["pictures", "photos", "viewer", "png", "jpeg"],
        types: &[
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "image/svg+xml",
            "image/bmp",
            "image/tiff",
            "image/avif",
            "image/heif",
        ],
    },
    Category {
        id: "pdf",
        title: "PDF",
        description: "Opens PDF documents.",
        keywords: &["documents", "reader"],
        types: &["application/pdf"],
    },
    Category {
        id: "video",
        title: "Video",
        description: "Plays films and clips.",
        keywords: &["movies", "player", "mp4", "mkv"],
        types: &[
            "video/mp4",
            "video/x-matroska",
            "video/webm",
            "video/quicktime",
            "video/x-msvideo",
            "video/mpeg",
            "video/ogg",
        ],
    },
    Category {
        id: "music",
        title: "Music",
        description: "Plays songs and sound files.",
        keywords: &["audio", "player", "mp3", "flac"],
        types: &[
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-vorbis+ogg",
            "audio/opus",
            "audio/wav",
            "audio/x-wav",
            "audio/mp4",
            "audio/aac",
        ],
    },
    Category {
        id: "archives",
        title: "Archives",
        description: "Opens zip files, tarballs and other archives.",
        keywords: &["zip", "tar", "compressed", "extract"],
        types: &[
            "application/zip",
            "application/x-tar",
            "application/gzip",
            "application/x-compressed-tar",
            "application/x-xz-compressed-tar",
            "application/x-7z-compressed",
            "application/x-bzip2",
            "application/vnd.rar",
            "application/x-rar",
            "application/zstd",
        ],
    },
    Category {
        id: "calendar",
        title: "Calendar",
        description: "Opens calendar files and subscriptions.",
        keywords: &["events", "ics", "webcal"],
        types: &["text/calendar", "x-scheme-handler/webcal"],
    },
];

/// The category a setting or `alpymist open` names.
#[must_use]
pub fn category(id: &str) -> Option<&'static Category> {
    let id = id.strip_prefix("default.").unwrap_or(id);
    CATEGORIES.iter().find(|c| c.id == id)
}

/// Where to look, for this environment's account and system.
fn places(env: &Env) -> Places {
    Places {
        config_home: env.config.clone(),
        config_dirs: vec![env.system("etc/xdg")],
        data_dirs: desktop_entry::data_dirs(),
        desktops: vec!["Hyprland".into()],
    }
}

/// The places, but only the account's lists: what the account itself chose.
fn own(places: &Places) -> Places {
    Places {
        config_dirs: Vec::new(),
        data_dirs: Vec::new(),
        ..places.clone()
    }
}

/// What decides whether an application applies here.
fn here() -> Environment {
    Environment {
        desktops: vec!["Hyprland".into()],
        ..Environment::current()
    }
}

/// `librewolf` from `librewolf.desktop`: what a choice's value is.
fn stem(id: &str) -> &str {
    id.strip_suffix(".desktop").unwrap_or(id)
}

/// A string that lives as long as the process, for a `Setting` to carry.
fn kept(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The applications that could be chosen for a category, by name.
fn candidates<'a>(c: &Category, apps: &'a [Found], here: &Environment) -> Vec<&'a Found> {
    let mut found: Vec<&Found> = apps
        .iter()
        .filter(|f| {
            defaults::usable(&f.entry)
                && !f.entry.yes("NoDisplay")
                && f.entry.applies(here)
                && if c.types.is_empty() {
                    defaults::is_terminal(&f.entry)
                } else {
                    c.types.iter().any(|t| defaults::handles(&f.entry, t))
                }
        })
        .collect();
    found.sort_by_cached_key(|f| name(f, here).to_lowercase());
    found
}

fn name(f: &Found, here: &Environment) -> String {
    f.entry
        .localised("Name", &here.locales)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| stem(&f.id).to_owned())
}

/// What opens a category's things, as far as `places` says: its first type's
/// default, or the terminal.
fn resolved<'a>(c: &Category, places: &Places, apps: &'a [Found]) -> Option<&'a Found> {
    match c.types.first() {
        Some(first) => defaults::default_for(places, apps, first),
        None => defaults::terminal(places, apps),
    }
}

/// What the account itself chose, if anything is installed that it names.
fn chosen<'a>(c: &Category, places: &Places, apps: &'a [Found]) -> Option<&'a Found> {
    let own = own(places);
    match c.types.first() {
        Some(first) => defaults::listed(&own.mimeapps(), first)
            .iter()
            .find_map(|id| {
                apps.iter()
                    .find(|f| &f.id == id && defaults::usable(&f.entry))
            }),
        None => defaults::listed_terminals(&own.terminal_lists())
            .iter()
            .find_map(|id| {
                apps.iter().find(|f| {
                    &f.id == id && defaults::usable(&f.entry) && defaults::is_terminal(&f.entry)
                })
            }),
    }
}

/// A setting for each category.
#[must_use]
pub fn settings() -> Vec<Setting> {
    settings_for(&places(&Env::detect()), &here())
}

fn settings_for(places: &Places, here: &Environment) -> Vec<Setting> {
    let apps = places.applications();
    CATEGORIES
        .iter()
        .map(|c| {
            // Automatic says what it comes to, so choosing it is not a leap.
            let fallback = {
                let system = Places {
                    config_home: Path::new("/nonexistent").to_path_buf(),
                    ..places.clone()
                };
                resolved(c, &system, &apps).map(|f| name(f, here))
            };
            let automatic = match fallback {
                Some(n) => format!("Automatic ({n})"),
                None if c.types.is_empty() => {
                    format!("Automatic ({})", defaults::FALLBACK_TERMINAL)
                }
                None => "Automatic (nothing installed)".to_owned(),
            };
            let mut choices = vec![Choice::new("", automatic)];
            let mut offered = candidates(c, &apps, here);
            // What the account chose stays offered even when it would not be
            // otherwise — hidden from menus, say — so the page shows it.
            if let Some(f) = chosen(c, places, &apps)
                && !offered.iter().any(|o| o.id == f.id)
            {
                offered.push(f);
            }
            choices.extend(
                offered
                    .iter()
                    .map(|f| Choice::new(stem(&f.id), name(f, here))),
            );
            let description = if choices.len() == 1 && !c.types.is_empty() {
                format!(
                    "{} Nothing installed opens these yet; the store has applications that do.",
                    c.description
                )
            } else {
                c.description.to_owned()
            };
            Setting {
                id: kept(format!("default.{}", c.id)),
                title: c.title,
                description: kept(description),
                keywords: c.keywords,
                kind: Kind::Choice(choices),
                default: Value::Text(String::new()),
                scope: Scope::Account,
                applies: Applies::Now,
            }
        })
        .collect()
}

/// What the account chose for a category: an application's id, or nothing
/// for Automatic.
///
/// # Errors
/// Not a category.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    get_in(&places(env), s)
}

fn get_in(places: &Places, s: &Setting) -> Result<Value, String> {
    let c = category(s.id).ok_or_else(|| format!("no category `{}`", s.id))?;
    let apps = places.applications();
    Ok(Value::Text(
        chosen(c, places, &apps).map_or_else(String::new, |f| stem(&f.id).to_owned()),
    ))
}

/// Choose an application for a category, or with nothing, go back to
/// Automatic.
///
/// # Errors
/// Not a category, not an application that opens it, or the list could not
/// be written.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    set_in(env, &places(env), &here(), s, value)?;
    Ok(category(s.id)
        .map(|c| take_over(env, c))
        .unwrap_or_default())
}

/// Make the account's key for a category follow the choice, if its
/// `hyprland.conf` still names what every account started with. Says what it
/// did.
fn take_over(env: &Env, c: &Category) -> Vec<String> {
    let Some(&(variable, shipped, _)) = KEYS.iter().find(|(_, _, id)| *id == c.id) else {
        return Vec::new();
    };
    let path = env.account(HYPRLAND);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Some(new) = follow(&text, variable, shipped, c.id) else {
        return Vec::new();
    };
    let kept = crate::generated::beside(&path, KEPT);
    if std::fs::write(&kept, &text).is_err() || crate::generated::replace(&path, &new).is_err() {
        return Vec::new();
    }
    // The key's command is read when the file is: without this, the old
    // one stays until the next login.
    if env.hyprland {
        let _ = env.run(&["hyprctl", "reload"]);
    }
    vec![format!(
        "{} now follows this choice ({} as it was).",
        if c.id == "browser" {
            "Super+B"
        } else {
            "Super+Return"
        },
        kept.display()
    )]
}

/// `hyprland.conf` with `variable = shipped` made `variable = alpymist open
/// category`, or `None` when no line says exactly that.
fn follow(conf: &str, variable: &str, shipped: &str, category: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(conf.len() + 16);
    for line in conf.split_inclusive('\n') {
        let is_shipped = line
            .trim()
            .split_once('=')
            .is_some_and(|(k, v)| k.trim() == variable && v.trim() == shipped);
        if is_shipped && !changed {
            let _ = write!(out, "{variable} = alpymist open {category}");
            if line.ends_with('\n') {
                out.push('\n');
            }
            changed = true;
        } else {
            out.push_str(line);
        }
    }
    changed.then_some(out)
}

fn set_in(
    env: &Env,
    places: &Places,
    here: &Environment,
    s: &Setting,
    value: Option<&Value>,
) -> Result<(), String> {
    let c = category(s.id).ok_or_else(|| format!("no category `{}`", s.id))?;
    let wanted = value.and_then(Value::as_text).unwrap_or_default();
    let apps = places.applications();
    let app = if wanted.is_empty() {
        None
    } else {
        Some(
            candidates(c, &apps, here)
                .into_iter()
                .chain(chosen(c, places, &apps))
                .find(|f| stem(&f.id) == wanted)
                .ok_or_else(|| {
                    format!(
                        "`{wanted}` is not installed, or does not open {}",
                        c.title.to_lowercase()
                    )
                })?,
        )
    };
    if c.types.is_empty() {
        return set_terminal(env, app.map(|f| f.id.as_str()));
    }

    let path = env.account(MIMEAPPS);
    let mut text = read(&path)?;
    for t in c.types {
        // A type the application does not open falls back to what would open
        // it otherwise, rather than to something that cannot.
        let id = app
            .filter(|f| defaults::handles(&f.entry, t))
            .map(|f| f.id.as_str());
        text = crate::ini::with_key(&text, GROUP, t, id);
    }
    // Nothing chosen and nothing else in it: no file, as before anything was.
    if text.trim() == format!("[{GROUP}]") {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| io_error(&path, &e))?;
        }
        return Ok(());
    }
    write(&path, &text)
}

/// Put the terminal first in the account's list, or with `None` empty it.
fn set_terminal(env: &Env, id: Option<&str>) -> Result<(), String> {
    let path = env.account(TERMINALS);
    let old = read(&path)?;
    let Some(id) = id else {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| io_error(&path, &e))?;
        }
        return Ok(());
    };
    let mut text = format!("{id}\n");
    for line in old.lines() {
        let named = line.trim().trim_start_matches('+');
        if named.split(':').next() != Some(id) {
            text.push_str(line);
            text.push('\n');
        }
    }
    write(&path, &text)
}

fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(io_error(path, &e)),
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_error(dir, &e))?;
    }
    std::fs::write(path, text).map_err(|e| io_error(path, &e))
}

/// The command lines that open `args` — files or URLs, or for the terminal a
/// command — with what a category's setting chose. An application that runs
/// in a terminal is given one.
///
/// # Errors
/// Not a category, or nothing installed opens it.
pub fn open(category_id: &str, args: &[String]) -> Result<Vec<Vec<String>>, String> {
    let places = Places::current();
    let c = category(category_id).ok_or_else(|| {
        let ids: Vec<&str> = CATEGORIES.iter().map(|c| c.id).collect();
        format!("`{category_id}` is not one of {}", ids.join(", "))
    })?;
    if c.types.is_empty() {
        return Ok(vec![defaults::terminal_argv(&places, args)]);
    }
    let apps = places.applications();
    let app = resolved(c, &places, &apps).ok_or_else(|| {
        format!(
            "nothing installed opens {}; choose one in Settings › Default applications",
            c.title.to_lowercase()
        )
    })?;
    let exec = app
        .entry
        .raw("Exec")
        .map(desktop_entry::unescape)
        .unwrap_or_default();
    let runs = desktop_entry::exec_with(&exec, args);
    if app.entry.wants_terminal() {
        let prefix = defaults::terminal_prefix(&places);
        return Ok(runs
            .into_iter()
            .map(|argv| prefix.iter().cloned().chain(argv).collect())
            .collect());
    }
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::{get_in, set_in, settings_for};
    use crate::env::Env;
    use crate::model::{Kind, Value};
    use alpymist_core::defaults::Places;
    use alpymist_core::desktop_entry::Environment;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn here() -> Environment {
        Environment {
            desktops: vec!["Hyprland".into()],
            locales: Vec::new(),
            path: Vec::new(),
        }
    }

    /// The system's browser, another browser, and two terminals.
    fn setup(name: &str) -> (PathBuf, Env, Places) {
        let d =
            std::env::temp_dir().join(format!("alpymist-default-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, false, &RAN);
        let apps = d.join("share/applications");
        put(
            &apps.join("librewolf.desktop"),
            "[Desktop Entry]\nType=Application\nName=LibreWolf\nExec=librewolf %u\n\
             MimeType=text/html;application/xhtml+xml;x-scheme-handler/http;\
             x-scheme-handler/https;application/pdf;\n",
        );
        put(
            &apps.join("org.example.Browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser %U\n\
             MimeType=text/html;x-scheme-handler/https;x-scheme-handler/http;\n",
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
        put(
            &env.system("etc/xdg/mimeapps.list"),
            "[Default Applications]\ntext/html=librewolf.desktop\n\
             x-scheme-handler/https=librewolf.desktop\n",
        );
        let places = Places {
            config_home: env.config.clone(),
            config_dirs: vec![env.system("etc/xdg")],
            data_dirs: vec![d.join("share")],
            desktops: vec!["Hyprland".into()],
        };
        (d, env, places)
    }

    fn values(setting: &crate::model::Setting) -> Vec<(String, String)> {
        let Kind::Choice(choices) = &setting.kind else {
            panic!("a category is a choice")
        };
        choices
            .iter()
            .map(|c| (c.value.clone(), c.label.clone()))
            .collect()
    }

    #[test]
    fn each_category_offers_what_opens_it_and_says_what_automatic_is() {
        let (d, _env, places) = setup("offer");
        let settings = settings_for(&places, &here());
        let find = |id: &str| settings.iter().find(|s| s.id == id).unwrap();
        assert_eq!(
            values(find("default.browser")),
            [
                (String::new(), "Automatic (LibreWolf)".into()),
                ("org.example.Browser".into(), "Browser".into()),
                ("librewolf".into(), "LibreWolf".into()),
            ]
        );
        assert_eq!(values(find("default.terminal"))[0].1, "Automatic (Foot)");
        assert_eq!(values(find("default.terminal")).len(), 3);
        let images = find("default.images");
        assert_eq!(
            values(images),
            [(String::new(), "Automatic (nothing installed)".into())]
        );
        assert!(images.description.contains("the store"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn choosing_a_browser_sets_its_types_and_keeps_the_rest_of_the_file() {
        let (d, env, places) = setup("choose");
        let settings = settings_for(&places, &here());
        let browser = settings.iter().find(|s| s.id == "default.browser").unwrap();
        assert_eq!(get_in(&places, browser), Ok(Value::Text(String::new())));

        let list = env.account("mimeapps.list");
        put(
            &list,
            "[Added Associations]\nimage/png=someone.desktop;\n\
             [Default Applications]\napplication/pdf=librewolf.desktop\n",
        );
        let pick = Value::Text("org.example.Browser".into());
        set_in(&env, &places, &here(), browser, Some(&pick)).unwrap();
        let text = std::fs::read_to_string(&list).unwrap();
        assert!(text.starts_with("[Added Associations]\nimage/png=someone.desktop;\n"));
        for t in [
            "x-scheme-handler/https",
            "x-scheme-handler/http",
            "text/html",
        ] {
            assert!(
                text.contains(&format!("{t}=org.example.Browser.desktop\n")),
                "{t}"
            );
        }
        // It does not open XHTML, so that is left to whatever does.
        assert!(!text.contains("application/xhtml+xml"));
        assert!(text.contains("application/pdf=librewolf.desktop\n"));
        assert_eq!(get_in(&places, browser), Ok(pick.clone()));

        set_in(&env, &places, &here(), browser, None).unwrap();
        let text = std::fs::read_to_string(&list).unwrap();
        assert!(
            text.contains("[Default Applications]\n"),
            "other keys keep it"
        );
        assert!(!text.contains("org.example.Browser"), "{text}");
        assert!(text.contains("application/pdf=librewolf.desktop\n"));
        assert_eq!(get_in(&places, browser), Ok(Value::Text(String::new())));

        // A list Settings made, taken back to Automatic, is not left behind
        // as an empty heading.
        std::fs::remove_file(&list).unwrap();
        set_in(&env, &places, &here(), browser, Some(&pick)).unwrap();
        set_in(&env, &places, &here(), browser, None).unwrap();
        assert!(!list.exists());

        let nothing = Value::Text("gimp".into());
        assert!(set_in(&env, &places, &here(), browser, Some(&nothing)).is_err());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn an_older_accounts_keys_follow_its_first_change() {
        let (d, env, _places) = setup("keys");
        let conf = env.account("hypr/hyprland.conf");
        let old = "$terminal = foot\n$browser = librewolf\nbind = SUPER, B, exec, $browser\n";
        put(&conf, old);
        let browser = super::category("browser").unwrap();
        let notes = super::take_over(&env, browser);
        assert!(notes[0].starts_with("Super+B now follows"), "{notes:?}");
        assert_eq!(
            std::fs::read_to_string(&conf).unwrap(),
            "$terminal = foot\n$browser = alpymist open browser\nbind = SUPER, B, exec, $browser\n"
        );
        assert_eq!(
            std::fs::read_to_string(env.account("hypr/hyprland.conf.bak-defaults")).unwrap(),
            old
        );
        assert!(RAN.lock().unwrap().contains(&"hyprctl reload".to_owned()));
        // Once is enough, and another category leaves the file be.
        assert!(super::take_over(&env, browser).is_empty());
        assert!(super::take_over(&env, super::category("pdf").unwrap()).is_empty());

        // A browser of the account's own choosing is its own.
        put(&conf, "$browser = firefox --private\n");
        assert!(super::take_over(&env, browser).is_empty());
        assert_eq!(
            super::follow("$terminal=foot\n", "$terminal", "foot", "terminal").as_deref(),
            Some("$terminal = alpymist open terminal\n")
        );
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn choosing_a_terminal_puts_it_first_in_the_list() {
        let (d, env, places) = setup("terminal");
        let settings = settings_for(&places, &here());
        let terminal = settings
            .iter()
            .find(|s| s.id == "default.terminal")
            .unwrap();
        let list = env.account("xdg-terminals.list");
        put(&list, "# mine\nfoot.desktop\nkitty.desktop\n");
        set_in(
            &env,
            &places,
            &here(),
            terminal,
            Some(&Value::Text("kitty".into())),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&list).unwrap(),
            "kitty.desktop\n# mine\nfoot.desktop\n"
        );
        assert_eq!(get_in(&places, terminal), Ok(Value::Text("kitty".into())));
        set_in(&env, &places, &here(), terminal, None).unwrap();
        assert!(!list.exists());
        std::fs::remove_dir_all(d).ok();
    }
}

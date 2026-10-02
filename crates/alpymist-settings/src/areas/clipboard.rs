//! Clipboard: whether what is copied is kept, for how long, and how much.
//!
//! The history is off until turned on here. It keeps whatever is copied —
//! passwords and keys too, unless the program that copied them marks them
//! secret, as password managers do — so it is something to choose knowing
//! that. It is kept in memory, and forgotten at the end of the session and,
//! unless told otherwise, when the screen locks; only if asked is it kept
//! across logins, in a file only the account can read. The values are
//! `alpymist-clipboard`'s file, and the daemon that keeps the history reads
//! them (ADR 0007).
//!
//! Super+C, Super+V and Super+Shift+V are keys in the account's own
//! `hyprland.conf`. An account made before them has Super+V floating a
//! window, so the first change here gives it the new keys, as the theme does
//! with its lines: floating moves to Super+Shift+F, the file is kept as it
//! was beside it, and Hyprland is reloaded. Keys of the account's own are left
//! alone.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_clipboard::config::{Config, FILE, MAX_SIZE, MIN_SIZE};
use std::fmt::Write as _;

/// The account's Hyprland configuration.
const HYPRLAND: &str = "hypr/hyprland.conf";
/// What `hyprland.conf` is kept beside it as, before its keys change.
pub const KEPT: &str = ".bak-clipboard";
/// The floating key every account was given before the clipboard's.
const OLD_FLOAT: &str = "bind = SUPER, V, togglefloating";
/// The screenshot key every account was given, saving to a file only.
const OLD_PRINT: &str = r#"bind = , Print, exec, sh -c 'grim -g "$(slurp)" "$HOME/screenshot-$(date +%Y%m%d-%H%M%S).png"'"#;
/// The same, copying the picture as well.
const NEW_PRINT: &str = r#"bind = , Print, exec, sh -c 'f="$HOME/screenshot-$(date +%Y%m%d-%H%M%S).png"; grim -g "$(slurp)" "$f" && wl-copy --type image/png < "$f"'"#;
/// The keys, in place of the old floating one: each chord, and its line.
const KEYS: [(&str, &str); 4] = [
    ("SUPER|C", COPY),
    ("SUPER|V", "bind = SUPER, V, exec, alpymist clipboard paste"),
    (
        "SHIFT SUPER|V",
        "bind = SUPER SHIFT, V, exec, alpymist-menu clipboard",
    ),
    ("SHIFT SUPER|F", "bind = SUPER SHIFT, F, togglefloating"),
];

/// Super+C's line, which Super+X's goes after.
const COPY: &str = "bind = SUPER, C, exec, alpymist clipboard copy";
/// Super+X: cut, given where it is free.
const CUT: (&str, &str) = ("SUPER|X", "bind = SUPER, X, exec, alpymist clipboard cut");

/// The settings.
pub fn settings() -> Vec<Setting> {
    let defaults = Config::default();
    vec![
        Setting {
            id: "clipboard.history",
            title: "Keep a history",
            description: "Keep what you copy, to paste again from Super+Shift+V. Passwords \
                          and keys you copy are kept too, unless a password manager marks \
                          them secret.",
            keywords: &["clipboard history", "copy", "paste", "super+shift+v"],
            kind: Kind::Switch,
            default: Value::Bool(defaults.history),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: "clipboard.remember",
            title: "Keep it after logging out",
            description: "Keep the history in a file only you can read, rather than in \
                          memory only.",
            keywords: &["persist", "save", "across logins"],
            kind: Kind::Switch,
            default: Value::Bool(defaults.remember),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: "clipboard.size",
            title: "Entries kept",
            description: "The most the history keeps; the oldest go first. Pinned entries \
                          are not counted, and never go.",
            keywords: &["limit", "length", "how many"],
            kind: Kind::Number {
                min: i64::from(MIN_SIZE),
                max: i64::from(MAX_SIZE),
                step: 10,
                unit: "",
            },
            default: Value::Number(i64::from(defaults.size)),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: "clipboard.clear-on-lock",
            title: "Forget it when the screen locks",
            description: "Forget all but pinned entries whenever the screen locks.",
            keywords: &["lock", "privacy", "security"],
            kind: Kind::Switch,
            default: Value::Bool(defaults.clear_on_lock),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: "clipboard.clear",
            title: "Clear the history",
            description: "Forget everything but pinned entries, now.",
            keywords: &["delete", "erase", "empty"],
            kind: Kind::Action { label: "Clear" },
            default: Value::Text(String::new()),
            scope: Scope::Account,
            applies: Applies::Now,
        },
    ]
}

fn load(env: &Env) -> Config {
    Config::load_from(&env.account(FILE))
}

/// A setting's value.
pub fn get(env: &Env, s: &Setting) -> Value {
    let c = load(env);
    match s.id {
        "clipboard.history" => Value::Bool(c.history),
        "clipboard.remember" => Value::Bool(c.remember),
        "clipboard.size" => Value::Number(i64::from(c.size)),
        "clipboard.clear-on-lock" => Value::Bool(c.clear_on_lock),
        _ => Value::Text(String::new()),
    }
}

/// Change one, or clear the history. Says what else it did.
///
/// # Errors
/// The file could not be written.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    if s.id == "clipboard.clear" {
        return Ok(Vec::new());
    }
    let mut c = load(env);
    let value = value.unwrap_or(&s.default);
    let on = || value.as_bool().unwrap_or(false);
    match s.id {
        "clipboard.history" => c.history = on(),
        "clipboard.remember" => c.remember = on(),
        "clipboard.clear-on-lock" => c.clear_on_lock = on(),
        "clipboard.size" => {
            c.size = value
                .as_number()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(c.size);
        }
        other => return Err(format!("no setting {other}")),
    }
    c.save_to(&env.account(FILE))?;
    Ok(take_over(env))
}

/// Tell the history: start or stop it, have it read its settings again, or
/// clear it.
///
/// # Errors
/// The history could not be told.
pub fn live(env: &Env, s: &Setting) -> Result<(), String> {
    let what = match s.id {
        "clipboard.history" => "start",
        "clipboard.clear" => "clear",
        _ => "reload",
    };
    env.run(&["alpymist", "clipboard", what]).map(drop)
}

/// Give an older account's `hyprland.conf` the clipboard's keys, if it
/// still has the ones every account started with. Says what it did.
fn take_over(env: &Env) -> Vec<String> {
    let path = env.account(HYPRLAND);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let (new, what) = match with_keys(&text) {
        Some(new) => (
            new,
            "Super+C and Super+V now copy and paste, Super+Shift+V opens the history, \
             and floating a window moved to Super+Shift+F",
        ),
        None => match with_cut(&text) {
            Some(new) => (new, "Super+X now cuts"),
            None => return Vec::new(),
        },
    };
    let kept = crate::generated::beside(&path, KEPT);
    if std::fs::write(&kept, &text).is_err() || crate::generated::replace(&path, &new).is_err() {
        return Vec::new();
    }
    if env.hyprland {
        let _ = env.run(&["hyprctl", "reload"]);
    }
    vec![format!("{what} ({} as it was).", kept.display())]
}

/// A bind's modifiers and key, as `SHIFT SUPER|V`: the same whatever order
/// and case they were written in.
pub(super) fn chord(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("bind")?;
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let rest = rest.trim_start().strip_prefix('=')?;
    let mut fields = rest.split(',');
    let mut mods: Vec<String> = fields
        .next()?
        .split_whitespace()
        .map(str::to_uppercase)
        .collect();
    mods.sort();
    let key = fields.next()?.trim().to_uppercase();
    Some(format!("{}|{key}", mods.join(" ")))
}

/// Every chord the file binds, but the line at `except`.
fn taken(lines: &[&str], except: Option<usize>) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != except)
        .filter_map(|(_, l)| chord(l))
        .collect()
}

/// `hyprland.conf` with the clipboard's keys where the old floating key was,
/// Super+X among them where it is free, and the screenshot key copying too;
/// `None` when the floating key is not as shipped, or a key of the account's
/// own is already on one of the four the clipboard needs.
fn with_keys(conf: &str) -> Option<String> {
    let lines: Vec<&str> = conf.lines().collect();
    let at = lines.iter().position(|l| l.trim() == OLD_FLOAT)?;
    let taken = taken(&lines, Some(at));
    if KEYS.iter().any(|(c, _)| taken.iter().any(|t| t == c)) {
        return None;
    }
    let cut_free = !taken.iter().any(|t| t == CUT.0);
    let mut out = String::with_capacity(conf.len() + 240);
    for (i, line) in lines.iter().enumerate() {
        if i == at {
            for (_, key) in KEYS {
                let _ = writeln!(out, "{key}");
                if key == COPY && cut_free {
                    let _ = writeln!(out, "{}", CUT.1);
                }
            }
        } else if line.trim() == OLD_PRINT {
            let _ = writeln!(out, "{NEW_PRINT}");
        } else {
            let _ = writeln!(out, "{line}");
        }
    }
    Some(out)
}

/// `hyprland.conf` with Super+X after Super+C, for an account given the
/// clipboard's keys before there was a Super+X; `None` when Super+C is not
/// the clipboard's, or Super+X is already bound.
fn with_cut(conf: &str) -> Option<String> {
    let lines: Vec<&str> = conf.lines().collect();
    let at = lines.iter().position(|l| l.trim() == COPY)?;
    if taken(&lines, None).iter().any(|t| t == CUT.0) {
        return None;
    }
    let mut out = String::with_capacity(conf.len() + 60);
    for (i, line) in lines.iter().enumerate() {
        let _ = writeln!(out, "{line}");
        if i == at {
            let _ = writeln!(out, "{}", CUT.1);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        COPY, CUT, NEW_PRINT, OLD_FLOAT, OLD_PRINT, chord, get, set, settings, with_cut, with_keys,
    };
    use crate::env::Env;
    use crate::model::Value;
    use std::sync::Mutex;

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    #[test]
    fn a_chord_is_the_same_however_written() {
        assert_eq!(
            chord("bind = SUPER SHIFT, V, exec, x").as_deref(),
            Some("SHIFT SUPER|V")
        );
        assert_eq!(
            chord("binde=shift super,v,exec,x").as_deref(),
            Some("SHIFT SUPER|V")
        );
        assert_eq!(chord("# bind = SUPER, V"), None);
    }

    #[test]
    fn an_older_accounts_keys_are_given_once_and_its_own_kept() {
        let old = format!("$terminal = foot\n{OLD_FLOAT}\n{OLD_PRINT}\nbind = SUPER, L, exec, x\n");
        let new = with_keys(&old).unwrap();
        assert!(new.contains("bind = SUPER, C, exec, alpymist clipboard copy\n"));
        assert!(new.contains("bind = SUPER, X, exec, alpymist clipboard cut\n"));
        assert!(new.contains("bind = SUPER, V, exec, alpymist clipboard paste\n"));
        assert!(new.contains("bind = SUPER SHIFT, F, togglefloating\n"));
        assert!(new.contains(NEW_PRINT));
        assert!(!new.contains(OLD_FLOAT));
        assert!(new.ends_with("bind = SUPER, L, exec, x\n"));
        assert_eq!(with_keys(&new), None, "once");
        // Super+X the account's own: the rest is given, and that is kept.
        let own_x = format!("{OLD_FLOAT}\nbind = SUPER, X, exec, mine\n");
        let new = with_keys(&own_x).unwrap();
        assert!(!new.contains(CUT.1) && new.contains("exec, mine"));
        // Super+C the account's own: nothing is touched.
        let own = format!("{OLD_FLOAT}\nbind = SUPER, C, exec, mine\n");
        assert_eq!(with_keys(&own), None);
    }

    #[test]
    fn the_history_is_off_until_turned_on() {
        let d = std::env::temp_dir().join(format!("alpymist-clip-settings-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, false, &RAN);
        let all = settings();
        let history = &all[0];
        assert_eq!(get(&env, history), Value::Bool(false));
        let conf = env.account("hypr/hyprland.conf");
        std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
        std::fs::write(&conf, format!("{OLD_FLOAT}\n")).unwrap();
        let notes = set(&env, history, Some(&Value::Bool(true))).unwrap();
        assert_eq!(get(&env, history), Value::Bool(true));
        assert!(notes[0].starts_with("Super+C and Super+V"), "{notes:?}");
        assert!(RAN.lock().unwrap().contains(&"hyprctl reload".to_owned()));
        let size = &all[2];
        set(&env, size, Some(&Value::Number(120))).unwrap();
        assert_eq!(get(&env, size), Value::Number(120));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn an_account_given_the_keys_before_super_x_gets_it_once() {
        let before = format!("{COPY}\nbind = SUPER, V, exec, alpymist clipboard paste\n");
        let new = with_cut(&before).unwrap();
        assert_eq!(
            new,
            format!(
                "{COPY}\n{}\nbind = SUPER, V, exec, alpymist clipboard paste\n",
                CUT.1
            )
        );
        assert_eq!(with_cut(&new), None, "once");
        assert_eq!(with_cut("bind = SUPER, C, exec, mine\n"), None);
        assert_eq!(
            with_cut(&format!("{COPY}\nbind = SUPER, X, killactive\n")),
            None
        );
    }
}

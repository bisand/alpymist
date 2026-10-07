//! Displays: how the screens are arranged is its own page in the settings
//! app, drawn from `alpymist-displays`, and has no value to keep here. What
//! are settings are what a closed lid does to the laptop's panel while
//! another screen is on, and whether each screen has its own workspaces, kept
//! with the layouts in `~/.config/alpymist/displays.toml` (ADR 0015).
//!
//! Super+1 to Super+9 go through `alpymist displays workspace`, which knows
//! which screen has the focus. An account made before that has Hyprland's
//! own `workspace` binds in its `hyprland.conf`; changing either setting takes
//! those lines over, as the theme does (ADR 0007), and keeps the file as it
//! was beside it.
//!
//! Super+Shift and an arrow move the window that way, among the others and
//! from the edge of a screen to the next. An account made before those keys
//! is given them the same way, after its Super+arrow lines, unless it has put
//! something of its own on one of them. One given them when they went only
//! to another screen, and so did nothing up or down beside a screen with
//! none above or below it, has them put right before each session.
//!
//! Super+Tab is the overview of a screen's workspaces, and is given likewise,
//! after the Super+number lines, to an account with nothing on that key.

use super::clipboard::chord;
use super::lock::{NEW_KEY as NEW_LOCK, OLD_KEY as OLD_LOCK};
use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_displays::layout::{FILE, Layouts};
use std::fmt::Write as _;

/// The lid's setting.
pub const LID: &str = "displays.lid";
/// Each screen its own workspaces.
pub const WORKSPACES: &str = "displays.workspaces";

/// The account's Hyprland configuration.
const HYPRLAND: &str = "hypr/hyprland.conf";
/// Where it is kept as it was before the workspace keys were taken over.
pub const KEPT: &str = ".bak-workspaces";
/// The last of the Super+arrow lines every account started with, which the
/// keys that take a window to another screen go after.
const FOCUS_DOWN: &str = "bind = SUPER, down, movefocus, d";
/// Super+Shift and an arrow: each chord, and its line.
const SCREEN_KEYS: [(&str, &str); 4] = [
    (
        "SHIFT SUPER|LEFT",
        "bind = SUPER SHIFT, left, movewindow, l",
    ),
    (
        "SHIFT SUPER|RIGHT",
        "bind = SUPER SHIFT, right, movewindow, r",
    ),
    ("SHIFT SUPER|UP", "bind = SUPER SHIFT, up, movewindow, u"),
    (
        "SHIFT SUPER|DOWN",
        "bind = SUPER SHIFT, down, movewindow, d",
    ),
];
/// The same keys as they were first given, taking the window only to another
/// screen, in the same order.
const SCREEN_ONLY: [&str; 4] = [
    "bind = SUPER SHIFT, left, movewindow, mon:l",
    "bind = SUPER SHIFT, right, movewindow, mon:r",
    "bind = SUPER SHIFT, up, movewindow, mon:u",
    "bind = SUPER SHIFT, down, movewindow, mon:d",
];
/// Where `hyprland.conf` is kept as it was before those are put right.
const KEPT_MOVE: &str = ".bak-move";

/// The last of the Super+number lines, which the overview's key goes after.
const MOVE_NINE: &str = "bind = SUPER SHIFT, 9, exec, alpymist displays move 9";
/// The same line as every account started with, before the numbers went
/// through `alpymist displays`: an account that never changed a Displays
/// setting still has it.
const OLD_MOVE_NINE: &str = "bind = SUPER SHIFT, 9, movetoworkspace, 9";
/// Where `hyprland.conf` is kept as it was before a login gave it the
/// overview's key; and, by being there, what says it was given once.
const KEPT_OVERVIEW: &str = ".bak-overview";
/// Super+Tab: its chord, and its line.
const OVERVIEW_KEY: (&str, &str) = (
    "SUPER|TAB",
    "bind = SUPER, Tab, exec, alpymist displays overview",
);

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![
        Setting {
            id: WORKSPACES,
            title: "Each screen has its own workspaces",
            description: "Super+1 to Super+9 switch between the workspaces of the screen the \
                          pointer is on. Off, workspaces 1 to 9 are shared by every screen.",
            keywords: &["workspace", "virtual desktop", "spaces", "super", "monitor"],
            kind: Kind::Switch,
            default: Value::Bool(true),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: LID,
            title: "Turn the laptop's screen off when the lid closes",
            description: "While another screen is on, closing the lid turns the built-in \
                      screen off and moves what was on it to the others.",
            keywords: &["lid", "clamshell", "dock", "laptop", "built-in", "external"],
            kind: Kind::Switch,
            default: Value::Bool(true),
            scope: Scope::Account,
            applies: Applies::Now,
        },
    ]
}

/// What a setting is.
pub fn get(env: &Env, s: &Setting) -> Value {
    let all = Layouts::load(&env.account(FILE));
    Value::Bool(if s.id == WORKSPACES {
        all.per_screen
    } else {
        all.lid_off
    })
}

/// Change one, keeping every layout as it is, and give an older account the
/// keys that follow it. Says what else it did.
///
/// # Errors
/// The file could not be written.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    let path = env.account(FILE);
    let mut all = Layouts::load(&path);
    let on = value.and_then(Value::as_bool).unwrap_or(true);
    if s.id == WORKSPACES {
        all.per_screen = on;
    } else {
        all.lid_off = on;
    }
    all.save(&path)?;
    Ok(take_over(env))
}

/// Put the screens and their workspaces as the settings now have them.
///
/// # Errors
/// `alpymist displays apply` failed.
pub fn live(env: &Env) -> Result<(), String> {
    env.run(&["alpymist", "displays", "apply"]).map(drop)
}

/// Give an older account's `hyprland.conf` the keys that go through
/// `alpymist displays`, where it still has Hyprland's own as every account
/// started with, and the ones that take a window to another screen and show
/// the overview, where it has none. Says what it did.
fn take_over(env: &Env) -> Vec<String> {
    let path = env.account(HYPRLAND);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut what = Vec::new();
    let mut new = text.clone();
    if let Some(keys) = with_keys(&new) {
        new = keys;
        what.push(
            "Super+1 to Super+9 now follow the screen the pointer is on, and Super+L \
             locks with Alpymist's lock",
        );
    }
    if let Some(keys) = with_screen_keys(&new) {
        new = keys;
        what.push("Super+Shift and an arrow now move the window that way");
    }
    if let Some(keys) = with_overview_key(&new) {
        new = keys;
        what.push("Super+Tab now shows every workspace of the screen the pointer is on");
    }
    if what.is_empty() {
        return Vec::new();
    }
    let kept = crate::generated::beside(&path, KEPT);
    if std::fs::write(&kept, &text).is_err() || crate::generated::replace(&path, &new).is_err() {
        return Vec::new();
    }
    if env.hyprland {
        let _ = env.run(&["hyprctl", "reload"]);
    }
    vec![format!(
        "{} ({} as it was).",
        what.join(". "),
        kept.display()
    )]
}

/// `hyprland.conf` with each of the Super+number lines every account started
/// with going through `alpymist displays`, and Super+L locking with
/// Alpymist's lock where it still runs swaylock, which is no longer
/// installed; `None` when there are none of those left, as there are not once
/// they have been taken over or changed by hand.
fn with_keys(conf: &str) -> Option<String> {
    let mut out = String::with_capacity(conf.len() + 400);
    let mut changed = false;
    for line in conf.lines() {
        let trimmed = line.trim();
        let new = (trimmed == OLD_LOCK)
            .then(|| NEW_LOCK.to_owned())
            .or_else(|| {
                (1..=9).find_map(|n| {
                    if trimmed == format!("bind = SUPER, {n}, workspace, {n}") {
                        Some(format!(
                            "bind = SUPER, {n}, exec, alpymist displays workspace {n}"
                        ))
                    } else if trimmed == format!("bind = SUPER SHIFT, {n}, movetoworkspace, {n}") {
                        Some(format!(
                            "bind = SUPER SHIFT, {n}, exec, alpymist displays move {n}"
                        ))
                    } else {
                        None
                    }
                })
            });
        changed |= new.is_some();
        let _ = writeln!(out, "{}", new.as_deref().unwrap_or(line));
    }
    changed.then_some(out)
}

/// `hyprland.conf` with Super+Shift and an arrow moving the window that way,
/// after the Super+arrow lines every account started
/// with; `None` when those are not as shipped, or any of the four is already
/// bound, by the account or by having been given them before.
fn with_screen_keys(conf: &str) -> Option<String> {
    let lines: Vec<&str> = conf.lines().collect();
    let at = lines.iter().position(|l| l.trim() == FOCUS_DOWN)?;
    if lines
        .iter()
        .filter_map(|l| chord(l))
        .any(|c| SCREEN_KEYS.iter().any(|(k, _)| *k == c))
    {
        return None;
    }
    let mut out = String::with_capacity(conf.len() + 200);
    for (i, line) in lines.iter().enumerate() {
        let _ = writeln!(out, "{line}");
        if i == at {
            for (_, key) in SCREEN_KEYS {
                let _ = writeln!(out, "{key}");
            }
        }
    }
    Some(out)
}

/// `hyprland.conf` with Super+Tab showing the overview, after the Super+number
/// lines; `None` when the last of those is not as shipped, now or at first,
/// or Super+Tab is already bound, by the account or by having been given it
/// before.
fn with_overview_key(conf: &str) -> Option<String> {
    let lines: Vec<&str> = conf.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim() == MOVE_NINE || l.trim() == OLD_MOVE_NINE)?;
    if lines
        .iter()
        .filter_map(|l| chord(l))
        .any(|c| c == OVERVIEW_KEY.0)
    {
        return None;
    }
    let mut out = String::with_capacity(conf.len() + 60);
    for (i, line) in lines.iter().enumerate() {
        let _ = writeln!(out, "{line}");
        if i == at {
            let _ = writeln!(out, "{}", OVERVIEW_KEY.1);
        }
    }
    Some(out)
}

/// What a login, and an upgrade for each account, puts right in the
/// account's `hyprland.conf`, and tells a running Hyprland.
///
/// Super+Shift and an arrow move the window that way, where the file still
/// takes it only to another screen, as Alpymist first wrote: each of the
/// four is put right by itself, and one written any other way is somebody's
/// choice.
///
/// And Super+Tab shows the overview, where the file binds that key to
/// nothing: once. An account made before the key had it only after a
/// Displays setting was changed, and most never change one. The file as it
/// was is kept beside it, and that copy being there is what says the key was
/// given: an account that then takes the line out is not given it again.
///
/// # Errors
/// The file could not be written.
pub fn prepare_session(env: &Env) -> Result<(), String> {
    let path = env.account(HYPRLAND);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let keep = |to: &std::path::Path| {
        std::fs::write(to, &text).map_err(|e| format!("{}: {e}", to.display()))
    };
    let mut new = None;
    if let Some(right) = with_moves(&text) {
        keep(&crate::generated::beside(&path, KEPT_MOVE))?;
        new = Some(right);
    }
    let given = {
        let mut name = path.as_os_str().to_owned();
        name.push(KEPT_OVERVIEW);
        std::path::PathBuf::from(name)
    };
    if !given.exists()
        && let Some(keyed) = with_overview_key(new.as_deref().unwrap_or(&text))
    {
        keep(&given)?;
        new = Some(keyed);
    }
    let Some(new) = new else {
        return Ok(());
    };
    crate::generated::replace(&path, &new)?;
    // As for the lock's key: at an upgrade nothing says which Hyprland is the
    // account's, and before a login there is none to tell.
    let _ = env.run(&["hyprctl", "-i", "0", "reload"]);
    Ok(())
}

/// `hyprland.conf` with each of the keys that took the window only to another
/// screen now moving it that way; `None` when there is none of them.
fn with_moves(conf: &str) -> Option<String> {
    let mut out = String::with_capacity(conf.len());
    let mut changed = false;
    for line in conf.lines() {
        let new = SCREEN_ONLY
            .iter()
            .position(|old| line.trim() == *old)
            .map(|i| SCREEN_KEYS[i].1);
        changed |= new.is_some();
        let _ = writeln!(out, "{}", new.unwrap_or(line));
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        FOCUS_DOWN, HYPRLAND, LID, MOVE_NINE, OVERVIEW_KEY, SCREEN_ONLY, WORKSPACES, get,
        prepare_session, set, settings, with_keys, with_overview_key, with_screen_keys,
    };
    use crate::env::Env;
    use crate::model::Value;
    use std::sync::Mutex;

    #[test]
    fn the_lid_is_kept_beside_the_layouts() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let dir =
            std::env::temp_dir().join(format!("alpymist-displays-area-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        let all = settings();
        let lid = all.iter().find(|s| s.id == LID).unwrap();
        let spaces = all.iter().find(|s| s.id == WORKSPACES).unwrap();
        assert_eq!(get(&env, lid), Value::Bool(true), "on until turned off");
        assert_eq!(get(&env, spaces), Value::Bool(true), "on until turned off");
        set(&env, lid, Some(&Value::Bool(false))).unwrap();
        assert_eq!(get(&env, lid), Value::Bool(false));
        assert_eq!(
            get(&env, spaces),
            Value::Bool(true),
            "the other is as it was"
        );
        let text = std::fs::read_to_string(env.account(super::FILE)).unwrap();
        assert!(text.contains("lid-turns-panel-off = false"), "{text}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_keys_every_account_started_with_are_taken_over_and_no_others() {
        let conf = "bind = SUPER, 1, workspace, 1\nbind = SUPER SHIFT, 1, movetoworkspace, 1\n\
                    bind = SUPER, 2, workspace, 7\nbind = SUPER, Q, killactive\n";
        let new = with_keys(conf).unwrap();
        assert_eq!(
            new,
            "bind = SUPER, 1, exec, alpymist displays workspace 1\n\
             bind = SUPER SHIFT, 1, exec, alpymist displays move 1\n\
             bind = SUPER, 2, workspace, 7\nbind = SUPER, Q, killactive\n"
        );
        assert_eq!(with_keys(&new), None, "once is enough");
        assert_eq!(
            with_keys("bind = SUPER, L, exec, swaylock -f -c 0b121e\n").as_deref(),
            Some("bind = SUPER, L, exec, alpymist-lock\n"),
            "swaylock is gone"
        );
    }

    #[test]
    fn an_older_account_is_given_the_screen_keys_once_and_its_own_kept() {
        let old = format!("{FOCUS_DOWN}\nbind = SUPER, Q, killactive\n");
        let new = with_screen_keys(&old).unwrap();
        assert_eq!(
            new,
            format!(
                "{FOCUS_DOWN}\n\
                 bind = SUPER SHIFT, left, movewindow, l\n\
                 bind = SUPER SHIFT, right, movewindow, r\n\
                 bind = SUPER SHIFT, up, movewindow, u\n\
                 bind = SUPER SHIFT, down, movewindow, d\n\
                 bind = SUPER, Q, killactive\n"
            )
        );
        assert_eq!(with_screen_keys(&new), None, "once is enough");
        // One of the four the account's own: nothing is touched.
        let own = format!("{FOCUS_DOWN}\nbind = SHIFT SUPER, Left, exec, mine\n");
        assert_eq!(with_screen_keys(&own), None);
        // Its Super+arrow lines changed by hand: there is nowhere to put them.
        assert_eq!(with_screen_keys("bind = SUPER, down, movefocus, u\n"), None);
    }

    #[test]
    fn keys_that_only_went_to_another_screen_move_the_window_that_way() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let dir =
            std::env::temp_dir().join(format!("alpymist-displays-move-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        std::fs::create_dir_all(env.account(HYPRLAND).parent().unwrap()).unwrap();
        // Three as Alpymist wrote them, and one somebody changed.
        let conf = format!(
            "{FOCUS_DOWN}\n{}\n{}\n  {}\nbind = SUPER SHIFT, down, movewindow, mon:DP-1\n",
            SCREEN_ONLY[0], SCREEN_ONLY[1], SCREEN_ONLY[2]
        );
        std::fs::write(env.account(HYPRLAND), &conf).unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(
            std::fs::read_to_string(env.account(HYPRLAND)).unwrap(),
            format!(
                "{FOCUS_DOWN}\n\
                 bind = SUPER SHIFT, left, movewindow, l\n\
                 bind = SUPER SHIFT, right, movewindow, r\n\
                 bind = SUPER SHIFT, up, movewindow, u\n\
                 bind = SUPER SHIFT, down, movewindow, mon:DP-1\n"
            )
        );
        let kept = format!("{}.bak-move", env.account(HYPRLAND).display());
        assert_eq!(std::fs::read_to_string(kept).unwrap(), conf);
        // Once is enough, and a running Hyprland is told the once.
        prepare_session(&env).unwrap();
        assert_eq!(*RAN.lock().unwrap(), ["hyprctl -i 0 reload"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_older_account_is_given_the_overview_key_once_and_its_own_kept() {
        let old = format!("{MOVE_NINE}\nbindm = SUPER, mouse:272, movewindow\n");
        let new = with_overview_key(&old).unwrap();
        assert_eq!(
            new,
            format!(
                "{MOVE_NINE}\n{}\nbindm = SUPER, mouse:272, movewindow\n",
                OVERVIEW_KEY.1
            )
        );
        assert_eq!(with_overview_key(&new), None, "once is enough");
        // Super+Tab the account's own: nothing is touched.
        let own = format!("{MOVE_NINE}\nbind = SUPER, TAB, cyclenext\n");
        assert_eq!(with_overview_key(&own), None);
        // Its Super+number lines as every account first had them: after those.
        let first = format!(
            "{}\nbindm = SUPER, mouse:272, movewindow\n",
            super::OLD_MOVE_NINE
        );
        assert_eq!(
            with_overview_key(&first).unwrap(),
            format!(
                "{}\n{}\nbindm = SUPER, mouse:272, movewindow\n",
                super::OLD_MOVE_NINE,
                OVERVIEW_KEY.1
            )
        );
        // Changed by hand: there is nowhere to put it.
        assert_eq!(with_overview_key("bind = SUPER, Q, killactive\n"), None);
    }

    #[test]
    fn a_login_gives_an_older_account_the_overview_key_and_only_once() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let dir =
            std::env::temp_dir().join(format!("alpymist-displays-tab-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        let path = env.account(HYPRLAND);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conf = format!("{}\nbind = SUPER, W, killactive\n", super::OLD_MOVE_NINE);
        std::fs::write(&path, &conf).unwrap();
        prepare_session(&env).unwrap();
        let keyed = format!(
            "{}\n{}\nbind = SUPER, W, killactive\n",
            super::OLD_MOVE_NINE,
            OVERVIEW_KEY.1
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), keyed);
        let kept = format!("{}.bak-overview", path.display());
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), conf);
        // Again changes nothing and tells Hyprland nothing more.
        prepare_session(&env).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), keyed);
        assert_eq!(*RAN.lock().unwrap(), ["hyprctl -i 0 reload"]);
        // Taken out by the account: it stays out.
        std::fs::write(&path, &conf).unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), conf);
        // Its own Super+Tab, on an account never given the key: left alone,
        // and nothing kept.
        std::fs::remove_file(&kept).unwrap();
        let own = format!("{}\nbind = SUPER, Tab, cyclenext\n", super::OLD_MOVE_NINE);
        std::fs::write(&path, &own).unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), own);
        assert!(!std::path::Path::new(&kept).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_skeleton_already_has_every_key_an_older_account_is_given() {
        let skel = include_str!("../../../../desktop/skel/desktop/.config/hypr/hyprland.conf");
        assert!(skel.contains(FOCUS_DOWN));
        for (_, key) in super::SCREEN_KEYS {
            assert!(skel.lines().any(|l| l == key), "{key}");
        }
        assert_eq!(with_keys(skel), None);
        assert_eq!(with_screen_keys(skel), None);
        assert_eq!(super::with_moves(skel), None);
        assert!(skel.lines().any(|l| l == OVERVIEW_KEY.1));
        assert_eq!(with_overview_key(skel), None);
    }
}

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
/// Super+L as accounts made before Alpymist's own lock have it.
const OLD_LOCK: &str = "bind = SUPER, L, exec, swaylock -f -c 0b121e";
/// Super+L as it is now.
const NEW_LOCK: &str = "bind = SUPER, L, exec, alpymist-lock";

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
/// started with. Says what it did.
fn take_over(env: &Env) -> Vec<String> {
    let path = env.account(HYPRLAND);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Some(new) = with_keys(&text) else {
        return Vec::new();
    };
    let kept = crate::generated::beside(&path, KEPT);
    if std::fs::write(&kept, &text).is_err() || crate::generated::replace(&path, &new).is_err() {
        return Vec::new();
    }
    if env.hyprland {
        let _ = env.run(&["hyprctl", "reload"]);
    }
    vec![format!(
        "Super+1 to Super+9 now follow the screen the pointer is on, and Super+L \
         locks with Alpymist's lock ({} as it was).",
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

#[cfg(test)]
mod tests {
    use super::{LID, WORKSPACES, get, set, settings, with_keys};
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
}

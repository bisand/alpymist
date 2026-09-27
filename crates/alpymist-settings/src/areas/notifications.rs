//! Notifications: where mako shows them, for how long, and do not disturb.
//!
//! The position and the timeout are the account's `settings.toml`, and from
//! them comes mako's whole configuration, `~/.config/mako/config`, as a
//! generated file. Not a file of Alpymist's that the account's configuration
//! includes, as ADR 0007 §3 has it elsewhere: mako refuses to start at all
//! when a file it includes is missing, and an account without notifications is
//! worse than a hand edit not followed. The file every account started with
//! is taken over as it stands; one edited by hand is left alone.
//!
//! Do not disturb is mako's `do-not-disturb` mode, switched in the running
//! mako with `makoctl` as the menu's toggle does, and it lasts until it is
//! turned off or the session ends. The mode hides new notifications only
//! because the generated file says so.

use crate::env::Env;
use crate::generated;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use crate::values::Values;

/// mako's configuration, under the account's configuration.
pub const MAKO_CONF: &str = "mako/config";
/// The account's values.
pub const VALUES: &str = "alpymist/settings.toml";
/// Do not disturb's id, which is not kept in the values.
pub const DO_NOT_DISTURB: &str = "notifications.do-not-disturb";
/// The mode it is in mako.
const MODE: &str = "do-not-disturb";

/// What every account's `~/.config/mako/config` was before Alpymist wrote it
/// from settings: taken over when it is still exactly this.
const SHIPPED: &str = "# Alpymist: notifications.\n\
font=Fira Mono 10\n\
background-color=#0b121eee\n\
text-color=#eaf0f6\n\
border-color=#7fb8d9\n\
border-size=2\n\
border-radius=8\n\
default-timeout=6000\n";

/// Where on the screen, as mako's `anchor` names them.
const POSITIONS: &[(&str, &str)] = &[
    ("top-right", "Top right"),
    ("top-center", "Top centre"),
    ("top-left", "Top left"),
    ("bottom-right", "Bottom right"),
    ("bottom-center", "Bottom centre"),
    ("bottom-left", "Bottom left"),
];

/// How long, in seconds; `never` is until it is dismissed.
const TIMEOUTS: &[(&str, &str)] = &[
    ("3", "3 seconds"),
    ("6", "6 seconds"),
    ("10", "10 seconds"),
    ("15", "15 seconds"),
    ("30", "30 seconds"),
    ("never", "Until dismissed"),
];

fn choices(table: &[(&str, &str)]) -> Kind {
    Kind::Choice(table.iter().map(|&(v, l)| Choice::new(v, l)).collect())
}

/// The settings.
pub fn settings() -> Vec<Setting> {
    let account = |id, title, description, keywords, kind, default, applies| Setting {
        id,
        title,
        description,
        keywords,
        kind,
        default,
        scope: Scope::Account,
        applies,
    };
    vec![
        account(
            DO_NOT_DISTURB,
            "Do not disturb",
            "Show no new notifications until it is turned off, or until you log out.",
            &["dnd", "silence", "quiet", "focus", "mute"],
            Kind::Switch,
            Value::Bool(false),
            Applies::Now,
        ),
        account(
            "notifications.position",
            "Position",
            "Which corner or edge of the screen notifications appear at.",
            &["corner", "anchor", "where", "place"],
            choices(POSITIONS),
            Value::Text("top-right".into()),
            Applies::Now,
        ),
        account(
            "notifications.timeout",
            "Show for",
            "How long a notification stays, when the program that sent it does not say.",
            &["timeout", "duration", "dismiss", "expire", "how long"],
            choices(TIMEOUTS),
            Value::Text("6".into()),
            Applies::Now,
        ),
    ]
}

/// Its value: do not disturb from the running mako, the rest from the file.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    if setting.id == DO_NOT_DISTURB {
        let modes = env.run(&["makoctl", "mode"])?;
        return Ok(Value::Bool(modes.lines().any(|m| m.trim() == MODE)));
    }
    Ok(Values::load(&env.account(VALUES))?
        .get(setting.id)
        .unwrap_or_else(|| setting.default.clone()))
}

/// Set one, or reset it with `None`. Returns notes worth telling.
pub fn set(
    env: &Env,
    setting: &Setting,
    value: Option<&Value>,
    force: bool,
) -> Result<Vec<String>, String> {
    if setting.id == DO_NOT_DISTURB {
        return do_not_disturb(env, value.and_then(Value::as_bool).unwrap_or(false));
    }
    let path = env.account(VALUES);
    let mut values = Values::load(&path)?;
    match value {
        Some(v) => values.set(setting.id, v),
        None => values.remove(setting.id),
    }
    // mako's file first: if it was edited by hand, nothing changes.
    write_conf(env, &values, force)?;
    values.save(&path)?;
    Ok(Vec::new())
}

/// Switch the mode in the running mako. Before turning it on, make sure the
/// file that gives the mode a meaning is the one mako has read.
fn do_not_disturb(env: &Env, on: bool) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let path = env.account(MAKO_CONF);
    if on && generated::state(&path, "#", adopt)? == generated::State::HandEdited {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if !text.contains(&format!("[mode={MODE}]")) {
            notes.push(format!(
                "~/.config/mako/config was edited by hand and has no [mode={MODE}] \
                 section, so notifications still show."
            ));
        }
    } else if on {
        let before = std::fs::read_to_string(&path).ok();
        write_conf(env, &Values::load(&env.account(VALUES))?, false)?;
        if before != std::fs::read_to_string(&path).ok() {
            env.run(&["makoctl", "reload"])?;
        }
    }
    let flag = if on { "-a" } else { "-r" };
    env.run(&["makoctl", "mode", flag, MODE])?;
    Ok(notes)
}

/// Have the running mako read its file again.
pub fn live(env: &Env, setting: &Setting) -> Result<(), String> {
    if setting.id == DO_NOT_DISTURB {
        return Ok(());
    }
    env.run(&["makoctl", "reload"]).map(drop)
}

/// Whether `text` is the file every account started with.
fn adopt(text: &str) -> bool {
    text == SHIPPED
}

/// mako's configuration for `values`.
fn conf(values: &Values) -> String {
    let all = settings();
    let value = |id: &str| {
        values
            .get(id)
            .or_else(|| all.iter().find(|s| s.id == id).map(|s| s.default.clone()))
            .and_then(|v| v.as_text().map(str::to_owned))
            .unwrap_or_default()
    };
    let timeout = match value("notifications.timeout").as_str() {
        "never" => 0,
        seconds => seconds.parse::<u32>().unwrap_or(6) * 1000,
    };
    format!(
        "font=Fira Mono 10\n\
         background-color=#0b121eee\n\
         text-color=#eaf0f6\n\
         border-color=#7fb8d9\n\
         border-size=2\n\
         border-radius=8\n\
         anchor={}\n\
         default-timeout={timeout}\n\
         \n\
         # Do not disturb, from Settings or the menu: new notifications are not shown.\n\
         [mode={MODE}]\n\
         invisible=1\n",
        value("notifications.position"),
    )
}

fn write_conf(env: &Env, values: &Values, force: bool) -> Result<(), String> {
    generated::write(
        &env.account(MAKO_CONF),
        "#",
        "~/.config/alpymist/settings.toml",
        &conf(values),
        adopt,
        force,
    )
}

/// Write mako's file before mako starts, so the mode means something from the
/// first login: when it is missing, is the file every account started with,
/// or was written from settings. Left alone when edited by hand.
///
/// # Errors
/// The values could not be read or the file written.
pub fn prepare_session(env: &Env) -> Result<(), String> {
    match generated::state(&env.account(MAKO_CONF), "#", adopt)? {
        generated::State::HandEdited => Ok(()),
        _ => write_conf(env, &Values::load(&env.account(VALUES))?, false),
    }
}

#[cfg(test)]
mod tests {
    use super::{MAKO_CONF, SHIPPED, conf, prepare_session};
    use crate::env::Env;
    use crate::values::Values;
    use crate::{Error, Settings, Value};
    use std::sync::Mutex;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "alpymist-notifications-{name}-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn the_skeleton_is_what_settings_would_write() {
        let skel = include_str!("../../../../desktop/skel/common/.config/mako/config");
        assert_eq!(
            skel,
            crate::generated::render(
                "#",
                "~/.config/alpymist/settings.toml",
                &conf(&Values::default())
            )
        );
    }

    #[test]
    fn the_defaults_keep_what_every_account_had() {
        let body = conf(&Values::default());
        for line in SHIPPED.lines().skip(1) {
            assert!(body.lines().any(|l| l == line), "{line} went missing");
        }
        assert!(body.contains("anchor=top-right\n"), "{body}");
    }

    #[test]
    fn the_shipped_file_is_taken_over_and_a_hand_edit_is_not() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("takeover");
        let env = Env::test(&d, false, &RAN);
        let path = env.account(MAKO_CONF);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, SHIPPED).unwrap();
        prepare_session(&env).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("[mode=do-not-disturb]\ninvisible=1\n")
        );

        let settings = Settings::new();
        settings
            .set(&env, "notifications.position", "bottom-left", false)
            .unwrap();
        settings
            .set(&env, "notifications.timeout", "never", false)
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("anchor=bottom-left\n"), "{text}");
        assert!(text.contains("default-timeout=0\n"), "{text}");
        assert_eq!(
            settings.get(&env, "notifications.timeout"),
            Ok(Value::Text("never".into()))
        );

        std::fs::write(&path, "font=Comic Sans 12\n").unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "font=Comic Sans 12\n"
        );
        let refused = settings.set(&env, "notifications.position", "top-left", false);
        assert!(
            matches!(refused, Err(Error::Failed(ref m)) if m.contains("by hand")),
            "{refused:?}"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn do_not_disturb_is_the_running_makos_mode() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("dnd");
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();
        settings
            .set(&env, "notifications.do-not-disturb", "on", false)
            .unwrap();
        settings
            .set(&env, "notifications.do-not-disturb", "off", false)
            .unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "makoctl reload",
                "makoctl mode -a do-not-disturb",
                "makoctl mode -r do-not-disturb",
            ]
        );
        assert!(
            std::fs::read_to_string(env.account(MAKO_CONF))
                .unwrap()
                .contains("[mode=do-not-disturb]")
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

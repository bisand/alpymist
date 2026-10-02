//! The lock, for an account made before Alpymist had one of its own
//! (ADR 0010). Such an account was given `swaylock` twice: on Super+L in its
//! `hyprland.conf`, and in `power.toml` as what locks before the machine
//! sleeps. swaylock is no longer installed, so the key did nothing and a
//! closed lid slept unlocked, a failed lock not being allowed to stop a
//! suspend.
//!
//! Nothing here is a setting. Both lines are put right before each session,
//! and at an upgrade, where they are still exactly what Alpymist wrote: a
//! lock somebody chose, swaylock with other arguments included, is theirs.

use crate::env::Env;
use crate::generated;
use std::fmt::Write as _;

/// The account's Hyprland configuration.
const HYPRLAND: &str = "hypr/hyprland.conf";
/// What either file is kept beside it as, before its lock changes.
const KEPT: &str = ".bak-lock";
/// Super+L as accounts made before Alpymist's own lock have it.
pub(super) const OLD_KEY: &str = "bind = SUPER, L, exec, swaylock -f -c 0b121e";
/// Super+L as it is now.
pub(super) const NEW_KEY: &str = "bind = SUPER, L, exec, alpymist-lock";
/// What locks before sleeping, as `power.toml` was first written.
const OLD_COMMAND: &str = r#"lock = "swaylock -f -c 0b121e""#;
/// The same as it is now: `-f` returns once the lock is up.
const NEW_COMMAND: &str = r#"lock = "alpymist-lock -f""#;

/// Have Super+L and the lock before sleeping run Alpymist's lock, where they
/// still run the swaylock every account once started with. Each file is
/// tried whatever became of the other.
///
/// # Errors
/// A file could not be written.
pub fn prepare_session(env: &Env) -> Result<(), String> {
    let key = put_right(env, HYPRLAND, OLD_KEY, NEW_KEY);
    let command = put_right(env, super::power::CONFIG, OLD_COMMAND, NEW_COMMAND);
    key.and(command)
}

/// Replace each line of the account's `file` that is `old` with `new`,
/// keeping the file as it was beside it. A file that is missing, or has no
/// such line, is left alone.
fn put_right(env: &Env, file: &str, old: &str, new: &str) -> Result<(), String> {
    let path = env.account(file);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Some(right) = with_line(&text, old, new) else {
        return Ok(());
    };
    let kept = generated::beside(&path, KEPT);
    std::fs::write(&kept, &text).map_err(|e| format!("{}: {e}", kept.display()))?;
    generated::replace(&path, &right)
}

/// `text` with each line that is `old` now `new`; `None` when there is none.
pub(super) fn with_line(text: &str, old: &str, new: &str) -> Option<String> {
    if !text.lines().any(|l| l.trim() == old) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let _ = writeln!(out, "{}", if line.trim() == old { new } else { line });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{HYPRLAND, prepare_session};
    use crate::areas::power::CONFIG;
    use crate::env::Env;
    use std::sync::Mutex;

    fn account(name: &str) -> (std::path::PathBuf, Env) {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let dir = std::env::temp_dir().join(format!("alpymist-lock-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        for file in [HYPRLAND, CONFIG] {
            std::fs::create_dir_all(env.account(file).parent().unwrap()).unwrap();
        }
        (dir, env)
    }

    #[test]
    fn the_swaylock_every_account_started_with_becomes_alpymists_lock() {
        let (dir, env) = account("old");
        let conf = "bind = SUPER, F, fullscreen\nbind = SUPER, L, exec, swaylock -f -c 0b121e\n";
        let power = "[actions]\nlid = \"suspend\"\n\n[commands]\n\
                     lock = \"swaylock -f -c 0b121e\"\nmenu = \"alpymist-menu system\"\n";
        std::fs::write(env.account(HYPRLAND), conf).unwrap();
        std::fs::write(env.account(CONFIG), power).unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(
            std::fs::read_to_string(env.account(HYPRLAND)).unwrap(),
            "bind = SUPER, F, fullscreen\nbind = SUPER, L, exec, alpymist-lock\n"
        );
        let now = std::fs::read_to_string(env.account(CONFIG)).unwrap();
        assert_eq!(
            alpymist_power::config::Config::parse(&now)
                .unwrap()
                .commands,
            alpymist_power::config::Commands::default(),
            "{now}"
        );
        // Each as it was is beside it.
        for (file, was) in [(HYPRLAND, conf), (CONFIG, power)] {
            let kept = format!("{}.bak-lock", env.account(file).display());
            assert_eq!(std::fs::read_to_string(kept).unwrap(), was);
        }
        // Once is enough: nothing is written, or kept, a second time.
        prepare_session(&env).unwrap();
        let again = format!("{}.bak-lock-2", env.account(HYPRLAND).display());
        assert!(!std::path::Path::new(&again).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_lock_somebody_chose_is_theirs_and_a_missing_file_is_no_error() {
        let (dir, env) = account("own");
        // swaylock still, but not as Alpymist wrote it.
        let conf = "bind = SUPER, L, exec, swaylock -f -c 000000\n";
        std::fs::write(env.account(HYPRLAND), conf).unwrap();
        prepare_session(&env).unwrap();
        assert_eq!(
            std::fs::read_to_string(env.account(HYPRLAND)).unwrap(),
            conf
        );
        assert!(!env.account(CONFIG).exists(), "no power.toml is made");
        std::fs::remove_dir_all(&dir).ok();
    }
}

//! The services a setting turns on and off: in the default runlevel
//! and running, or neither. The SSH server, network time and Bluetooth are
//! each one; what else they do when switched is their own.
//!
//! On is being in the default runlevel, which is what decides whether the
//! service is there after the next boot. Starting one already started and
//! adding one already added both succeed; taking out one that is not in the
//! runlevel fails, so that is done only when it is there.

use crate::env::Env;
use std::path::PathBuf;

/// `name`'s link in the default runlevel.
pub(crate) fn link(env: &Env, name: &str) -> PathBuf {
    env.system(&format!("etc/runlevels/default/{name}"))
}

/// Whether `name` starts at boot.
pub(crate) fn at_boot(env: &Env, name: &str) -> bool {
    link(env, name).symlink_metadata().is_ok()
}

/// Start `name` now and at every boot, as root.
pub(crate) fn start(env: &Env, name: &str) -> Result<(), String> {
    env.run(&["rc-update", "add", name, "default"])?;
    env.run(&["rc-service", name, "start"]).map(drop)
}

/// Stop `name` if it is running, and take it out of the default runlevel if
/// it is there, as root.
pub(crate) fn stop(env: &Env, name: &str) -> Result<(), String> {
    env.run(&["rc-service", "--ifstarted", name, "stop"])?;
    if at_boot(env, name) {
        env.run(&["rc-update", "del", name, "default"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{at_boot, link, start, stop};
    use crate::env::Env;
    use std::sync::Mutex;

    #[test]
    fn on_is_the_runlevel_and_off_takes_out_only_what_is_there() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-service-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, true, &RAN);
        assert!(!at_boot(&env, "thing"));
        start(&env, "thing").unwrap();
        // Not in the runlevel: stopped, not taken out.
        stop(&env, "thing").unwrap();
        let l = link(&env, "thing");
        std::fs::create_dir_all(l.parent().unwrap()).unwrap();
        std::fs::write(&l, "").unwrap();
        assert!(at_boot(&env, "thing"));
        stop(&env, "thing").unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "rc-update add thing default",
                "rc-service thing start",
                "rc-service --ifstarted thing stop",
                "rc-service --ifstarted thing stop",
                "rc-update del thing default",
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

//! Closing the lid and pressing the power button.
//!
//! The compositor sees both — a lid is a switch and the power button a key to
//! libinput — and runs `alpymist-power lid` or `alpymist-power button`, which
//! decide here what that means from the user's [`Config`]. There is no elogind
//! to do it: the desktop runs seatd, so nothing else is listening.
//!
//! Locking runs as the user. Suspending, hibernating and shutting down need
//! root, and go through `alpymist-power-helper` with pkexec. polkit lets
//! anyone at the machine — in the seat group, which the desktop's seat needs
//! anyway — do those without a password, so a closing lid never waits for
//! one; a charge limit, which outlives the session, asks an administrator in
//! alpymist-auth's dialog.

use crate::battery::Power;
use crate::config::{Action, Config};
use std::path::Path;
use std::process::Command;

/// Where the helper is installed.
pub const HELPER: &str = "/usr/libexec/alpymist-power-helper";

/// What closing the lid does now.
#[must_use]
pub fn for_lid(config: &Config, power: &Power, docked: bool) -> Action {
    let a = &config.actions;
    let action = if docked {
        a.lid_docked
    } else if power.on_mains() {
        a.lid_on_power
    } else {
        a.lid
    };
    // The menu means nothing with the lid shut.
    if action == Action::Menu {
        Action::Nothing
    } else {
        action
    }
}

/// Whether a screen other than the built-in one is connected.
///
/// Read from DRM's connectors rather than asked of the compositor, so it
/// holds under any of them. Built-in panels are eDP, LVDS and DSI.
#[must_use]
pub fn docked(drm: &Path) -> bool {
    let Ok(dir) = std::fs::read_dir(drm) else {
        return false;
    };
    dir.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        // card0-HDMI-A-1; a bare card0 is the device, not a connector.
        let Some((_, connector)) = name.split_once('-') else {
            return false;
        };
        let internal = ["eDP", "LVDS", "DSI"]
            .iter()
            .any(|p| connector.starts_with(p));
        !internal
            && std::fs::read_to_string(entry.path().join("status"))
                .is_ok_and(|s| s.trim() == "connected")
    })
}

fn shell(command: &str) -> Result<(), String> {
    let status = Command::new("/bin/sh")
        .args(["-c", command])
        .status()
        .map_err(|e| format!("could not run {command}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} failed ({status})"))
    }
}

/// Where the password dialog is.
pub const AUTH: &str = "/usr/bin/alpymist-auth";

/// Whether polkit asks a password for the helper's `verb`. Those that do go
/// through `alpymist-auth run`, which brings its own agent to ask with; the
/// rest need none, and are run by pkexec directly, so the lid never depends
/// on the dialog.
#[must_use]
pub fn asks_password(verb: &str) -> bool {
    verb == "charge-limit"
}

/// Run the helper as root through pkexec.
///
/// # Errors
/// polkit refused, or the helper failed; its own message is passed on.
pub fn helper(args: &[&str]) -> Result<(), String> {
    // `alpymist-auth run` runs pkexec itself, with its own agent to ask.
    let mut command = if args.first().is_some_and(|v| asks_password(v)) {
        let mut c = Command::new(AUTH);
        c.args(["run", "--"]);
        c
    } else {
        Command::new("pkexec")
    };
    let output = command.arg(HELPER).args(args).output().map_err(|e| {
        format!(
            "could not run {}: {e}",
            command.get_program().to_string_lossy()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(explain(&said, output.status.code()))
}

/// pkexec's refusals, and the helper's own messages, as the popup says them.
fn explain(said: &str, code: Option<i32>) -> String {
    if said.contains("dismissed") {
        "Cancelled.".into()
    } else if said.contains("Not authorized") {
        "Not allowed: this needs an administrator's password.".into()
    } else if said.contains("authentication agent") {
        "Not allowed: nothing could ask for a password.".into()
    } else if said.is_empty() {
        format!(
            "{HELPER} failed ({})",
            code.map_or_else(|| "killed".into(), |c| c.to_string())
        )
    } else {
        said.lines()
            .last()
            .unwrap_or(said)
            .trim_start_matches("alpymist-power-helper: ")
            .to_owned()
    }
}

/// Carry out `action`.
///
/// # Errors
/// Whatever step failed. A lock that fails does not stop a suspend: a
/// machine left running in a bag is worse than one asleep unlocked.
pub fn run(action: Action, config: &Config) -> Result<(), String> {
    match action {
        Action::Nothing => Ok(()),
        Action::Lock => shell(&config.commands.lock),
        Action::Menu => shell(&config.commands.menu),
        Action::Suspend | Action::Hibernate => {
            let (verb, state) = if action == Action::Suspend {
                ("suspend", "mem")
            } else {
                ("hibernate", "disk")
            };
            // Asked first, so a machine that cannot is not locked for nothing.
            crate::system::can_sleep(Path::new(crate::profile::SYSFS), state)?;
            if let Err(e) = shell(&config.commands.lock) {
                eprintln!("alpymist-power: {e}");
            }
            helper(&[verb])
        }
        Action::PowerOff => helper(&["power-off"]),
    }
}

#[cfg(test)]
mod tests {
    use super::{asks_password, docked, explain, for_lid};
    use crate::battery::sample;
    use crate::config::{Action, Config};

    #[test]
    fn only_the_charge_limit_asks_and_refusals_read_plainly() {
        assert!(asks_password("charge-limit"));
        for verb in ["suspend", "hibernate", "power-off", "reboot", "profile"] {
            assert!(
                !asks_password(verb),
                "{verb} must never wait for a password"
            );
        }
        assert_eq!(
            explain(
                "Error executing command as another user: Request dismissed",
                Some(126)
            ),
            "Cancelled."
        );
        assert_eq!(
            explain(
                "Error creating textual authentication agent: no tty",
                Some(127)
            ),
            "Not allowed: nothing could ask for a password."
        );
        assert_eq!(
            explain(
                "alpymist-power-helper: this machine cannot hibernate",
                Some(1)
            ),
            "this machine cannot hibernate"
        );
    }

    #[test]
    fn the_lid_follows_the_charger_and_the_dock() {
        let mut c = Config::default();
        c.actions.lid = Action::Suspend;
        c.actions.lid_on_power = Action::Lock;
        c.actions.lid_docked = Action::Nothing;
        let mut p = sample();
        assert_eq!(for_lid(&c, &p, false), Action::Suspend);
        p.plugged = Some(true);
        assert_eq!(for_lid(&c, &p, false), Action::Lock);
        assert_eq!(for_lid(&c, &p, true), Action::Nothing);
        c.actions.lid = Action::Menu;
        p.plugged = Some(false);
        assert_eq!(for_lid(&c, &p, false), Action::Nothing);
    }

    #[test]
    fn only_an_outside_screen_is_a_dock() {
        let root = std::env::temp_dir().join(format!("alpymist-drm-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (name, status) in [
            ("card0", ""),
            ("card0-eDP-1", "connected"),
            ("card0-HDMI-A-1", "disconnected"),
        ] {
            std::fs::create_dir_all(root.join(name)).unwrap();
            std::fs::write(root.join(name).join("status"), status).unwrap();
        }
        assert!(!docked(&root));
        std::fs::write(root.join("card0-HDMI-A-1/status"), "connected\n").unwrap();
        assert!(docked(&root));
        std::fs::remove_dir_all(&root).ok();
    }
}

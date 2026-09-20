//! Noticing that nobody is there.
//!
//! There is no screensaver protocol on Wayland, and nothing like the X server's
//! screen saver extension. What there is, is `ext-idle-notify-v1`: a compositor
//! tells a client when a seat has been still for so long, and tells it again
//! when somebody touches it. `swayidle` is the client that speaks it, and it
//! runs a command at each timeout, so the policy — mountains, then darkness,
//! then perhaps a lock — is an argument list rather than a protocol.
//!
//! This module builds that argument list from the account's settings and keeps
//! a `swayidle` running with it. Running `alpymist-screensaver idle` a second
//! time does not start a second one: it tells the first to read the file again
//! and start over, which is how a change in Settings takes effect without
//! logging out.

use crate::config::Config;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command};

/// The lock screen: the same command the compositors bind to Super+L, so
/// locking from the keyboard and locking from idleness are the same thing. `-f`
/// returns once the screen is covered, which is what a lock before sleeping
/// needs and what a lock before a blank screen may as well have.
const LOCK: &str = "alpymist-lock -f";

/// Turning the screen off. `wlopm` speaks `wlr-output-power-management`, which
/// Hyprland and labwc both implement, so one command covers both Wayland tiers
/// where `hyprctl dispatch dpms` would cover only one.
const OFF: &str = "wlopm --off '*'";
/// And on again.
const ON: &str = "wlopm --on '*'";

/// Where the watcher listens, so a second run can reach the first.
#[must_use]
pub fn socket() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    Some(PathBuf::from(dir).join(format!("alpymist-idle-{display}.sock")))
}

/// Seconds of stillness a setting in minutes asks for.
fn seconds(minutes: u32) -> u32 {
    minutes.saturating_mul(60)
}

/// This program, as the commands name it.
///
/// The bare name, found on `PATH`, rather than this process's own path: every
/// command swayidle runs goes through `sh -c`, where a path with a space in it
/// would be two arguments, and every other program the desktop starts is named
/// the same way.
const ME: &str = "alpymist-screensaver";

/// `swayidle`'s arguments for these settings.
///
/// Empty when the settings ask for nothing at all, which the caller should read
/// as "do not start a watcher" rather than "start one that does nothing".
///
/// Note what is *not* here: `-w`. It makes swayidle wait for each command to
/// finish before it does anything else, which is right for a lock before
/// suspend and quite wrong for a screensaver that runs for as long as nobody
/// touches the machine — it would hold off the timeout that turns the screen
/// off, and the picture would stay lit all night.
#[must_use]
pub fn arguments(config: &Config) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut timeout = |after: u32, run: &str, resume: Option<&str>| {
        args.extend(["timeout".to_owned(), after.to_string(), run.to_owned()]);
        if let Some(resume) = resume {
            args.extend(["resume".to_owned(), resume.to_owned()]);
        }
    };

    if config.after > 0 {
        timeout(seconds(config.after), ME, Some(&format!("{ME} stop")));
    }
    if config.blank_after > 0 {
        // Never before the mountains: a hand-edited file can put the screen off
        // ahead of the picture that is meant to precede it, and a screen that
        // goes dark and then lights up with a screensaver is worse than either.
        let blank = seconds(config.blank_after.max(config.after));
        if config.lock {
            timeout(blank, LOCK, None);
        }
        timeout(blank, OFF, Some(ON));
    }
    if config.lock {
        // Locking before suspend is the one place waiting matters, and
        // `alpymist-lock -f` does the waiting itself: it returns only once the
        // lock is on screen, so the command is done when the screen is covered.
        args.extend(["before-sleep".to_owned(), LOCK.to_owned()]);
    }
    args
}

/// Tell a watcher, if one is listening, to read the settings again.
///
/// Returns whether anybody was there. Nobody is not an error: settings can be
/// changed from a text console, or before the desktop starts.
#[must_use]
pub fn reload() -> bool {
    let Some(path) = socket() else {
        return false;
    };
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            stream.write_all(b"reload").ok();
            true
        }
        Err(_) => false,
    }
}

/// Start `swayidle` with these settings, or `None` when they ask for nothing.
fn start(config: &Config) -> Option<Child> {
    let args = arguments(config);
    if args.is_empty() {
        return None;
    }
    match Command::new("swayidle").args(&args).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("alpymist-screensaver: could not start swayidle: {e}");
            None
        }
    }
}

/// Stop a `swayidle` this process started, and wait for it to go.
fn stop(child: Option<Child>) {
    if let Some(mut child) = child {
        child.kill().ok();
        child.wait().ok();
    }
}

/// Watch for idleness until the session ends, restarting on every reload.
///
/// Returns once a second run has been told there is already a watcher, so
/// `alpymist-screensaver idle` is safe to run from a login file and from
/// Settings alike.
///
/// What this does not do is notice a `swayidle` that died on its own. The way
/// that happens is the compositor going away, which takes the session and this
/// with it; a compositor restarted in place would leave a watch with nothing
/// under it until the next `alpymist-screensaver idle`, which Settings runs on
/// any change. Watching for it would mean a thread waiting on the child beside
/// the thread waiting on the socket, and that has not been worth it yet.
///
/// # Errors
/// When there is no runtime directory to listen in, which means there is no
/// session either.
pub fn watch() -> Result<(), String> {
    let path = socket().ok_or("no XDG_RUNTIME_DIR: this needs a login session")?;
    // Somebody already watching is told to start over, and that is this run's
    // whole job. A socket left by a crash refuses the connection, and is then
    // ours to replace.
    if reload() {
        return Ok(());
    }
    std::fs::remove_file(&path).ok();
    let listener = UnixListener::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut config = Config::load().unwrap_or_else(|e| {
        eprintln!("alpymist-screensaver: {e}");
        Config::default()
    });
    let mut child = start(&config);

    for connection in listener.incoming() {
        if connection.is_err() {
            continue;
        }
        config = Config::load().unwrap_or_else(|e| {
            eprintln!("alpymist-screensaver: {e}");
            config
        });
        stop(child.take());
        child = start(&config);
    }
    stop(child);
    std::fs::remove_file(&path).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::arguments;
    use crate::config::Config;

    /// The arguments as one string, which is how they read in a shell and how a
    /// test can say what should be in them without pinning their order.
    fn line(config: &Config) -> String {
        arguments(config).join(" ")
    }

    #[test]
    fn the_defaults_show_the_mountains_then_turn_the_screen_off() {
        let args = arguments(&Config::default());
        let joined = args.join(" ");
        assert!(joined.contains("timeout 300"), "five minutes: {joined}");
        assert!(joined.contains("timeout 600"), "ten minutes: {joined}");
        assert!(joined.contains("wlopm --off"), "and darkness: {joined}");
        assert!(!joined.contains("alpymist-lock"), "but no lock unasked for");
    }

    #[test]
    fn nothing_asked_for_is_no_watcher_at_all() {
        let quiet = Config {
            after: 0,
            blank_after: 0,
            lock: false,
            ..Config::default()
        };
        assert!(arguments(&quiet).is_empty());
    }

    #[test]
    fn a_screensaver_with_no_blanking_still_gets_its_timeout() {
        let c = Config {
            after: 3,
            blank_after: 0,
            lock: false,
            ..Config::default()
        };
        let line = line(&c);
        assert!(line.contains("timeout 180"));
        assert!(!line.contains("wlopm"), "nothing turns the screen off");
    }

    #[test]
    fn blanking_with_no_screensaver_is_just_the_screen_going_off() {
        let c = Config {
            after: 0,
            blank_after: 10,
            lock: false,
            ..Config::default()
        };
        let line = line(&c);
        assert!(line.contains("timeout 600"));
        assert!(line.contains("wlopm --off"));
        assert!(!line.contains("stop"), "no screensaver to take away");
    }

    #[test]
    fn locking_adds_the_lock_and_locks_before_suspending() {
        let c = Config {
            after: 5,
            blank_after: 10,
            lock: true,
            ..Config::default()
        };
        let line = line(&c);
        assert!(line.contains("before-sleep alpymist-lock"), "{line}");
        assert_eq!(
            line.matches("alpymist-lock").count(),
            2,
            "at blank, and asleep"
        );
    }

    #[test]
    fn the_screen_never_goes_off_before_the_mountains_arrive() {
        let upside_down = Config {
            after: 10,
            blank_after: 2,
            lock: false,
            ..Config::default()
        };
        let line = line(&upside_down);
        assert!(
            line.contains("timeout 600 "),
            "the mountains at ten: {line}"
        );
        assert_eq!(
            line.matches("timeout 600").count(),
            2,
            "and darkness no sooner: {line}"
        );
    }

    #[test]
    fn the_longest_stillness_offered_does_not_overflow_into_no_time_at_all() {
        let long = Config {
            after: crate::config::MAX_MINUTES,
            blank_after: crate::config::MAX_MINUTES,
            lock: false,
            ..Config::default()
        };
        let seconds = crate::config::MAX_MINUTES * 60;
        assert!(
            line(&long).contains(&format!("timeout {seconds}")),
            "{}",
            line(&long)
        );
    }

    #[test]
    fn resume_takes_the_mountains_away_again() {
        let args = arguments(&Config::default());
        let at = args.iter().position(|a| a == "resume").expect("a resume");
        assert!(args[at + 1].ends_with(" stop"), "{:?}", args[at + 1]);
    }
}

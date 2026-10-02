//! Alpymist's watchdog: the watches a session keeps over its own desktop
//! (ADR 0019).
//!
//! One program, started once by the packaged Hyprland configuration, as the
//! account and with nothing the account does not have: it listens on no
//! socket, reads no configuration, and keeps nothing. What it watches is a
//! short list here, [`WATCHES`], and adding to the list is how a new watch is
//! made, not a new program started from somewhere else.
//!
//! Each watch runs in a process of its own. Alpymist's programs end on a
//! panic rather than unwinding, so only a process keeps one watch's fault
//! from being another's; the watchdog itself only starts them, waits, and
//! starts again one that failed, a little later each time. A watch that ends
//! well has seen Hyprland go, and the watchdog ends with it.
//!
//! - [`bar`]: keeping the bar listening to Hyprland.
//! - the screens are `alpymist_displays::watch`.

pub mod bar;

use rustix::process::{Signal, set_parent_process_death_signal};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A watch: something looked after for as long as the session lasts.
pub struct Watch {
    /// What it is called, on the command line and in what is logged.
    pub name: &'static str,
    /// What it looks after, for `alpymist watchdog --list`.
    pub about: &'static str,
    /// Keep watch until Hyprland ends: `Ok` then, `Err` when it could not
    /// go on.
    pub run: fn() -> Result<(), String>,
}

/// Every watch there is.
pub const WATCHES: &[Watch] = &[
    Watch {
        name: "screens",
        about: "Puts each set of screens in the layout it had, as screens come and go.",
        run: alpymist_displays::watch::run,
    },
    Watch {
        name: "bar",
        about: "Starts the bar again when Hyprland has stopped telling it of workspaces.",
        run: bar::run,
    },
];

/// The shortest wait before a watch that failed is started again.
const SOONEST: Duration = Duration::from_secs(2);
/// The longest.
const LATEST: Duration = Duration::from_mins(5);
/// A watch that kept going this long had not failed at starting, and its
/// next failure is counted as its first.
const STEADY: Duration = Duration::from_mins(1);

/// How long to wait before starting again a watch that has now failed
/// `failures` times running: twice as long each time, up to [`LATEST`].
#[must_use]
pub fn wait_before(failures: u32) -> Duration {
    let doubled = 1u32
        .checked_shl(failures.saturating_sub(1))
        .unwrap_or(u32::MAX);
    SOONEST.saturating_mul(doubled).min(LATEST)
}

/// How many failures running a watch has, given how many it had and how
/// long it kept going this time.
#[must_use]
pub fn failures_after(failures: u32, ran: Duration) -> u32 {
    if ran >= STEADY { 1 } else { failures + 1 }
}

/// Keep the one watch called `name`, in this process, until Hyprland ends.
///
/// # Errors
/// There is no such watch, or it could not go on.
pub fn watch(name: &str) -> Result<(), String> {
    let watch = WATCHES
        .iter()
        .find(|w| w.name == name)
        .ok_or_else(|| format!("no watch called {name}"))?;
    // A watch does not outlive the watchdog that started it.
    let _ = set_parent_process_death_signal(Some(Signal::TERM));
    (watch.run)()
}

/// Start every watch, each as this program again with its name, and start
/// again any that fails, until one ends well: Hyprland has gone.
///
/// # Errors
/// This program could not be found to start again.
pub fn run() -> Result<(), String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    let (ended, wait) = std::sync::mpsc::channel();
    for watch in WATCHES {
        let (program, ended) = (program.clone(), ended.clone());
        std::thread::spawn(move || {
            let mut failures = 0;
            loop {
                let started = Instant::now();
                let outcome = Command::new(&program)
                    .args(["watchdog", watch.name])
                    .stdin(Stdio::null())
                    .status();
                match outcome {
                    Ok(status) if status.success() => {
                        let _ = ended.send(());
                        return;
                    }
                    Ok(status) => eprintln!("alpymist watchdog: {}: {status}", watch.name),
                    Err(e) => eprintln!("alpymist watchdog: {}: {e}", watch.name),
                }
                failures = failures_after(failures, started.elapsed());
                std::thread::sleep(wait_before(failures));
            }
        });
    }
    drop(ended);
    // Ending takes the other watches along: each asked to be ended with
    // this process.
    let _ = wait.recv();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{LATEST, STEADY, WATCHES, failures_after, wait_before};
    use std::time::Duration;

    #[test]
    fn a_failing_watch_is_started_again_later_each_time_and_never_given_up() {
        assert_eq!(wait_before(1), Duration::from_secs(2));
        assert_eq!(wait_before(2), Duration::from_secs(4));
        assert_eq!(wait_before(3), Duration::from_secs(8));
        assert_eq!(wait_before(9), LATEST);
        assert_eq!(wait_before(40), LATEST);
        assert_eq!(wait_before(u32::MAX), LATEST);
    }

    #[test]
    fn a_watch_that_kept_going_starts_its_count_again() {
        assert_eq!(failures_after(0, Duration::ZERO), 1);
        assert_eq!(failures_after(4, Duration::from_secs(3)), 5);
        assert_eq!(failures_after(4, STEADY), 1);
    }

    #[test]
    fn every_watch_has_a_name_of_its_own() {
        for (i, watch) in WATCHES.iter().enumerate() {
            assert!(!watch.name.is_empty() && !watch.about.is_empty());
            assert!(WATCHES[..i].iter().all(|w| w.name != watch.name));
        }
    }
}

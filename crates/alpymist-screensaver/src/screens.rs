//! The screens a picture can go on, and which of them is the main one.
//!
//! Finding them takes a Wayland connection, which is `paint`'s and comes with
//! the `saver` feature. Settings, built without it, asks the
//! `alpymist-screensaver screens` command instead: [`connected`] runs it and
//! reads back what [`line`] wrote.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A screen the picture can be put on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// What the compositor calls it: `eDP-1`, `DP-3`.
    pub name: String,
    /// What it is, where the compositor says so.
    pub description: String,
}

/// The main screen: the one `configured` names if it is connected, or else the
/// first the compositor numbers. `None` with no screens to name.
#[must_use]
pub fn main_screen<'a>(configured: &str, screens: &'a [Screen]) -> Option<&'a Screen> {
    screens
        .iter()
        .find(|s| !configured.is_empty() && s.name == configured)
        .or_else(|| screens.first())
}

/// How `alpymist-screensaver screens` prints one: the name, a tab, and the
/// description. A compositor's output names have no tabs in them.
#[must_use]
pub fn line(screen: &Screen) -> String {
    format!("{}\t{}", screen.name, screen.description)
}

/// The screens in what `alpymist-screensaver screens` printed, first first.
#[must_use]
pub fn parse(printed: &str) -> Vec<Screen> {
    printed
        .lines()
        .filter_map(|l| {
            let (name, description) = l.split_once('\t').unwrap_or((l, ""));
            (!name.is_empty()).then(|| Screen {
                name: name.to_owned(),
                description: description.to_owned(),
            })
        })
        .collect()
}

/// How long [`connected`] waits for an answer. Listing the screens is one
/// round trip to the compositor, a few milliseconds; one that is stuck — a
/// virtual machine whose display stopped taking frames did it — would
/// otherwise hold up every `alpymist` command and Settings with it.
pub const WAIT: Duration = Duration::from_secs(2);

/// The screens connected now, from `alpymist-screensaver screens`; empty where
/// it is not installed, finds no session, or does not answer within [`WAIT`].
#[must_use]
pub fn connected() -> Vec<Screen> {
    run_within(Command::new("alpymist-screensaver").arg("screens"), WAIT)
        .map(|out| parse(&out))
        .unwrap_or_default()
}

/// What `command` printed, if it finished well within `wait`. One that takes
/// longer is killed.
fn run_within(command: &mut Command, wait: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read on the side, so a child that fills the pipe is not waited on
    // while it waits on us.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        out
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < wait => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let out = reader.join().ok()?;
    status.success().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{Screen, line, main_screen, parse, run_within};
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn a_command_that_does_not_answer_is_given_up_on() {
        let started = Instant::now();
        let out = run_within(
            Command::new("sh").args(["-c", "sleep 30"]),
            Duration::from_millis(200),
        );
        assert_eq!(out, None);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            run_within(
                Command::new("sh").args(["-c", "echo hi"]),
                Duration::from_secs(5)
            ),
            Some("hi\n".to_owned())
        );
        assert_eq!(
            run_within(&mut Command::new("false"), Duration::from_secs(5)),
            None
        );
    }

    fn screens(names: &[&str]) -> Vec<Screen> {
        names
            .iter()
            .map(|n| Screen {
                name: (*n).to_owned(),
                description: String::new(),
            })
            .collect()
    }

    #[test]
    fn the_main_screen_is_the_first_unless_another_is_named() {
        let all = screens(&["eDP-1", "DP-3", "DP-5"]);
        assert_eq!(
            main_screen("", &all).map(|s| s.name.as_str()),
            Some("eDP-1")
        );
        assert_eq!(
            main_screen("DP-5", &all).map(|s| s.name.as_str()),
            Some("DP-5")
        );
    }

    #[test]
    fn a_named_screen_that_is_not_connected_falls_back_to_the_first() {
        let undocked = screens(&["eDP-1"]);
        assert_eq!(
            main_screen("DP-5", &undocked).map(|s| s.name.as_str()),
            Some("eDP-1")
        );
        assert_eq!(main_screen("DP-5", &[]), None);
    }

    #[test]
    fn what_the_command_prints_reads_back_as_the_same_screens() {
        let all = vec![
            Screen {
                name: "eDP-1".to_owned(),
                description: String::new(),
            },
            Screen {
                name: "DP-3".to_owned(),
                description: "Samsung Electric Company S24C750".to_owned(),
            },
        ];
        let printed: String = all.iter().map(|s| line(s) + "\n").collect();
        assert_eq!(parse(&printed), all);
        assert_eq!(parse(""), Vec::new());
    }
}

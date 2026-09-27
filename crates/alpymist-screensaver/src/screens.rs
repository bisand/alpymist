//! The screens a picture can go on, and which of them is the main one.
//!
//! Finding them takes a Wayland connection, which is `paint`'s and comes with
//! the `saver` feature. Settings, built without it, asks the
//! `alpymist-screensaver screens` command instead: [`connected`] runs it and
//! reads back what [`line`] wrote.

use std::process::Command;

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

/// The screens connected now, from `alpymist-screensaver screens`; empty where
/// it is not installed, or finds no session.
#[must_use]
pub fn connected() -> Vec<Screen> {
    Command::new("alpymist-screensaver")
        .arg("screens")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| parse(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Screen, line, main_screen, parse};

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

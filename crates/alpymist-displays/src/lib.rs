//! Alpymist's screens: a layout for each set of screens, put back whenever
//! that set is connected again (#17, ADR 0015).
//!
//! Hyprland does the arranging; this decides what to ask of it. Nothing runs
//! in between: the layout for the screens there now is written as Hyprland's
//! own monitor rules, into `~/.config/alpymist/hypr/displays.conf`, which the
//! packaged configuration sources, and given to the running Hyprland with
//! `hyprctl keyword monitor`. So a configuration reload, which Settings does
//! often, keeps the layout rather than undoing it.
//!
//! - [`screen`]: a screen as Hyprland reports it, and what it is called.
//! - [`layout`]: the layouts, and the account's file of them.
//! - [`hypr`]: asking Hyprland, and telling it.
//! - [`arrange`]: where a screen goes when it is moved by hand.
//! - [`watch`]: following screens as they come and go.
//!
//! The laptop's own panel is turned off while the lid is closed and another
//! screen is on, and back on when the lid opens or the other screens go;
//! Hyprland moves its workspaces to the screens left. That is not part of any
//! layout: a layout says how the screens are arranged with the lid open.

pub mod arrange;
pub mod hypr;
pub mod layout;
pub mod screen;
pub mod watch;

use layout::{Layout, Layouts, Output};
use screen::Monitor;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The rules Hyprland reads at start and at every reload.
pub const CONF: &str = "alpymist/hypr/displays.conf";

/// What to do for the screens there now.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The set of screens.
    pub key: Vec<String>,
    /// Its layout.
    pub layout: Layout,
    /// Whether the set was never seen before, and the layout is new.
    pub new: bool,
    /// Whether the laptop's panel is off for a closed lid.
    pub lid_closed: bool,
    /// Hyprland's monitor rules, one a screen.
    pub rules: Vec<String>,
}

/// Decide what to do for `monitors`, from the account's `layouts`, with the
/// lid closed or not.
#[must_use]
pub fn plan(monitors: &[Monitor], layouts: &Layouts, lid_closed: bool) -> Plan {
    let names = screen::names(monitors);
    let key = layout::key(names.iter().cloned());
    let (layout, new) = match layouts.find(&key) {
        Some(l) => (l.clone(), false),
        None => (Layout::extended(monitors), true),
    };
    let output = |name: &str, m: &Monitor| {
        layout
            .outputs
            .iter()
            .find(|o| o.screen == name)
            .cloned()
            .unwrap_or_else(|| Output::of(name, m))
    };
    // Only with somewhere else to show things: a closed laptop on its own is
    // still a laptop, locked or asleep, and its panel is its only screen.
    let others = monitors
        .iter()
        .zip(&names)
        .any(|(m, name)| !m.internal() && output(name, m).enabled);
    let lid_off = lid_closed && others && layouts.lid_off;
    let rules = monitors
        .iter()
        .zip(&names)
        .map(|(m, name)| {
            let target = screen::target(name, m);
            if lid_off && m.internal() {
                format!("{target}, disable")
            } else {
                output(name, m).rule(&target)
            }
        })
        .collect();
    Plan {
        key,
        layout,
        new,
        lid_closed: lid_off,
        rules,
    }
}

/// The file Hyprland sources, for a plan: its rules, and what they are.
#[must_use]
pub fn conf(plan: &Plan) -> String {
    let mut text = String::from(
        "# Written by Alpymist for the screens connected now, from\n\
         # ~/.config/alpymist/displays.toml; rewritten when screens come and go.\n\
         # Change the layout with `alpymist displays`, not here.\n",
    );
    if plan.lid_closed {
        text.push_str("# The lid is closed: the laptop's panel is off.\n");
    }
    for rule in &plan.rules {
        let _ = writeln!(text, "monitor = {rule}");
    }
    text
}

/// `$XDG_CONFIG_HOME/alpymist/hypr/displays.conf`.
#[must_use]
pub fn conf_path() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config").join(CONF)
}

/// Make sure the file Hyprland sources is there before Hyprland starts,
/// since it refuses a missing one. What is in it from last time stays: the
/// watcher puts the right layout in place once Hyprland is up.
///
/// # Errors
/// It could not be written.
pub fn prepare(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(
        path,
        "# Written by Alpymist for the screens connected now. Empty until\n\
         # Hyprland has told it which those are.\n",
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

/// Put the layout for the screens there now in place, remembering it if it
/// is new.
///
/// # Errors
/// Hyprland could not be asked or told, or the file not written.
pub fn apply() -> Result<Plan, String> {
    let monitors = hypr::monitors()?;
    let path = layout::path();
    let mut layouts = Layouts::load(&path);
    let plan = plan(&monitors, &layouts, lid_closed(Path::new(LID)));
    if plan.new {
        layouts.put(plan.layout.clone());
        // Unremembered is still applied: the screens work this time.
        if let Err(e) = layouts.save(&path) {
            eprintln!("alpymist displays: {e}");
        }
    }
    write(&conf_path(), &conf(&plan))?;
    hypr::apply(&plan.rules)?;
    Ok(plan)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Where ACPI says whether a lid is open.
pub const LID: &str = "/proc/acpi/button/lid";

/// Whether the lid is closed: any lid ACPI knows of says so. A machine
/// without one has no lid to close.
#[must_use]
pub fn lid_closed(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|e| {
            std::fs::read_to_string(e.path().join("state"))
                .is_ok_and(|s| s.split_whitespace().last() == Some("closed"))
        })
    })
}

pub(crate) fn base(variable: &str, under_home: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(under_home)))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::{conf, lid_closed, plan};
    use crate::layout::{Layout, Layouts};
    use crate::screen::Monitor;

    fn screen(name: &str, description: &str) -> Monitor {
        Monitor {
            name: name.into(),
            description: description.into(),
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
            scale: 1.0,
            ..Monitor::default()
        }
    }

    #[test]
    fn a_known_set_gets_its_layout_back_whatever_ports_it_is_on() {
        let desk = [screen("eDP-1", "Panel"), screen("DP-3", "Samsung A")];
        let mut layout = Layout::extended(&desk);
        layout.outputs[1].position = [-1920, 0];
        let mut layouts = Layouts::default();
        layouts.put(layout);
        // After a reboot the same screen is on another connector.
        let again = [screen("eDP-1", "Panel"), screen("DP-5", "Samsung A")];
        let p = plan(&again, &layouts, false);
        assert!(!p.new);
        assert_eq!(
            p.rules,
            [
                "desc:Panel, 1920x1080@60.00, 0x0, 1",
                "desc:Samsung A, 1920x1080@60.00, -1920x0, 1"
            ]
        );
        let away = plan(&[screen("eDP-1", "Panel")], &layouts, false);
        assert!(away.new, "the laptop alone is another set");
    }

    #[test]
    fn a_closed_lid_turns_the_panel_off_only_with_another_screen_on() {
        let desk = [screen("eDP-1", "Panel"), screen("DP-3", "Samsung A")];
        let p = plan(&desk, &Layouts::default(), true);
        assert!(p.lid_closed);
        assert_eq!(p.rules[0], "desc:Panel, disable");
        assert!(conf(&p).contains("monitor = desc:Panel, disable\n"));
        assert!(
            p.layout.outputs[0].enabled,
            "the layout keeps the panel for when the lid opens"
        );
        let alone = plan(&[screen("eDP-1", "Panel")], &Layouts::default(), true);
        assert!(!alone.lid_closed, "never no screen at all");
        let kept_on = Layouts {
            lid_off: false,
            ..Layouts::default()
        };
        assert!(
            !plan(&desk, &kept_on, true).lid_closed,
            "not when Settings says to leave it on"
        );
        assert_eq!(alone.rules[0], "desc:Panel, 1920x1080@60.00, 0x0, 1");
    }

    #[test]
    fn the_lid_is_read_from_acpi() {
        let dir = std::env::temp_dir().join(format!("alpymist-lid-{}", std::process::id()));
        let lid = dir.join("LID0");
        std::fs::create_dir_all(&lid).unwrap();
        std::fs::write(lid.join("state"), "state:      open\n").unwrap();
        assert!(!lid_closed(&dir));
        std::fs::write(lid.join("state"), "state:      closed\n").unwrap();
        assert!(lid_closed(&dir));
        assert!(!lid_closed(&dir.join("none")), "no lid is never closed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

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
//! - [`lid`]: what the laptop's screen showed when the lid closed.
//! - [`workspaces`]: each screen's own workspaces 1 to 9.
//!
//! The laptop's own panel is turned off while the lid is closed and another
//! screen is on, and back on when the lid opens or the other screens go;
//! Hyprland moves its workspaces to the screens left. That is not part of any
//! layout: a layout says how the screens are arranged with the lid open.

pub mod arrange;
pub mod hypr;
pub mod layout;
pub mod lid;
pub mod screen;
pub mod watch;
pub mod workspaces;

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
    /// Hyprland's workspace rules.
    pub workspace_rules: Vec<String>,
    /// Each screen that is on, by connector, and its block of workspaces.
    pub on: Vec<(String, u32)>,
    /// Every screen's block, those seen before included.
    pub blocks: std::collections::BTreeMap<String, u32>,
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
    let (given, blocks) = workspaces::blocks(monitors, &names, &layouts.blocks);
    let mut rules = Vec::new();
    let mut targets = Vec::new();
    let mut on = Vec::new();
    for ((m, name), block) in monitors.iter().zip(&names).zip(given) {
        let target = screen::target(name, m);
        let lit = !(lid_off && m.internal()) && output(name, m).enabled;
        rules.push(if lid_off && m.internal() {
            format!("{target}, disable")
        } else {
            output(name, m).rule(&target)
        });
        if lit {
            targets.push((target, block));
            on.push((m.name.clone(), block));
        }
    }
    Plan {
        key,
        layout,
        new,
        lid_closed: lid_off,
        rules,
        workspace_rules: workspaces::rules(layouts.per_screen, &targets),
        on,
        blocks,
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
    for rule in &plan.workspace_rules {
        let _ = writeln!(text, "workspace = {rule}");
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
    if plan.new || plan.blocks != layouts.blocks {
        if plan.new {
            layouts.put(plan.layout.clone());
        }
        layouts.blocks.clone_from(&plan.blocks);
        // Unremembered is still applied: the screens work this time.
        if let Err(e) = layouts.save(&path) {
            eprintln!("alpymist displays: {e}");
        }
    }
    // The lid closing or opening, as far as the panel goes: on and to go
    // off, or off and to come on. The rules are in the screens' order.
    let panel = monitors.iter().position(Monitor::internal);
    let to_go_off = |i: usize| plan.rules.get(i).is_some_and(|r| r.ends_with(", disable"));
    let closing = panel
        .filter(|&i| !monitors[i].disabled && to_go_off(i))
        .map(|i| lid::keep(&monitors[i], &hypr::workspaces().unwrap_or_default()));
    let opening = panel.is_some_and(|i| monitors[i].disabled && !to_go_off(i));
    // Workspace rules cannot be taken back one at a time: when they change,
    // Hyprland reads them all again from the file.
    let text = conf(&plan);
    let old = std::fs::read_to_string(conf_path()).unwrap_or_default();
    let workspace_lines = |t: &str| {
        t.lines()
            .filter(|l| l.starts_with("workspace ="))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let reload = workspace_lines(&old) != workspace_lines(&text);
    write(&conf_path(), &text)?;
    hypr::apply(&plan.rules)?;
    if reload {
        hypr::reload()?;
    }
    // Each screen's workspaces back on it, when it is on: after a dock, an
    // undock, or the lid opening. With one set for every screen, only what
    // the laptop had when the lid closed goes back to it.
    if layouts.per_screen {
        let spaces = hypr::workspaces().unwrap_or_default();
        hypr::dispatch(&workspaces::homes(&spaces, &plan.on))?;
        let now = hypr::monitors().unwrap_or_default();
        hypr::dispatch(&workspaces::show_own(&now, &plan.on))?;
    } else if opening && let Some(kept) = lid::take() {
        hypr::dispatch(&lid::opened(&kept))?;
    }
    if let Some(kept) = closing {
        lid::save(&kept);
        hypr::dispatch(&lid::closed(&kept))?;
    }
    Ok(plan)
}

/// Super+`n`, or with Shift: go to workspace `n`, 1 to 9, of the screen that
/// has the focus — or take the window there too. With one set for every
/// screen, workspace `n`, wherever it is.
///
/// # Errors
/// `n` is not 1 to 9, or Hyprland could not be asked or told.
pub fn switch(n: u32, take_window: bool) -> Result<(), String> {
    if !(1..=9).contains(&n) {
        return Err("workspaces are 1 to 9".into());
    }
    let monitors = screen::parse(&hypr::request("j/monitors all")?)?;
    let layouts = Layouts::load(&layout::path());
    let id = focused_id(n, &monitors, &layouts);
    let verb = if take_window {
        "movetoworkspace"
    } else {
        "workspace"
    };
    hypr::request(&format!("dispatch {verb} {id}")).map(drop)
}

/// Workspace `n` of the screen with the focus, as Hyprland numbers it.
#[must_use]
pub fn focused_id(n: u32, monitors: &[Monitor], layouts: &Layouts) -> i32 {
    let plain = i32::try_from(n).unwrap_or(1);
    if !layouts.per_screen {
        return plain;
    }
    let names = screen::names(monitors);
    let Some(i) = monitors.iter().position(|m| m.focused) else {
        return plain;
    };
    let block = layouts
        .blocks
        .get(&names[i])
        .copied()
        .or_else(|| monitors[i].internal().then_some(0));
    block.map_or(plain, |b| workspaces::id(b, n))
}

/// What of `layout` the screens did not take, as Hyprland has them now: a
/// mode a screen refused, which Hyprland answers by keeping the one it had,
/// or a scale it would not use. Said for a person, one sentence a screen.
#[must_use]
pub fn missed(layout: &Layout, monitors: &[Monitor]) -> Vec<String> {
    let names = screen::names(monitors);
    let mut missed = Vec::new();
    for (m, name) in monitors.iter().zip(&names) {
        let Some(o) = layout.outputs.iter().find(|o| &o.screen == name) else {
            continue;
        };
        if o.enabled == m.disabled {
            missed.push(format!(
                "{} did not turn {}.",
                m.name,
                if o.enabled { "on" } else { "off" }
            ));
            continue;
        }
        if !o.enabled {
            continue;
        }
        if let (Some((w, h)), Some(rate)) = (
            arrange::pixels(&o.mode),
            o.mode
                .split_once('@')
                .and_then(|(_, r)| r.parse::<f64>().ok()),
        ) && (w != m.width || h != m.height || (rate - m.refresh_rate).abs() > 1.0)
        {
            missed.push(format!(
                "{} would not show {w} × {h} at {rate:.0} Hz, and stayed at {} × {} at {:.0} Hz.",
                m.name, m.width, m.height, m.refresh_rate
            ));
        }
        if (o.scale - m.scale).abs() > 0.01 {
            missed.push(format!(
                "{} would not take a scale of {:.0} %, and is at {:.0} %.",
                m.name,
                o.scale * 100.0,
                m.scale * 100.0
            ));
        }
    }
    missed
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
    fn a_mode_the_screen_would_not_show_is_noticed() {
        let now = [screen("Virtual-1", "QEMU")];
        let mut asked = crate::layout::Layout::current(&now);
        assert!(super::missed(&asked, &now).is_empty(), "as it is");
        asked.outputs[0].mode = "3840x2160@60.00".into();
        asked.outputs[0].scale = 1.25;
        let missed = super::missed(&asked, &now);
        assert_eq!(missed.len(), 2, "{missed:?}");
        assert!(missed[0].contains("stayed at 1920 × 1080"), "{missed:?}");
        asked.outputs[0].mode = "preferred".into();
        asked.outputs[0].scale = 1.0;
        assert!(
            super::missed(&asked, &now).is_empty(),
            "preferred is whatever it is"
        );
    }

    #[test]
    fn super_and_a_number_is_that_workspace_of_the_screen_with_the_focus() {
        let mut desk = [screen("eDP-1", "Panel"), screen("DP-3", "Samsung A")];
        desk[1].focused = true;
        let mut layouts = Layouts::default();
        layouts.blocks.insert("Panel".into(), 0);
        layouts.blocks.insert("Samsung A".into(), 1);
        assert_eq!(super::focused_id(4, &desk, &layouts), 14);
        desk[1].focused = false;
        desk[0].focused = true;
        assert_eq!(super::focused_id(4, &desk, &layouts), 4);
        layouts.per_screen = false;
        desk[1].focused = true;
        assert_eq!(super::focused_id(4, &desk, &layouts), 4, "one set for all");
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

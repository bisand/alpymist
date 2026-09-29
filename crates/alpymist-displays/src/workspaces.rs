//! Workspaces for each screen: its own 1 to 9, as on a Mac (ADR 0015).
//!
//! Hyprland has one set of workspaces for every screen, and Super+4 goes to
//! workspace 4 wherever it happens to be, the pointer with it. So each
//! screen is given a block of Hyprland's workspaces instead — the laptop's
//! own screen 1 to 9, the next screen 11 to 19, the one after 21 to 29 — named
//! 1 to 9 in the bar, and Super+1 to Super+9 go to the one with that name on
//! the screen the pointer is on. A laptop on its own is exactly as before.
//!
//! Which block is a screen's is kept with the layouts, by the screen's make,
//! model and serial, so a screen has the same one whatever port it is on and
//! whenever it comes back. And because a workspace's number says whose it
//! is, nothing has to be remembered about where one was: when a screen goes,
//! Hyprland moves its workspaces to the screens left, and when it comes
//! back, or the lid opens, each is sent home.

use crate::hypr::Workspace;
use crate::screen::Monitor;
use std::collections::BTreeMap;

/// Workspaces each screen keeps however few are in use, so the bar always
/// has these to show, as it had before.
pub const KEPT: u32 = 5;

/// The most blocks: ten, 1 to 99.
const BLOCKS: u32 = 10;

/// Workspace `n`, 1 to 9, of the screen with block `block`.
#[must_use]
pub fn id(block: u32, n: u32) -> i32 {
    i32::try_from(block * 10 + n).unwrap_or(1)
}

/// Whose workspace `id` is: its block, or none for a special workspace or
/// a number that is no screen's.
#[must_use]
pub fn block_of(id: i32) -> Option<u32> {
    let id = u32::try_from(id).ok()?;
    (id % 10 != 0).then_some(id / 10)
}

/// Each screen's block, in the screens' order, and the blocks now known,
/// with any screen seen for the first time given one: the laptop's own
/// screen block 0, so its workspaces are 1 to 9 as they always were; any
/// other the lowest free, block 0 too on a computer with no screen of its
/// own. When every block has been given out, one of a screen not connected
/// is given again.
#[must_use]
pub fn blocks(
    monitors: &[Monitor],
    names: &[String],
    known: &BTreeMap<String, u32>,
) -> (Vec<u32>, BTreeMap<String, u32>) {
    let mut known = known.clone();
    let mut order: Vec<usize> = (0..monitors.len()).collect();
    order.sort_by_key(|&i| !monitors[i].internal());
    let mut given = vec![0; monitors.len()];
    for i in order {
        let name = &names[i];
        if let Some(&b) = known.get(name) {
            given[i] = b;
            continue;
        }
        let b = if monitors[i].internal() {
            0
        } else {
            let here: Vec<&String> = names.iter().collect();
            (0..BLOCKS)
                .find(|b| !known.values().any(|v| v == b))
                .or_else(|| {
                    (0..BLOCKS).find(|b| !known.iter().any(|(n, v)| v == b && here.contains(&n)))
                })
                .unwrap_or(BLOCKS - 1)
        };
        // A block given again is no longer the absent screen's.
        known.retain(|_, v| *v != b || monitors[i].internal());
        known.insert(name.clone(), b);
        given[i] = b;
    }
    (given, known)
}

/// The workspace rules for the screens that are on: each one's block on it,
/// named 1 to 9, the first [`KEPT`] kept. With one set for every screen,
/// only that the first [`KEPT`] are kept.
#[must_use]
pub fn rules(per_screen: bool, on: &[(String, u32)]) -> Vec<String> {
    if !per_screen {
        return (1..=KEPT)
            .map(|n| format!("{n}, persistent:true"))
            .collect();
    }
    let mut rules = Vec::new();
    for (target, block) in on {
        for n in 1..=9 {
            let kept = if n <= KEPT { ", persistent:true" } else { "" };
            rules.push(format!(
                "{}, monitor:{target}, defaultName:{n}{kept}",
                id(*block, n)
            ));
        }
    }
    rules
}

/// Where to send each workspace that is not on its own screen, when its own
/// screen is on: `(connector, block)` for each screen that is.
#[must_use]
pub fn homes(workspaces: &[Workspace], on: &[(String, u32)]) -> Vec<String> {
    workspaces
        .iter()
        .filter_map(|w| {
            let block = block_of(w.id)?;
            let (home, _) = on.iter().find(|(_, b)| *b == block)?;
            (w.monitor != *home).then(|| format!("moveworkspacetomonitor {} {home}", w.id))
        })
        .collect()
}

/// For a screen that is on and showing a workspace not its own — Hyprland
/// makes one up, 10 or 20, for a screen whose last one was sent home — its
/// own first one instead; then the focus back where it was.
#[must_use]
pub fn show_own(monitors: &[Monitor], on: &[(String, u32)]) -> Vec<String> {
    let mut commands = Vec::new();
    for (connector, block) in on {
        let Some(m) = monitors.iter().find(|m| &m.name == connector) else {
            continue;
        };
        if block_of(m.active_workspace.id) != Some(*block) {
            commands.push(format!("focusmonitor {connector}"));
            commands.push(format!("workspace {}", id(*block, 1)));
        }
    }
    if !commands.is_empty()
        && let Some(focused) = monitors.iter().find(|m| m.focused)
    {
        commands.push(format!("focusmonitor {}", focused.name));
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::{block_of, blocks, homes, id, rules, show_own};
    use crate::hypr::Workspace;
    use crate::screen::Monitor;
    use std::collections::BTreeMap;

    fn m(name: &str) -> Monitor {
        Monitor {
            name: name.into(),
            ..Monitor::default()
        }
    }

    #[test]
    fn the_laptop_keeps_1_to_9_and_each_screen_gets_its_own_block() {
        let monitors = [m("DP-3"), m("eDP-1"), m("DP-5")];
        let names = [
            "Samsung A".to_owned(),
            "Panel".to_owned(),
            "Samsung B".to_owned(),
        ];
        let (given, known) = blocks(&monitors, &names, &BTreeMap::new());
        assert_eq!(given, [1, 0, 2]);
        // Back another day, on other ports, in another order: the same.
        let again = [m("DP-5"), m("eDP-1"), m("DP-3")];
        let names = [
            "Samsung A".to_owned(),
            "Panel".to_owned(),
            "Samsung B".to_owned(),
        ];
        assert_eq!(blocks(&again, &names, &known).0, [1, 0, 2]);
        // A desktop computer's first screen has 1 to 9.
        let (given, _) = blocks(&[m("DP-1")], &["LG".to_owned()], &BTreeMap::new());
        assert_eq!(given, [0]);
    }

    #[test]
    fn a_workspace_says_whose_it_is() {
        assert_eq!((id(0, 4), id(2, 1)), (4, 21));
        assert_eq!(block_of(4), Some(0));
        assert_eq!(block_of(27), Some(2));
        assert_eq!(block_of(20), None);
        assert_eq!(block_of(-98), None, "special workspaces are no screen's");
    }

    #[test]
    fn each_screen_that_is_on_gets_its_workspaces_named_1_to_9() {
        let on = [("desc:Panel".to_owned(), 0), ("DP-5".to_owned(), 2)];
        let r = rules(true, &on);
        assert_eq!(r.len(), 18);
        assert_eq!(
            r[0],
            "1, monitor:desc:Panel, defaultName:1, persistent:true"
        );
        assert_eq!(r[9], "21, monitor:DP-5, defaultName:1, persistent:true");
        assert_eq!(r[17], "29, monitor:DP-5, defaultName:9");
        assert_eq!(rules(false, &on)[0], "1, persistent:true");
    }

    #[test]
    fn a_screen_left_showing_a_made_up_workspace_shows_its_own() {
        let screen = |name: &str, shown, focused| Monitor {
            name: name.into(),
            active_workspace: crate::screen::Shown { id: shown },
            focused,
            ..Monitor::default()
        };
        let monitors = [
            screen("eDP-1", 2, true),
            screen("DP-3", 10, false),
            screen("DP-5", 23, false),
        ];
        let on = [
            ("eDP-1".to_owned(), 0),
            ("DP-3".to_owned(), 1),
            ("DP-5".to_owned(), 2),
        ];
        assert_eq!(
            show_own(&monitors, &on),
            ["focusmonitor DP-3", "workspace 11", "focusmonitor eDP-1"]
        );
    }

    #[test]
    fn workspaces_go_home_when_their_screen_is_on() {
        let w = |id, monitor: &str| Workspace {
            id,
            monitor: monitor.into(),
        };
        // Undocked, everything went to the laptop; docked again.
        let spaces = [
            w(1, "eDP-1"),
            w(12, "eDP-1"),
            w(21, "eDP-1"),
            w(-98, "eDP-1"),
        ];
        let on = [("eDP-1".to_owned(), 0), ("DP-3".to_owned(), 1)];
        assert_eq!(
            homes(&spaces, &on),
            ["moveworkspacetomonitor 12 DP-3"],
            "21's screen is not here: it stays"
        );
    }
}

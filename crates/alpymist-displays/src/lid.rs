//! What the laptop's screen was showing when the lid closed, so what was on
//! it is in sight on another screen at once, and goes back when the lid
//! opens.
//!
//! Hyprland moves a screen's workspaces to another when it goes off, but
//! leaves that screen showing the workspace it had: whatever was on the
//! laptop is then one Super+number away and nowhere to be seen. So the
//! workspace the laptop showed is brought into view where it went, and
//! every workspace the laptop had is remembered for this session — in the
//! runtime directory, gone at the next boot — and moved back when the lid
//! opens.

use crate::hypr::Workspace;
use crate::screen::Monitor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the laptop's screen had.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// Its connector.
    pub panel: String,
    /// The workspaces on it.
    pub workspaces: Vec<i32>,
    /// The one it showed.
    pub shown: i32,
}

/// What `panel` has, of `workspaces`, before it goes off. Special
/// workspaces, below 0, follow the focus on their own and are left be.
#[must_use]
pub fn keep(panel: &Monitor, workspaces: &[Workspace]) -> Kept {
    Kept {
        panel: panel.name.clone(),
        workspaces: workspaces
            .iter()
            .filter(|w| w.monitor == panel.name && w.id > 0)
            .map(|w| w.id)
            .collect(),
        shown: panel.active_workspace.id,
    }
}

/// With the panel off: show what it showed, where Hyprland put it.
#[must_use]
pub fn closed(kept: &Kept) -> Vec<String> {
    (kept.shown > 0)
        .then(|| format!("workspace {}", kept.shown))
        .into_iter()
        .collect()
}

/// With the panel on again: its workspaces back on it, and what it showed
/// in sight there. Those that no longer exist, Hyprland having let an empty
/// one go, are passed over by Hyprland itself.
#[must_use]
pub fn opened(kept: &Kept) -> Vec<String> {
    let mut commands: Vec<String> = kept
        .workspaces
        .iter()
        .map(|id| format!("moveworkspacetomonitor {id} {}", kept.panel))
        .collect();
    if kept.shown > 0 {
        commands.push(format!("workspace {}", kept.shown));
    }
    commands
}

/// Where it is kept for the session.
#[must_use]
pub fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join("alpymist-lid.json"))
}

/// Keep it until the lid opens.
pub fn save(kept: &Kept) {
    if let Some(path) = path()
        && let Ok(text) = serde_json::to_string(kept)
    {
        let _ = std::fs::write(path, text);
    }
}

/// Take what was kept, if anything was: it is gone after.
#[must_use]
pub fn take() -> Option<Kept> {
    let path = path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::{closed, keep, opened};
    use crate::hypr::Workspace;
    use crate::screen::{Monitor, Shown};

    #[test]
    fn what_the_laptop_showed_is_shown_elsewhere_and_goes_back() {
        let panel = Monitor {
            name: "eDP-1".into(),
            active_workspace: Shown { id: 1 },
            ..Monitor::default()
        };
        let on = |id, monitor: &str| Workspace {
            id,
            monitor: monitor.into(),
            ..Workspace::default()
        };
        let kept = keep(
            &panel,
            &[
                on(1, "eDP-1"),
                on(4, "eDP-1"),
                on(2, "DP-3"),
                on(-98, "eDP-1"),
            ],
        );
        assert_eq!(kept.workspaces, [1, 4], "special workspaces are left be");
        assert_eq!(closed(&kept), ["workspace 1"]);
        assert_eq!(
            opened(&kept),
            [
                "moveworkspacetomonitor 1 eDP-1",
                "moveworkspacetomonitor 4 eDP-1",
                "workspace 1"
            ]
        );
    }
}

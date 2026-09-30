//! Which screen a program that draws straight to the display should use:
//! the splash, the installer and the login screen, which run before
//! Hyprland and know nothing of its layouts.
//!
//! The laptop's own screen, unless its lid is closed and another screen is
//! connected. A closed lid still reports its panel connected, so a laptop on
//! a dock with its lid shut would otherwise draw behind the lid, and leave
//! the dock's screens dark.

use std::path::Path;

/// Where ACPI says whether a lid is open.
const LID: &str = "/proc/acpi/button/lid";
/// Where the kernel lists each connector and whether a screen is on it.
const DRM: &str = "/sys/class/drm";

/// The kind of screen to prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    /// Whichever the display library prefers: the laptop's own first.
    Any,
    /// A screen on `DP`, as a dock's are.
    DisplayPort,
    /// An HDMI screen.
    Hdmi,
    /// A DVI screen.
    Dvi,
    /// A VGA screen.
    Vga,
}

/// The screen to prefer on this machine now.
#[must_use]
pub fn prefer() -> Prefer {
    prefer_in(Path::new(LID), Path::new(DRM))
}

/// The screen to prefer, from the lid's state under `lid` and the
/// connectors under `drm`: the first connected screen that is not the
/// laptop's own, by connector name, when the lid is closed.
#[must_use]
pub fn prefer_in(lid: &Path, drm: &Path) -> Prefer {
    if !closed(lid) {
        return Prefer::Any;
    }
    let Ok(dir) = std::fs::read_dir(drm) else {
        return Prefer::Any;
    };
    let mut outside: Vec<(String, Prefer)> = dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // card1-DP-3; a bare card1 is the device, not a connector.
            let (_, connector) = name.split_once('-')?;
            let connected = std::fs::read_to_string(entry.path().join("status"))
                .is_ok_and(|s| s.trim() == "connected");
            let kind = match connector.split('-').next().unwrap_or("") {
                "DP" => Prefer::DisplayPort,
                "HDMI" => Prefer::Hdmi,
                "DVI" => Prefer::Dvi,
                "VGA" => Prefer::Vga,
                _ => return None,
            };
            connected.then(|| (connector.to_owned(), kind))
        })
        .collect();
    // The same one every time, whatever order the directory lists them in.
    outside.sort_by(|a, b| a.0.cmp(&b.0));
    outside.first().map_or(Prefer::Any, |(_, k)| *k)
}

/// Whether any lid ACPI knows of is closed. A machine without one has none
/// to close.
fn closed(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|e| {
            std::fs::read_to_string(e.path().join("state"))
                .is_ok_and(|s| s.split_whitespace().last() == Some("closed"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{Prefer, prefer_in};

    #[test]
    fn a_closed_lid_sends_the_picture_to_the_dock() {
        let dir = std::env::temp_dir().join(format!("alpymist-lid-drm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (lid, drm) = (dir.join("lid"), dir.join("drm"));
        for (name, status) in [
            ("card1-eDP-1", "connected"),
            ("card1-DP-5", "connected"),
            ("card1-HDMI-A-1", "disconnected"),
        ] {
            std::fs::create_dir_all(drm.join(name)).unwrap();
            std::fs::write(drm.join(name).join("status"), format!("{status}\n")).unwrap();
        }
        std::fs::create_dir_all(drm.join("card1")).unwrap();
        std::fs::create_dir_all(lid.join("LID")).unwrap();
        std::fs::write(lid.join("LID/state"), "state:      open\n").unwrap();
        assert_eq!(prefer_in(&lid, &drm), Prefer::Any, "open: the laptop's own");
        std::fs::write(lid.join("LID/state"), "state:      closed\n").unwrap();
        assert_eq!(prefer_in(&lid, &drm), Prefer::DisplayPort);
        std::fs::remove_dir_all(drm.join("card1-DP-5")).unwrap();
        assert_eq!(
            prefer_in(&lid, &drm),
            Prefer::Any,
            "closed with nothing else connected: the laptop's own still"
        );
        assert_eq!(prefer_in(&dir.join("none"), &drm), Prefer::Any, "no lid");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

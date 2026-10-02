//! What Hyprland is told because of the hardware it is on.
//!
//! `alpymist session` writes `~/.config/alpymist/hypr/hardware.conf` before
//! Hyprland starts, and the packaged `hyprland-security.conf` sources it. It
//! is written again at every login from the machine as it is then, so a disk
//! moved to another machine, or a card changed, is followed; and it comes
//! before anything in the account's own `hyprland.conf`, so a line there
//! still wins.
//!
//! It holds mends for drivers, each for every machine with that driver and
//! none for one machine alone:
//!
//! - **nouveau draws no pointer.** On a 2009 `MacBook` Pro (`GeForce` 9400M)
//!   the pointer moved and clicked and was not on the screen: the driver
//!   takes Hyprland's cursor plane and shows nothing on it. Hyprland draws
//!   the pointer itself there, which costs a little and is always seen.

use std::path::{Path, PathBuf};

/// The file, beside the screens' and the settings'.
#[must_use]
pub fn conf_path() -> PathBuf {
    alpymist_displays::conf_path().with_file_name("hardware.conf")
}

/// What the file holds for a machine whose graphics cards have these drivers.
#[must_use]
pub fn conf(drivers: &[String]) -> String {
    let mut text = String::from(
        "# Written by Alpymist at every login, for the hardware found then.\n\
         # Not for editing: a line in hyprland.conf comes after this and wins.\n",
    );
    if drivers.iter().any(|d| d == "nouveau") {
        text.push_str(
            "\n# nouveau shows nothing on the cursor plane: Hyprland draws the pointer.\n\
             cursor {\n    no_hardware_cursors = true\n}\n",
        );
    }
    text
}

/// Write the file for this machine, where it is not already that.
///
/// # Errors
/// It could not be written. Hyprland refuses a missing file it sources.
pub fn prepare(path: &Path) -> Result<(), String> {
    let text = conf(&alpymist_hwprobe::drm_drivers());
    if std::fs::read_to_string(path).is_ok_and(|was| was == text) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::conf;

    #[test]
    fn nouveau_has_the_pointer_drawn_by_hyprland_and_nothing_else_does() {
        let on = conf(&["nouveau".into()]);
        assert!(on.contains("cursor {\n    no_hardware_cursors = true\n}"));
        // Beside another card, as on a laptop with two.
        assert!(conf(&["i915".into(), "nouveau".into()]).contains("no_hardware_cursors"));
        for other in [vec![], vec!["i915".to_string()], vec!["amdgpu".to_string()]] {
            let text = conf(&other);
            assert!(!text.contains("cursor"), "{other:?}: {text}");
            assert!(
                text.lines().all(|l| l.is_empty() || l.starts_with('#')),
                "nothing but comments where nothing needs mending"
            );
        }
    }

    /// Hyprland refuses a file it is told to source and cannot find, so the
    /// packaged configuration and the session must name the same one.
    #[test]
    fn the_packaged_configuration_sources_the_file_the_session_writes() {
        let security = include_str!("../../../desktop/hypr/hyprland-security.conf");
        assert!(
            security
                .lines()
                .any(|l| l == "source = ~/.config/alpymist/hypr/hardware.conf"),
            "hyprland-security.conf does not source hardware.conf"
        );
        assert!(super::conf_path().ends_with("alpymist/hypr/hardware.conf"));
    }
}

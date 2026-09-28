//! Super+C and Super+V: copy and paste in the focused window, whatever it
//! is.
//!
//! Terminals copy on Ctrl+Shift+C and paste on Ctrl+Shift+V, since Ctrl+C
//! interrupts and Ctrl+V quotes the next key; everything else uses Ctrl+C
//! and Ctrl+V. Which the focused window is, Hyprland says by its class, and
//! the installed desktop entries say which classes are terminals, so a
//! terminal installed later is known without a list here to keep up.

use alpymist_core::defaults::{self, Places};
use std::process::{Command, Stdio};

/// Which key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Copy what is selected.
    Copy,
    /// Paste what was copied.
    Paste,
}

/// The shortcut to send, as `sendshortcut` takes it.
#[must_use]
pub fn shortcut(which: Which, terminal: bool) -> &'static str {
    match (which, terminal) {
        (Which::Copy, true) => "CTRL SHIFT, C",
        (Which::Copy, false) => "CTRL, C",
        (Which::Paste, true) => "CTRL SHIFT, V",
        (Which::Paste, false) => "CTRL, V",
    }
}

/// Send the focused window the key it understands.
///
/// # Errors
/// Hyprland could not be asked, or refused.
pub fn send(which: Which) -> Result<(), String> {
    let out = Command::new("hyprctl")
        .args(["-j", "activewindow"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("hyprctl: {e}"))?;
    let window: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let (Some(address), Some(class)) = (
        window.get("address").and_then(|v| v.as_str()),
        window.get("class").and_then(|v| v.as_str()),
    ) else {
        // Nothing has the focus: there is nothing to copy from or paste into.
        return Ok(());
    };
    let places = Places::current();
    let terminal = defaults::is_terminal_window(class, &places.applications());
    let arg = format!("{}, address:{address}", shortcut(which, terminal));
    let status = Command::new("hyprctl")
        .args(["dispatch", "sendshortcut", &arg])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("hyprctl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Hyprland would not send the shortcut".into())
    }
}

/// Put an entry from the history back on the clipboard, and paste it into
/// the focused window.
///
/// # Errors
/// No such entry, or it could not be put on the clipboard.
pub fn restore(id: u64) -> Result<(), String> {
    use std::io::Write as _;
    let (mime, data) = crate::client::get(id)?;
    let mut copy = Command::new("wl-copy")
        .args(["--type", &mime])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("wl-copy: {e}"))?;
    if let Some(mut input) = copy.stdin.take() {
        input
            .write_all(&data)
            .map_err(|e| format!("wl-copy: {e}"))?;
    }
    // wl-copy stays behind to serve the clipboard, and returns once it has
    // what to serve.
    let _ = copy.wait();
    // The picker has just closed, and focus is on its way back to the window
    // it was opened from.
    std::thread::sleep(std::time::Duration::from_millis(150));
    send(Which::Paste)
}

#[cfg(test)]
mod tests {
    use super::{Which, shortcut};

    #[test]
    fn a_terminal_gets_shift_as_well() {
        assert_eq!(shortcut(Which::Copy, true), "CTRL SHIFT, C");
        assert_eq!(shortcut(Which::Paste, false), "CTRL, V");
    }
}

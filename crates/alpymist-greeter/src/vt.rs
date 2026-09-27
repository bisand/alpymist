//! Ctrl+Alt+F1…F12, the way out to a text console.
//!
//! The login screen mutes its console's keyboard so a password cannot reach the
//! terminal underneath, and that also mutes the kernel's own console switching.
//! Without this, a desktop that dies at every login leaves nothing that answers
//! a key: no console to log in on, and no way to find out why.
//!
//! The greeter does the switch itself, by running `chvt`. It needs no
//! privilege: greetd makes the greeter's VT its controlling terminal, and the
//! kernel lets a process switch away from its own terminal. While another
//! console is showing, the greeter lets go of the display and stops reading
//! the keyboard, so the text console can draw and what is typed there reaches
//! only it; switched back, it takes both again.

use denise::input::{ElementState, InputEvent, KeyCode, Modifiers};

/// Where the kernel says which VT is showing, as `tty7`.
pub const ACTIVE: &str = "/sys/class/tty/tty0/active";

/// The major device number of the virtual terminals.
const VT_MAJOR: u32 = 4;

/// The console Ctrl+Alt+F`n` asks for, if this event is that key.
#[must_use]
pub fn console_for(event: &InputEvent) -> Option<u32> {
    let InputEvent::Key {
        code,
        state: ElementState::Down,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if !modifiers.contains(Modifiers::CTRL | Modifiers::ALT) {
        return None;
    }
    Some(match code {
        KeyCode::F1 => 1,
        KeyCode::F2 => 2,
        KeyCode::F3 => 3,
        KeyCode::F4 => 4,
        KeyCode::F5 => 5,
        KeyCode::F6 => 6,
        KeyCode::F7 => 7,
        KeyCode::F8 => 8,
        KeyCode::F9 => 9,
        KeyCode::F10 => 10,
        KeyCode::F11 => 11,
        KeyCode::F12 => 12,
        _ => return None,
    })
}

/// The VT a process's controlling terminal is, from its `/proc/PID/stat`.
///
/// `None` when it has none, or it is not a VT: a greeter started from SSH or
/// in a window, where there is nothing to switch.
#[must_use]
pub fn own_vt(stat: &str) -> Option<u32> {
    // The command name, in parentheses, may itself hold spaces and
    // parentheses; the fields after the last `)` are state, ppid, pgrp,
    // session and then tty_nr.
    let (_, rest) = stat.rsplit_once(')')?;
    let tty_nr: u32 = rest.split_whitespace().nth(4)?.parse().ok()?;
    let major = (tty_nr >> 8) & 0xfff;
    let minor = (tty_nr & 0xff) | ((tty_nr >> 12) & 0xfff00);
    (major == VT_MAJOR && (1..=63).contains(&minor)).then_some(minor)
}

/// The VT showing now, from the contents of [`ACTIVE`].
#[must_use]
pub fn active_vt(active: &str) -> Option<u32> {
    active.trim().strip_prefix("tty")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: Modifiers) -> InputEvent {
        InputEvent::Key {
            code,
            state: ElementState::Down,
            modifiers,
            repeat: false,
        }
    }

    #[test]
    fn ctrl_alt_and_a_function_key_names_that_console() {
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        assert_eq!(console_for(&key(KeyCode::F2, ctrl_alt)), Some(2));
        assert_eq!(console_for(&key(KeyCode::F12, ctrl_alt)), Some(12));
        assert_eq!(
            console_for(&key(KeyCode::F2, ctrl_alt | Modifiers::SHIFT)),
            Some(2),
            "a held Shift does not get in the way"
        );
    }

    #[test]
    fn a_function_key_without_both_ctrl_and_alt_is_not_a_switch() {
        assert_eq!(console_for(&key(KeyCode::F2, Modifiers::NONE)), None);
        assert_eq!(console_for(&key(KeyCode::F2, Modifiers::ALT)), None);
        assert_eq!(console_for(&key(KeyCode::F2, Modifiers::CTRL)), None);
        assert_eq!(
            console_for(&key(KeyCode::Enter, Modifiers::CTRL | Modifiers::ALT)),
            None
        );
    }

    #[test]
    fn the_controlling_terminal_is_read_past_an_awkward_command_name() {
        // tty7 is major 4, minor 7: 4 << 8 | 7 = 1031.
        let stat = "5027 (alpy (greeter) x) S 2504 5027 5027 1031 5027 4194560 0 0";
        assert_eq!(own_vt(stat), Some(7));
    }

    #[test]
    fn no_terminal_or_a_pseudo_terminal_is_no_vt() {
        assert_eq!(own_vt("12 (sh) S 1 12 12 0 -1 0"), None);
        // /dev/pts/3 is major 136.
        assert_eq!(own_vt("12 (sh) S 1 12 12 34819 -1 0"), None);
        assert_eq!(own_vt("garbage"), None);
    }

    #[test]
    fn the_active_console_is_read_as_the_kernel_writes_it() {
        assert_eq!(active_vt("tty7\n"), Some(7));
        assert_eq!(active_vt("tty2"), Some(2));
        assert_eq!(active_vt(""), None);
    }
}

//! The keyboard: a keymap from the compositor, and libxkbcommon to read it.
//!
//! The compositor sends keys by number and the layout as a file. What a key
//! means — which letter with which modifiers held, which accent a dead key
//! leaves on the next — is libxkbcommon's to say, as it is for every other
//! Wayland client.

use std::fs::File;
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;

use xkbcommon::xkb;

pub use xkb::Keysym;

/// The most keymap read. The largest layouts are a few hundred kilobytes.
const MOST: u32 = 8 << 20;

/// A key pressed, or held long enough to repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Which key, by what the layout calls it.
    pub keysym: Keysym,
    /// What it types, when it types anything.
    pub utf8: Option<String>,
}

/// The modifiers held or locked.
// Five keys that are each down or not: there is no state machine in that.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Ctrl.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Caps Lock.
    pub caps_lock: bool,
    /// The logo key: Super.
    pub logo: bool,
}

pub(crate) struct Keyboard {
    context: xkb::Context,
    state: Option<xkb::State>,
    compose: Option<xkb::compose::State>,
}

impl Keyboard {
    pub(crate) fn new() -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let compose = compose(&context);
        Self {
            context,
            state: None,
            compose,
        }
    }

    /// Take the layout the compositor sent.
    ///
    /// Read, not mapped: the protocol asks for a private mapping where one is
    /// made, and reading needs none.
    pub(crate) fn keymap(&mut self, fd: OwnedFd, size: u32) {
        let mut text = vec![0u8; size.min(MOST) as usize];
        if File::from(fd).read_exact_at(&mut text, 0).is_err() {
            return;
        }
        // It ends in a NUL, as a C string does.
        while text.last() == Some(&0) {
            text.pop();
        }
        let Ok(text) = String::from_utf8(text) else {
            return;
        };
        self.install(text);
    }

    fn install(&mut self, text: String) {
        if let Some(keymap) = xkb::Keymap::new_from_string(
            &self.context,
            text,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        ) {
            self.state = Some(xkb::State::new(&keymap));
        }
    }

    /// A key went down: what it is, and what it types.
    pub(crate) fn pressed(&mut self, raw: u32) -> Option<KeyEvent> {
        let state = self.state.as_ref()?;
        let code = code(raw);
        let keysym = state.key_get_one_sym(code);
        let utf8 = match self.compose.as_mut() {
            Some(compose) => match compose.feed(keysym) {
                xkb::FeedResult::Ignored => None,
                xkb::FeedResult::Accepted => match compose.status() {
                    xkb::Status::Composed => compose.utf8(),
                    xkb::Status::Nothing => Some(state.key_get_utf8(code)),
                    // In the middle of a sequence, or one that came to
                    // nothing: the key types nothing itself.
                    _ => None,
                },
            },
            None => Some(state.key_get_utf8(code)),
        };
        Some(KeyEvent {
            keysym,
            utf8: utf8.filter(|text| !text.is_empty()),
        })
    }

    /// What a key held types now, after the modifiers changed under it.
    pub(crate) fn text(&self, raw: u32) -> Option<String> {
        let text = self.state.as_ref()?.key_get_utf8(code(raw));
        (!text.is_empty()).then_some(text)
    }

    /// Whether the layout has this key repeat while held. Shift does not.
    pub(crate) fn repeats(&self, raw: u32) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.get_keymap().key_repeats(code(raw)))
    }

    pub(crate) fn modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> Option<Modifiers> {
        let state = self.state.as_mut()?;
        state.update_mask(depressed, latched, locked, 0, 0, group);
        let on = |name: &str| state.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE);
        Some(Modifiers {
            ctrl: on(xkb::MOD_NAME_CTRL),
            alt: on(xkb::MOD_NAME_ALT),
            shift: on(xkb::MOD_NAME_SHIFT),
            caps_lock: on(xkb::MOD_NAME_CAPS),
            logo: on(xkb::MOD_NAME_LOGO),
        })
    }
}

/// The protocol counts keys from zero and X from eight.
fn code(raw: u32) -> xkb::Keycode {
    xkb::Keycode::new(raw + 8)
}

/// Dead keys and the Compose key, for the locale the session is in.
fn compose(context: &xkb::Context) -> Option<xkb::compose::State> {
    let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    let locale = var("LC_ALL")
        .or_else(|| var("LC_CTYPE"))
        .or_else(|| var("LANG"))
        .unwrap_or_else(|| "C".into());
    let table =
        xkb::compose::Table::new_from_locale(context, &locale, xkb::compose::COMPILE_NO_FLAGS)
            .ok()?;
    Some(xkb::compose::State::new(
        &table,
        xkb::compose::STATE_NO_FLAGS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keyboard with the layout libxkbcommon builds from the system's
    /// rules, where there are any: a builder without xkeyboard-config has
    /// nothing to test against.
    fn keyboard(layout: &str) -> Option<Keyboard> {
        let mut keyboard = Keyboard::new();
        let keymap = xkb::Keymap::new_from_names(
            &keyboard.context,
            "",
            "",
            layout,
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        keyboard.install(keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1));
        keyboard.state.as_ref()?;
        Some(keyboard)
    }

    // From linux/input-event-codes.h.
    const KEY_A: u32 = 30;
    const KEY_ESC: u32 = 1;
    const KEY_LEFTSHIFT: u32 = 42;

    #[test]
    fn a_key_is_what_the_layout_says() {
        let Some(mut keyboard) = keyboard("us") else {
            return;
        };
        let a = keyboard.pressed(KEY_A).unwrap();
        assert_eq!(a.keysym, Keysym::a);
        assert_eq!(a.utf8.as_deref(), Some("a"));
        let esc = keyboard.pressed(KEY_ESC).unwrap();
        assert_eq!(esc.keysym, Keysym::Escape);
        assert!(keyboard.repeats(KEY_A));
        assert!(!keyboard.repeats(KEY_LEFTSHIFT));
    }

    #[test]
    fn modifiers_change_what_a_key_types() {
        let Some(mut keyboard) = keyboard("us") else {
            return;
        };
        let shift = keyboard.modifiers(1, 0, 0, 0).unwrap();
        assert!(shift.shift && !shift.ctrl && !shift.caps_lock);
        assert_eq!(keyboard.pressed(KEY_A).unwrap().utf8.as_deref(), Some("A"));
        assert_eq!(keyboard.text(KEY_A).as_deref(), Some("A"));
        let caps = keyboard.modifiers(0, 0, 2, 0).unwrap();
        assert!(caps.caps_lock && !caps.shift);
        let none = keyboard.modifiers(0, 0, 0, 0).unwrap();
        assert_eq!(none, Modifiers::default());
    }

    #[test]
    fn a_keyboard_with_no_layout_says_nothing() {
        let mut keyboard = Keyboard::new();
        assert!(keyboard.pressed(KEY_A).is_none());
        assert!(keyboard.modifiers(1, 0, 0, 0).is_none());
        assert!(!keyboard.repeats(KEY_A));
        keyboard.install("not a keymap".to_owned());
        assert!(keyboard.pressed(KEY_A).is_none());
    }
}

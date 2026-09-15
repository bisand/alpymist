//! A password that wipes itself.
//!
//! Typed a character at a time into a buffer that never grows — growing
//! would leave the old allocation behind, unwiped — and zeroed when it is
//! cleared, handed on, or dropped. It has no `Debug` or `Display`, so it
//! cannot end up in a log by accident.

use zeroize::Zeroize;

/// The most bytes a password may have.
pub const CAPACITY: usize = 512;

/// A password being typed.
pub struct Secret {
    bytes: Vec<u8>,
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

impl Secret {
    /// An empty password, its whole buffer allocated at once.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(CAPACITY),
        }
    }

    /// Add a character. Returns false, adding nothing, when it would not
    /// fit or is a line break, which the helper's protocol cannot carry.
    pub fn push(&mut self, ch: char) -> bool {
        let mut utf8 = [0u8; 4];
        let encoded = ch.encode_utf8(&mut utf8).as_bytes();
        let fits = self.bytes.len() + encoded.len() <= CAPACITY;
        let ok = fits && ch != '\n' && ch != '\r' && ch != '\0';
        if ok {
            self.bytes.extend_from_slice(encoded);
        }
        utf8.zeroize();
        ok
    }

    /// Remove the last character.
    pub fn pop(&mut self) {
        let Ok(text) = std::str::from_utf8(&self.bytes) else {
            self.clear();
            return;
        };
        let cut = text.char_indices().next_back().map_or(0, |(at, _)| at);
        // Zero what is removed before shortening: truncate does not.
        self.bytes[cut..].zeroize();
        self.bytes.truncate(cut);
    }

    /// Wipe it.
    pub fn clear(&mut self) {
        self.bytes.zeroize();
    }

    /// How many characters it has, for the dots.
    #[must_use]
    pub fn chars(&self) -> usize {
        std::str::from_utf8(&self.bytes).map_or(0, |t| t.chars().count())
    }

    /// Whether nothing has been typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The bytes, to write to the helper.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::{CAPACITY, Secret};

    #[test]
    fn typing_and_erasing_work_by_character() {
        let mut s = Secret::new();
        for ch in "pæss".chars() {
            assert!(s.push(ch));
        }
        assert_eq!(s.chars(), 4);
        s.pop();
        s.pop();
        assert_eq!(s.expose(), "pæ".as_bytes());
        s.pop();
        assert_eq!(s.expose(), b"p");
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn line_breaks_and_overlong_passwords_are_refused() {
        let mut s = Secret::new();
        let buffer = s.bytes.as_ptr();
        assert!(!s.push('\n'));
        for _ in 0..CAPACITY {
            s.push('a');
        }
        assert!(!s.push('b'));
        assert_eq!(s.expose().len(), CAPACITY);
        assert_eq!(
            s.bytes.as_ptr(),
            buffer,
            "never reallocated, so never left a copy behind"
        );
    }
}

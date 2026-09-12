//! Editing a single line of text.
//!
//! The caret is a **character** index, never a byte offset. Every field on the
//! Account screen can legitimately contain `æ ø å é`, which are two bytes each
//! in UTF-8; a byte-indexed caret would sooner or later split one and panic, or
//! worse, silently corrupt the value.
//!
//! Each operation returns the new caret position rather than mutating one, so
//! the caller cannot forget to move it and leave the caret pointing past the
//! end of a string it just shortened.

/// Where character `index` starts, in bytes.
///
/// A caret past the end resolves to the end, which is what makes every
/// operation below safe against a stale caret.
fn byte_at(value: &str, index: usize) -> usize {
    value
        .char_indices()
        .nth(index)
        .map_or(value.len(), |(b, _)| b)
}

/// How many characters the value holds.
#[must_use]
pub fn length(value: &str) -> usize {
    value.chars().count()
}

/// Pull a caret back inside the value.
#[must_use]
pub fn clamp(value: &str, caret: usize) -> usize {
    caret.min(length(value))
}

/// Insert `ch` at the caret, returning where the caret ends up.
#[must_use]
pub fn insert(value: &mut String, caret: usize, ch: char) -> usize {
    let caret = clamp(value, caret);
    let at = byte_at(value, caret);
    value.insert(at, ch);
    caret + 1
}

/// Delete the character before the caret.
#[must_use]
pub fn backspace(value: &mut String, caret: usize) -> usize {
    let caret = clamp(value, caret);
    if caret == 0 {
        return 0;
    }
    let from = byte_at(value, caret - 1);
    let to = byte_at(value, caret);
    value.replace_range(from..to, "");
    caret - 1
}

/// Delete the character under the caret.
#[must_use]
pub fn delete(value: &mut String, caret: usize) -> usize {
    let caret = clamp(value, caret);
    if caret >= length(value) {
        return caret;
    }
    let from = byte_at(value, caret);
    let to = byte_at(value, caret + 1);
    value.replace_range(from..to, "");
    caret
}

/// Move the caret one character left.
#[must_use]
pub fn left(value: &str, caret: usize) -> usize {
    clamp(value, caret).saturating_sub(1)
}

/// Move the caret one character right.
#[must_use]
pub fn right(value: &str, caret: usize) -> usize {
    (clamp(value, caret) + 1).min(length(value))
}

/// Move the caret to the start.
#[must_use]
pub fn home() -> usize {
    0
}

/// Move the caret to the end.
#[must_use]
pub fn end(value: &str) -> usize {
    length(value)
}

/// Render a value for display, masked if it is a secret, with a caret drawn in.
///
/// The caret is a character in the string rather than a drawn rectangle because
/// the text is monospaced and this keeps it correct at any size with no extra
/// measurement — and it still shows up on the built-in bitmap fallback.
#[must_use]
pub fn with_caret(value: &str, caret: usize, secret: bool, focused: bool) -> String {
    let shown: String = if secret {
        "*".repeat(length(value))
    } else {
        value.to_string()
    };
    if !focused {
        return shown;
    }
    let caret = clamp(&shown, caret);
    let at = byte_at(&shown, caret);
    let mut out = shown;
    out.insert(at, '_');
    out
}

#[cfg(test)]
mod tests {
    use super::{backspace, clamp, delete, end, insert, left, length, right, with_caret};

    #[test]
    fn typing_ascii_advances_the_caret() {
        let mut v = String::new();
        let mut c = 0;
        for ch in "andre".chars() {
            c = insert(&mut v, c, ch);
        }
        assert_eq!(v, "andre");
        assert_eq!(c, 5);
    }

    /// The reason the caret counts characters: these are two bytes each.
    #[test]
    fn typing_norwegian_letters_does_not_corrupt_the_value() {
        let mut v = String::new();
        let mut c = 0;
        for ch in "blåbærsyltetøy".chars() {
            c = insert(&mut v, c, ch);
        }
        assert_eq!(v, "blåbærsyltetøy");
        assert_eq!(c, length("blåbærsyltetøy"));
    }

    #[test]
    fn inserting_in_the_middle_of_multibyte_text_lands_between_characters() {
        let mut v = "blbær".to_string();
        let c = insert(&mut v, 2, 'å');
        assert_eq!(v, "blåbær");
        assert_eq!(c, 3);
    }

    #[test]
    fn backspace_removes_a_whole_multibyte_character() {
        let mut v = "blåbær".to_string();
        let c = backspace(&mut v, length("blåbær"));
        assert_eq!(v, "blåbæ");
        assert_eq!(c, length("blåbæ"));
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut v = "andre".to_string();
        assert_eq!(backspace(&mut v, 0), 0);
        assert_eq!(v, "andre");
    }

    #[test]
    fn delete_removes_the_character_under_the_caret() {
        let mut v = "bålet".to_string();
        let c = delete(&mut v, 1);
        assert_eq!(v, "blet");
        assert_eq!(c, 1, "the caret should not move");
    }

    #[test]
    fn delete_at_the_end_does_nothing() {
        let mut v = "andre".to_string();
        assert_eq!(delete(&mut v, 5), 5);
        assert_eq!(v, "andre");
    }

    #[test]
    fn the_caret_cannot_walk_off_either_end() {
        let v = "blåbær";
        assert_eq!(left(v, 0), 0);
        assert_eq!(right(v, length(v)), length(v));
        assert_eq!(end(v), length(v));
        assert_eq!(super::home(), 0);
    }

    /// A caret left over from a longer value must not index past the new one.
    #[test]
    fn a_stale_caret_is_pulled_back_rather_than_panicking() {
        let mut v = "ab".to_string();
        assert_eq!(clamp(&v, 99), 2);
        assert_eq!(backspace(&mut v, 99), 1);
        assert_eq!(v, "a");
        assert_eq!(delete(&mut v, 99), 1);
        assert_eq!(insert(&mut v, 99, 'z'), 2);
        assert_eq!(v, "az");
    }

    #[test]
    fn a_secret_is_shown_as_stars_and_never_as_itself() {
        let shown = with_caret("hunter2", 7, true, true);
        assert!(!shown.contains("hunter2"));
        assert_eq!(shown.matches('*').count(), 7);
    }

    #[test]
    fn a_secret_of_multibyte_characters_masks_one_star_per_character() {
        let shown = with_caret("blåbær", 6, true, false);
        assert_eq!(shown, "******", "one star per character, not per byte");
    }

    #[test]
    fn the_caret_is_only_drawn_on_the_focused_field() {
        assert_eq!(with_caret("andre", 2, false, false), "andre");
        assert_eq!(with_caret("andre", 2, false, true), "an_dre");
    }

    #[test]
    fn the_caret_draws_at_the_end_when_it_is_at_the_end() {
        assert_eq!(with_caret("andre", 5, false, true), "andre_");
    }

    #[test]
    fn the_caret_draws_between_multibyte_characters_correctly() {
        assert_eq!(with_caret("blåbær", 3, false, true), "blå_bær");
    }
}

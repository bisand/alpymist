//! Saturating conversions between screen-space integer types.
//!
//! Screen geometry mixes `u32` sizes with `i32` coordinates constantly. Raw
//! `as` casts would silently wrap on a nonsense value; these saturate instead,
//! so a bad input produces a clamped picture rather than a ridge drawn at
//! negative-two-billion.
//!
//! This is the single place in the crate where raw casts are allowed; every
//! other module goes through these helpers so the lint stays meaningful there.

#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

/// A size as a coordinate, saturating rather than wrapping.
#[must_use]
pub const fn px(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}

/// A coordinate as a size, clamping negatives to zero.
#[must_use]
pub const fn rows(v: i32) -> u32 {
    if v < 0 { 0 } else { v as u32 }
}

/// A `usize` index as a coordinate, saturating rather than wrapping.
#[must_use]
pub fn idx(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{idx, px, rows};

    #[test]
    fn ordinary_values_round_trip() {
        assert_eq!(px(1920), 1920);
        assert_eq!(rows(1080), 1080);
        assert_eq!(idx(640), 640);
    }

    #[test]
    fn negatives_clamp_to_zero_rather_than_wrapping() {
        assert_eq!(rows(-1), 0);
        assert_eq!(rows(i32::MIN), 0);
    }

    #[test]
    fn oversized_values_saturate_rather_than_wrapping() {
        assert_eq!(px(u32::MAX), i32::MAX);
        assert_eq!(idx(usize::MAX), i32::MAX);
    }
}

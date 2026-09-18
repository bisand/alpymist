//! What the screensaver draws, and how it moves — with no pixels in sight.
//!
//! The picture is the same mountains the wallpaper, the splash and the
//! installer draw, composed by [`alpymist_ui::backdrop`] at a fraction of the
//! screen's resolution and then blown up in square blocks. That is the whole
//! of the pixelation: there is no filtering, no second scene and no art
//! asset — one range of mountains, drawn small.
//!
//! What moves is the mist. Each band rises and falls on its own slow cycle and
//! swells and thins as it goes, so the valleys breathe rather than sit still.
//! Nothing else animates: a still picture with moving mist reads as weather,
//! and it is also the cheapest thing on screen to redraw.
//!
//! Everything here is integer arithmetic. Denise's rasteriser makes a point of
//! using no floating point, the machines this targets are the reason, and a
//! sine from a table is both exact across runs and quick on a core without an
//! FPU worth the name.

use denise::geom::Size;

/// A sine wave's amplitude, as the scale [`sine`] returns.
pub const UNIT: i32 = 1000;

/// A quarter turn of sine, in thousandths, at one degree per step.
///
/// A table rather than an approximation: 91 entries is less code than a decent
/// polynomial, and it cannot drift between architectures.
const QUARTER: [i16; 91] = [
    0, 17, 35, 52, 70, 87, 105, 122, 139, 156, 174, 191, 208, 225, 242, 259, 276, 292, 309, 326,
    342, 358, 375, 391, 407, 423, 438, 454, 469, 485, 500, 515, 530, 545, 559, 574, 588, 602, 616,
    629, 643, 656, 669, 682, 695, 707, 719, 731, 743, 755, 766, 777, 788, 799, 809, 819, 829, 839,
    848, 857, 866, 875, 883, 891, 899, 906, 914, 921, 927, 934, 940, 946, 951, 956, 961, 966, 970,
    974, 978, 982, 985, 988, 990, 993, 995, 996, 998, 999, 999, 1000, 1000,
];

/// The sine of `degrees`, in thousandths of [`UNIT`].
///
/// The angle wraps, so an ever-growing elapsed time can be handed straight in
/// without overflowing or folding back on itself.
#[must_use]
pub fn sine(degrees: i64) -> i32 {
    let d = degrees.rem_euclid(360);
    let quarter = i32::try_from(d / 90).unwrap_or(0);
    let step = usize::try_from(d % 90).unwrap_or(0);
    let raw = |i: usize| i32::from(QUARTER[i.min(90)]);
    match quarter {
        0 => raw(step),
        1 => raw(90 - step),
        2 => -raw(step),
        _ => -raw(90 - step),
    }
}

/// How one mist band moves: a slow rise and fall, and a swell that thickens and
/// thins it as it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    /// One full cycle, in milliseconds.
    pub period: u32,
    /// How far into its cycle the band starts, in degrees.
    pub phase: i64,
    /// How far it rises and falls, in rows of the reduced picture.
    pub sway: i32,
    /// How much its opacity swells, as a percentage of the resting value.
    pub swell: i32,
}

impl Motion {
    /// Where this band stands at `elapsed` milliseconds: how many rows it has
    /// risen — negative is upwards — and its opacity as a percentage of rest.
    #[must_use]
    pub fn at(&self, elapsed: u64) -> (i32, i32) {
        let period = i64::from(self.period.max(1));
        let elapsed = i64::try_from(elapsed).unwrap_or(i64::MAX);
        // Degrees round the cycle, and the swell runs at twice the rate so the
        // band is not at its thickest exactly when it is at its highest.
        let turn = (elapsed % period) * 360 / period + self.phase;
        let rise = sine(turn) * self.sway / UNIT;
        let swell = sine(turn * 2) * self.swell / UNIT;
        (rise, 100 + swell)
    }
}

/// How the mist bands move, furthest first, to match the order the backdrop
/// composes them in.
///
/// Each band gets a period and a phase of its own. Periods that share no small
/// factor keep the bands from falling into step, which is what would make the
/// whole scene look like it was breathing on a metronome.
#[must_use]
pub fn motions(bands: usize) -> Vec<Motion> {
    (0..bands)
        .map(|i| {
            let i = i32::try_from(i).unwrap_or(0);
            Motion {
                // 23, 31, 43, 59 seconds and so on: slow, and no two in step.
                period: match i % 4 {
                    0 => 23_000,
                    1 => 31_000,
                    2 => 43_000,
                    _ => 59_000,
                },
                phase: i64::from(i) * 97,
                // Nearer mist moves further, as nearer things do.
                sway: 1 + i / 2,
                swell: 25 + i * 10,
            }
        })
        .collect()
}

/// How thick a band of mist is, across its width.
///
/// Mist is patchy. A band of even opacity right across the screen reads as a
/// scan line, which is exactly what the composed backdrop's bands become once
/// they are only a few rows tall — so the screensaver paints its own, with this
/// in place of the flat profile.
///
/// Two waves of different and unrelated wavelengths, which is enough to look
/// unplanned and is two table lookups per column. `drift` slides the whole
/// profile sideways: the mist moving across the valleys is the thing the eye
/// actually follows, and it costs nothing but this addition.
///
/// The result is a percentage of the band's own opacity, never above 100 — mist
/// thins in places, it does not thicken past what the scene asked for.
#[must_use]
pub fn haze(x: u32, drift: i32, seed: i32) -> i32 {
    let at = i64::from(x).wrapping_add(i64::from(drift));
    // Degrees per column: a long roll and a shorter ripple over it.
    let roll = sine(at * 7 / 4 + i64::from(seed));
    let ripple = sine(at * 11 + i64::from(seed) * 3);
    let profile = (roll * 55 + ripple * 20) / UNIT;
    (45 + profile).clamp(0, 100)
}

/// How far a band has drifted sideways after `elapsed` milliseconds.
///
/// Columns per minute, so a band crosses a reduced picture in a few minutes:
/// slow enough that it is weather rather than a conveyor belt, quick enough to
/// be the reason to keep watching.
///
/// Saturating throughout: a screensaver left up for a month is a real thing,
/// and multiplying its milliseconds by a speed is exactly where that overflows.
#[must_use]
pub fn drift(elapsed: u64, per_minute: i32) -> i32 {
    let ms = i64::try_from(elapsed).unwrap_or(i64::MAX);
    let travelled = ms.saturating_mul(i64::from(per_minute)) / 60_000;
    i32::try_from(travelled).unwrap_or(i32::MAX)
}

/// The largest block a picture this size can be drawn in.
///
/// A block so big that the reduced picture is a handful of rows is not a
/// stylised mountain any more, it is a coloured smear, and the ridges stop
/// being ridges. Sixty rows is about where a skyline still reads.
const MIN_ROWS: u32 = 60;

/// The reduced size an output of `output` is drawn at, in blocks of `block`
/// physical pixels, and the block actually used.
///
/// The block is reduced until the picture has enough rows to be a picture, so
/// a 640x480 panel does not end up nine blocks tall.
///
/// Rounded up, not down: a block that does not divide the screen would
/// otherwise leave a strip along the bottom or the right that nothing ever
/// draws, showing whatever the compositor's buffer happened to hold. One block
/// too many, clipped at the edge, covers the screen for certain.
#[must_use]
pub fn reduced(output: Size, block: u32) -> (Size, u32) {
    let w = output.width.max(1);
    let h = output.height.max(1);
    let mut block = block.clamp(1, 64);
    while block > 1 && h / block < MIN_ROWS {
        block -= 1;
    }
    let up = |v: u32| v.div_ceil(block).max(1);
    (Size::new(up(w), up(h)), block)
}

/// Blow `src` up into `dst`, one source pixel to a `block` by `block` square.
///
/// `row` is scratch the size of one destination row, which is filled once per
/// source row and then copied into each of the `block` rows it covers: the
/// expansion is a handful of memory copies per source row rather than a
/// multiply and a bounds check per destination pixel, which is the difference
/// between a screensaver an Atom can draw and one it cannot.
///
/// Any destination not covered — the remainder when the block does not divide
/// the screen — keeps whatever it held, so callers wanting it painted should
/// choose a block that divides, or clear first.
pub fn expand(
    src: &[u32],
    small: Size,
    block: u32,
    row: &mut Vec<u32>,
    mut dst: impl FnMut(u32, &[u32]),
) {
    let block_rows = block.max(1);
    let block = block_rows as usize;
    let width = small.width as usize;
    row.clear();
    row.resize(width * block, 0);
    for y in 0..small.height {
        let Some(line) = src.get(y as usize * width..(y as usize + 1) * width) else {
            return;
        };
        for (x, px) in line.iter().enumerate() {
            let at = x * block;
            if let Some(cell) = row.get_mut(at..at + block) {
                cell.fill(*px);
            }
        }
        // The block is at most 64 and the height at most a screen's, so this
        // row number is nowhere near a u32; a missing row is dropped rather
        // than wrapped round to the top of the screen.
        for step in 0..block {
            let Ok(step) = u32::try_from(step) else {
                return;
            };
            let Some(at) = y.checked_mul(block_rows).and_then(|y| y.checked_add(step)) else {
                return;
            };
            dst(at, row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Motion, UNIT, expand, motions, reduced, sine};
    use denise::geom::Size;

    #[test]
    fn sine_hits_the_quarter_turns_exactly() {
        assert_eq!(sine(0), 0);
        assert_eq!(sine(90), UNIT);
        assert_eq!(sine(180), 0);
        assert_eq!(sine(270), -UNIT);
        assert_eq!(sine(360), 0);
    }

    #[test]
    fn sine_wraps_rather_than_running_off_the_table() {
        assert_eq!(sine(450), sine(90));
        assert_eq!(sine(-90), -UNIT);
        assert_eq!(sine(i64::MAX % 360 + 360 * 1_000_000), sine(i64::MAX % 360));
    }

    #[test]
    fn a_band_returns_to_where_it_started_after_one_period() {
        let m = Motion {
            period: 10_000,
            phase: 0,
            sway: 4,
            swell: 30,
        };
        assert_eq!(m.at(0), m.at(10_000));
        assert_eq!(m.at(1_234), m.at(11_234));
    }

    #[test]
    fn a_band_stays_within_its_sway_and_swell() {
        let m = Motion {
            period: 7_000,
            phase: 40,
            sway: 3,
            swell: 25,
        };
        for ms in (0..14_000).step_by(37) {
            let (rise, alpha) = m.at(ms);
            assert!((-3..=3).contains(&rise), "rise {rise} left its sway");
            assert!((75..=125).contains(&alpha), "alpha {alpha} left its swell");
        }
    }

    #[test]
    fn no_two_bands_share_a_period() {
        let m = motions(4);
        for (i, a) in m.iter().enumerate() {
            for b in &m[i + 1..] {
                assert_ne!(a.period, b.period, "two bands would move in step");
            }
        }
    }

    #[test]
    fn mist_is_patchy_but_never_thicker_than_the_scene_asked_for() {
        let mut seen = std::collections::BTreeSet::new();
        for x in 0..2000 {
            let h = super::haze(x, 0, 3);
            assert!((0..=100).contains(&h), "haze {h} at {x}");
            seen.insert(h);
        }
        assert!(seen.len() > 20, "the mist is the same thickness everywhere");
    }

    #[test]
    fn drifting_mist_is_the_same_mist_further_along() {
        let at = |x: u32, d: i32| super::haze(x, d, 1);
        assert_eq!(
            at(100, 0),
            at(90, 10),
            "the profile slid, it did not change"
        );
        assert_eq!(super::drift(0, 40), 0);
        assert_eq!(super::drift(60_000, 40), 40, "columns in a minute");
        assert_eq!(super::drift(30_000, 40), 20);
    }

    #[test]
    fn drift_does_not_overflow_on_a_screensaver_left_up_for_days() {
        let week = 7 * 24 * 60 * 60 * 1000;
        assert!(super::drift(week, 40) > 0, "it kept going forwards");
    }

    #[test]
    fn a_small_screen_keeps_enough_rows_to_be_a_picture() {
        for (w, h) in [(640, 480), (1024, 768), (1920, 1080), (320, 240)] {
            let (small, block) = reduced(Size::new(w, h), 4);
            assert!(small.height >= 60 || block == 1, "{w}x{h} lost its skyline");
            assert!(small.width >= 1 && small.height >= 1);
        }
    }

    #[test]
    fn a_big_screen_keeps_the_block_it_was_given() {
        let (small, block) = reduced(Size::new(1920, 1080), 4);
        assert_eq!(block, 4);
        assert_eq!(small, Size::new(480, 270));
    }

    #[test]
    fn a_block_that_does_not_divide_the_screen_still_covers_it() {
        let (small, block) = reduced(Size::new(1366, 768), 5);
        assert!(small.width * block >= 1366, "a strip down the right");
        assert!(small.height * block >= 768, "a strip along the bottom");
    }

    #[test]
    fn an_absurd_screen_does_not_panic_or_divide_by_zero() {
        for (w, h) in [(0, 0), (1, 1), (1, 10_000)] {
            let (small, block) = reduced(Size::new(w, h), 4);
            assert!(small.width >= 1 && small.height >= 1 && block >= 1);
        }
    }

    #[test]
    fn expanding_repeats_each_pixel_over_its_whole_block() {
        let src = [1u32, 2, 3, 4];
        let mut row = Vec::new();
        let mut out: Vec<(u32, Vec<u32>)> = Vec::new();
        expand(&src, Size::new(2, 2), 3, &mut row, |y, line| {
            out.push((y, line.to_vec()));
        });
        assert_eq!(out.len(), 6, "two source rows, three destination each");
        assert_eq!(out[0].1, vec![1, 1, 1, 2, 2, 2]);
        assert_eq!(out[2].1, vec![1, 1, 1, 2, 2, 2], "the block repeats down");
        assert_eq!(out[3].1, vec![3, 3, 3, 4, 4, 4]);
        assert_eq!(
            out.iter().map(|(y, _)| *y).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn expanding_a_short_source_stops_rather_than_reading_past_it() {
        let src = [1u32, 2];
        let mut row = Vec::new();
        let mut rows = 0;
        expand(&src, Size::new(2, 8), 2, &mut row, |_, _| rows += 1);
        assert_eq!(rows, 2, "only the row that was there");
    }
}

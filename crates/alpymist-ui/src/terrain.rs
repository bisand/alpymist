//! Procedurally generated mountain ridges.
//!
//! The backdrop is drawn rather than loaded from an image file. That keeps the
//! initramfs small, needs no image decoder in the boot path, and renders at
//! whatever resolution the panel actually has — which on the hardware Alpymist
//! targets is rarely a resolution anyone shipped artwork for.
//!
//! Everything here is pure integer maths with no rendering and no dependencies,
//! so the shapes can be tested without a framebuffer.

use crate::convert::{idx, px};
use crate::logo::{Silhouette, UNIT};

/// A deterministic pseudo-random source.
///
/// Deliberately not a general-purpose RNG: it exists so that a given seed always
/// produces the same mountains. The installer and the login screen draw the
/// same seed, so both show the same range.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Any non-zero state will do; xorshift degenerates from zero.
        Self(seed | 1)
    }

    /// xorshift64*, chosen for being three lines and adequate for scenery.
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `-range..=range`.
    fn jitter(&mut self, range: i32) -> i32 {
        if range <= 0 {
            return 0;
        }
        let span = u64::try_from(range * 2 + 1).unwrap_or(1);
        let v = i32::try_from(self.next() % span).unwrap_or(0);
        v - range
    }
}

/// Sub-pixel precision for ridge heights: 1/256th of a pixel.
///
/// A ridge is a near-horizontal line, and near-horizontal lines are where
/// whole-pixel steps show up as a staircase. Keeping the fraction lets the
/// renderer blend the topmost pixel by how much of it the mountain covers.
const SUBPIXEL: i32 = 256;

/// One mountain ridge: a height for every x across the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ridge {
    /// Height in whole pixels from the top of the screen, one entry per column.
    pub heights: Vec<i32>,
    /// How far below `heights` the surface actually lies, in 1/256ths.
    pub fractions: Vec<u8>,
}

impl Ridge {
    /// Generate a ridge by midpoint displacement.
    ///
    /// `base_y` is the resting height, `amplitude` the initial displacement, and
    /// `roughness` how quickly displacement decays at each subdivision — as a
    /// percentage, so 50 halves the amplitude per level. Lower values give
    /// smoother, more distant-looking ridges.
    ///
    /// Heights are clamped to the last drawable row, so a ridge can never
    /// escape the screen no matter how the parameters are set.
    #[must_use]
    pub fn generate(
        width: u32,
        height: u32,
        base_y: i32,
        amplitude: i32,
        roughness: u32,
        seed: u64,
    ) -> Self {
        let width = usize::try_from(width.max(2)).unwrap_or(2);
        let mut rng = Rng::new(seed);

        // Midpoint displacement needs a power-of-two span plus one point. Work
        // on that grid, then resample down to the screen width.
        let mut span = 1usize;
        while span + 1 < width {
            span *= 2;
        }
        // Everything below is in sub-pixel units, so the resampled heights
        // keep a fraction the renderer can blend with.
        let base = base_y.saturating_mul(SUBPIXEL);
        let mut grid = vec![base; span + 1];
        grid[0] = base + rng.jitter(amplitude * SUBPIXEL / 2);
        grid[span] = base + rng.jitter(amplitude * SUBPIXEL / 2);

        let mut step = span;
        let mut amp = amplitude.saturating_mul(SUBPIXEL);
        while step > 1 {
            let half = step / 2;
            let mut i = half;
            while i < span {
                let mid = i32::midpoint(grid[i - half], grid[i + half]);
                grid[i] = mid + rng.jitter(amp);
                i += step;
            }
            step = half;
            // Saturating and through i64: sub-pixel amplitudes are 256 times
            // larger than they were, and an absurd theme value must clamp
            // rather than wrap the ridge to the other end of the screen.
            amp = i32::try_from(i64::from(amp) * i64::from(roughness) / 100).unwrap_or(i32::MAX);
        }

        // Resample the grid onto actual screen columns, interpolating between
        // grid points so the surface is smooth between them as well as along.
        let fine: Vec<i32> = (0..width)
            .map(|x| {
                let pos = x * span / width.max(1);
                let next = (pos + 1).min(span);
                let within = (x * span) % width.max(1);
                let a = grid[pos.min(span)];
                let b = grid[next];
                // Through i64: these are sub-pixel units, so the products are
                // 256 times what they used to be.
                let step = i32::try_from(
                    i64::from(b - a) * i64::from(u32::try_from(within).unwrap_or(0))
                        / i64::from(u32::try_from(width.max(1)).unwrap_or(1)),
                )
                .unwrap_or(0);
                // height - 1: the last drawable row, not one past it.
                a.saturating_add(step)
                    .clamp(0, px(height.saturating_sub(1)).saturating_mul(SUBPIXEL))
            })
            .collect();

        Self::from_subpixel(&fine)
    }

    /// Split sub-pixel heights into whole pixels and fractions.
    fn from_subpixel(fine: &[i32]) -> Self {
        Self {
            heights: fine.iter().map(|q| q / SUBPIXEL).collect(),
            fractions: fine
                .iter()
                .map(|q| u8::try_from(q.rem_euclid(SUBPIXEL)).unwrap_or(0))
                .collect(),
        }
    }

    /// The partly covered pixel along the ridge's top edge.
    ///
    /// Each entry is `(x, y, coverage)`: how much of the topmost pixel the
    /// mountain fills, 0-255. Painting these blended is what turns a staircase
    /// into a skyline.
    #[must_use]
    pub fn edge(&self) -> Vec<(i32, i32, u8)> {
        self.heights
            .iter()
            .zip(&self.fractions)
            .enumerate()
            .filter_map(|(x, (&top, &frac))| {
                // The surface lies `frac` into this pixel, so the mountain
                // covers the rest of it.
                let coverage = 255u8.saturating_sub(frac);
                // Nearly empty or nearly full pixels are not worth a draw call.
                (20..=235)
                    .contains(&coverage)
                    .then(|| (idx(x), top, coverage))
            })
            .collect()
    }

    /// Generate a ridge that follows a silhouette, roughened by terrain noise.
    ///
    /// The furthest ridge in the backdrop is the Alpymist mark. `relief` is how
    /// tall it stands above `base_y` at its summit, `weather` how much noise is
    /// laid over it, and `spread` and `shift` what part of the horizon it
    /// occupies, as percentages of the width — a range fills part of a view,
    /// not all of it, and the difference between an homage and a stamp is
    /// mostly that it is not centred, not symmetrical, and not smooth.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_silhouette(
        width: u32,
        height: u32,
        base_y: i32,
        relief: i32,
        weather: i32,
        roughness: u32,
        spread: u32,
        shift: u32,
        seed: u64,
        shape: &Silhouette,
    ) -> Self {
        let noise = Self::generate(width, height, 0, weather, roughness, seed);
        let columns = noise.heights.len().max(1);
        let fine: Vec<i32> = noise
            .heights
            .iter()
            .enumerate()
            .map(|(x, &wobble)| {
                let span = columns * usize::try_from(spread.clamp(1, 100)).unwrap_or(100) / 100;
                let start = columns * usize::try_from(shift.min(100)).unwrap_or(0) / 100;
                let across = x.checked_sub(start).filter(|d| *d < span).map_or(0, |d| {
                    u32::try_from(d * UNIT as usize / span.max(1)).unwrap_or(0)
                });
                let up = i32::try_from(shape.elevation(across)).unwrap_or(0) * relief
                    / i32::try_from(UNIT).unwrap_or(1);
                // Noise eases off towards the summits, but never to nothing: a
                // ridge with a perfectly clean peak looks drawn, not weathered.
                let ease = 100 - (up * 45 / relief.max(1)).clamp(0, 45);
                (base_y - up + wobble * ease / 100)
                    .clamp(0, px(height.saturating_sub(1)))
                    .saturating_mul(SUBPIXEL)
            })
            .collect();
        Self::from_subpixel(&fine)
    }

    /// The ridge's filled area as one vertical span per screen column.
    ///
    /// A heightfield is naturally a set of vertical bars, and drawing it that
    /// way keeps the silhouette exact at every column. The obvious alternative
    /// — one polygon — cannot work here: Denise's rasteriser caps a polygon at
    /// 32 vertices and silently ignores anything longer, which is not enough
    /// to describe a ridge across even a 640-pixel screen.
    ///
    /// Each entry is `(x, top, height)`.
    #[must_use]
    pub fn columns(&self, screen_height: u32) -> Vec<(i32, i32, i32)> {
        let bottom = px(screen_height);
        self.heights
            .iter()
            .enumerate()
            .map(|(x, &top)| (idx(x), top, (bottom - top).max(0)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Ridge;
    use crate::convert::px;

    const W: u32 = 640;
    const H: u32 = 480;

    #[test]
    fn the_same_seed_always_draws_the_same_mountains() {
        let a = Ridge::generate(W, H, 300, 60, 55, 0xA1B2);
        let b = Ridge::generate(W, H, 300, 60, 55, 0xA1B2);
        assert_eq!(a, b, "the splash and installer must render identically");
    }

    #[test]
    fn different_seeds_draw_different_mountains() {
        let a = Ridge::generate(W, H, 300, 60, 55, 1);
        let b = Ridge::generate(W, H, 300, 60, 55, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn there_is_one_height_per_screen_column() {
        assert_eq!(
            Ridge::generate(W, H, 300, 60, 55, 7).heights.len(),
            W as usize
        );
        assert_eq!(Ridge::generate(1920, H, 300, 60, 55, 7).heights.len(), 1920);
    }

    #[test]
    fn a_ridge_never_escapes_the_screen() {
        // Absurd amplitude on purpose: clamping is what stops a bad theme value
        // from producing a ridge drawn off-screen.
        let ridge = Ridge::generate(W, H, 300, 100_000, 99, 42);
        assert!(ridge.heights.iter().all(|&y| (0..px(H)).contains(&y)));
    }

    #[test]
    fn low_roughness_gives_a_smoother_ridge_than_high() {
        let smooth = Ridge::generate(W, H, 300, 80, 20, 99);
        let jagged = Ridge::generate(W, H, 300, 80, 90, 99);
        let variation = |r: &Ridge| -> i64 {
            r.heights
                .windows(2)
                .map(|w| i64::from((w[1] - w[0]).abs()))
                .sum()
        };
        assert!(
            variation(&smooth) < variation(&jagged),
            "smooth {} should vary less than jagged {}",
            variation(&smooth),
            variation(&jagged)
        );
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        for (w, h) in [(1, 1), (2, 1), (1, 480), (3, 3)] {
            let r = Ridge::generate(w, h, 0, 10, 50, 1);
            assert!(!r.heights.is_empty());
            assert!(r.heights.iter().all(|&y| (0..px(h)).contains(&y)));
        }
    }

    #[test]
    fn every_column_reaches_the_bottom_of_the_screen() {
        let ridge = Ridge::generate(W, H, 300, 60, 55, 5);
        let cols = ridge.columns(H);
        assert_eq!(cols.len(), W as usize, "one span per screen column");
        for (x, top, h) in cols {
            assert!((0..px(W)).contains(&x));
            assert_eq!(top + h, px(H), "column {x} does not reach the bottom");
            assert!(h > 0, "column {x} is empty");
        }
    }
}

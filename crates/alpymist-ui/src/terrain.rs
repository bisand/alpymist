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

/// A deterministic pseudo-random source.
///
/// Deliberately not a general-purpose RNG: it exists so that a given seed always
/// produces the same mountains. The splash and the installer's first screen draw
/// the same seed, so the handover between them is invisible.
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

/// One mountain ridge: a height for every x across the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ridge {
    /// Height in pixels from the top of the screen, one entry per x column.
    pub heights: Vec<i32>,
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
        let mut grid = vec![base_y; span + 1];
        grid[0] = base_y + rng.jitter(amplitude / 2);
        grid[span] = base_y + rng.jitter(amplitude / 2);

        let mut step = span;
        let mut amp = amplitude;
        while step > 1 {
            let half = step / 2;
            let mut i = half;
            while i < span {
                let mid = i32::midpoint(grid[i - half], grid[i + half]);
                grid[i] = mid + rng.jitter(amp);
                i += step;
            }
            step = half;
            amp = (amp * i32::try_from(roughness).unwrap_or(50)) / 100;
        }

        // Resample the grid onto actual screen columns.
        let heights = (0..width)
            .map(|x| {
                let g = x * span / width.max(1);
                // height - 1: the last drawable row, not one past it.
                grid[g.min(span)].clamp(0, px(height.saturating_sub(1)))
            })
            .collect();

        Self { heights }
    }

    /// The ridge as a closed polygon filling everything below it.
    ///
    /// Returned as screen-space points ready to hand to a polygon fill.
    #[must_use]
    pub fn as_polygon(&self, height: u32) -> Vec<(i32, i32)> {
        let mut points: Vec<(i32, i32)> = self
            .heights
            .iter()
            .enumerate()
            .map(|(x, &y)| (idx(x), y))
            .collect();
        let bottom = px(height);
        let right = idx(self.heights.len()) - 1;
        points.push((right, bottom));
        points.push((0, bottom));
        points
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
    fn the_polygon_closes_along_the_bottom_of_the_screen() {
        let ridge = Ridge::generate(W, H, 300, 60, 55, 5);
        let poly = ridge.as_polygon(H);
        assert_eq!(poly.len(), W as usize + 2);
        assert_eq!(poly[poly.len() - 2], (px(W) - 1, px(H)));
        assert_eq!(poly[poly.len() - 1], (0, px(H)));
    }
}

//! The Alpymist badge: the mark set in a disc.
//!
//! [`crate::logo::MARK`] is the bare ridgeline — the shape the backdrop's
//! furthest range and the site's own header mark share. The badge is the brand's
//! enclosed form: the same two peaks cut out of a disc, with a bank of mist
//! across the lower half whose arc closes the circle. It is what the boot
//! splash and the bar show, where a mark needs an edge of its own rather than
//! a horizon to sit on.
//!
//! The geometry is the vector master in `brand/variants/alpymist-halo-mist.svg`,
//! flattened to polylines and scaled to [`UNIT`]. Curves are already flattened
//! to under a tenth of a pixel at the sizes anything here draws, so nothing in
//! this module has to evaluate one.
//!
//! Shapes only, and no colour: [`crate::render::paint_badge`] decides what to
//! paint them with, and [`mask`] hands a coverage map to whatever wants a
//! bitmap. That is what lets the badge be tested without a graphics stack.

use crate::logo::UNIT;

/// The disc's centre in [`UNIT`] space. The badge is square, so both axes run
/// `0..=UNIT`.
pub const CENTRE: (i32, i32) = (500, 500);

/// The disc's radius in [`UNIT`] space, leaving a little air at the edges.
pub const RADIUS: i32 = 469;

/// The ridgeline, left to right, as an open polyline.
///
/// Both ends sit on the disc, so [`peaks`] can close it along the disc's own
/// lower arc rather than off the edge of the shape. The near-vertical step at
/// the saddle is deliberate: it is where the tall peak's front face cuts in
/// front of its companion.
pub const RIDGE: &[(i32, i32)] = &[
    (59, 660),
    (363, 246),
    (604, 572),
    (605, 512),
    (717, 406),
    (926, 695),
];

/// The sliver of disc that separates the tall peak's tail from its companion.
///
/// Small enough to vanish in a bar icon and to matter at splash size, which is
/// the whole reason the badge is drawn from geometry rather than shipped as a
/// pair of bitmaps.
pub const RIBBON: &[(i32, i32)] = &[
    (605, 512),
    (628, 544),
    (662, 578),
    (705, 611),
    (748, 637),
    (712, 625),
    (667, 607),
    (629, 588),
    (604, 572),
];

/// The bank of mist across the lower half.
///
/// Its lower edge is an arc just inside the disc, and it is the badge's own
/// bottom edge: below it the disc is not drawn at all.
pub const MIST: &[(i32, i32)] = &[
    (88, 693),
    (283, 438),
    (292, 472),
    (311, 504),
    (340, 532),
    (376, 556),
    (429, 577),
    (620, 628),
    (727, 662),
    (807, 697),
    (865, 734),
    (845, 764),
    (796, 819),
    (769, 843),
    (709, 884),
    (677, 901),
    (609, 926),
    (537, 940),
    (464, 942),
    (428, 939),
    (357, 923),
    (323, 911),
    (258, 879),
    (227, 859),
    (172, 812),
    (125, 756),
];

/// The disc as a closed polygon of `segments` edges, in [`UNIT`] space.
///
/// The count is the caller's because the two callers want different things: a
/// painter has a hard ceiling on how many vertices one polygon may have, while
/// [`mask`] has none and would rather have no facets at all.
#[must_use]
pub fn disc(segments: usize) -> Vec<(i32, i32)> {
    let n = segments.clamp(8, 1024);
    (0..n)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "n is at most 1024")]
            let a = (i as f64) * std::f64::consts::TAU / (n as f64);
            point_on_disc(a)
        })
        .collect()
}

/// A point on the disc at `angle` radians, measured the way the screen runs:
/// zero to the right and growing downwards.
fn point_on_disc(angle: f64) -> (i32, i32) {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "bounded by CENTRE + RADIUS"
    )]
    let p = (
        CENTRE.0 + (f64::from(RADIUS) * angle.cos()).round() as i32,
        CENTRE.1 + (f64::from(RADIUS) * angle.sin()).round() as i32,
    );
    p
}

/// The peaks as a closed polygon: the ridgeline, shut along the disc's lower arc.
///
/// Closing on the arc rather than on a box below the badge is what keeps the
/// shape paintable directly — a painter with no clip still cannot spill it
/// outside the disc. `segments` is the count a whole disc would be drawn with;
/// the arc takes its share, so the peaks and the disc stay flush.
#[must_use]
pub fn peaks(segments: usize) -> Vec<(i32, i32)> {
    let mut out = RIDGE.to_vec();
    let (Some(&last), Some(&first)) = (RIDGE.last(), RIDGE.first()) else {
        return out;
    };
    let angle = |p: (i32, i32)| f64::from(p.1 - CENTRE.1).atan2(f64::from(p.0 - CENTRE.0));
    let (mut a0, a1) = (angle(last), angle(first));
    // Sweep the way that passes under the badge rather than over it.
    if a1 < a0 {
        a0 -= std::f64::consts::TAU;
    }
    let n = segments.clamp(8, 1024);
    // The arc's share of a whole disc, rounded up so it is never coarser than
    // the disc a caller drew beside it.
    #[expect(clippy::cast_precision_loss, reason = "n is at most 1024")]
    let wanted = (((a1 - a0) / std::f64::consts::TAU).abs() * (n as f64))
        .ceil()
        .clamp(1.0, 1024.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 1..=1024 on the line above"
    )]
    let steps = wanted as usize;
    for i in 1..steps {
        #[expect(clippy::cast_precision_loss, reason = "steps is at most 1024")]
        let t = (i as f64) / (steps as f64);
        out.push(point_on_disc(a0 + (a1 - a0) * t));
    }
    out
}

/// Coverage for a square icon `size` across, as one byte per pixel.
///
/// The badge is a single region — the disc, less the peaks, less the two
/// shapes the mist lays back over them — so one even-odd fill of all four
/// outlines gives the whole thing, and a caller that wants it in one colour
/// can use this directly as an alpha channel.
///
/// Supersampled: at the size a bar shows an icon, an aliased disc reads as a
/// blob rather than a circle.
#[must_use]
pub fn mask(size: u32) -> Vec<u8> {
    /// Samples per pixel on each axis.
    const SUB: u32 = 4;
    let size = size.max(1);
    let span = size.saturating_mul(SUB);
    let mut counts = vec![0u32; (size as usize).saturating_mul(size as usize)];

    let mut edges: Vec<(i64, i64, i64, i64)> = Vec::new();
    let to_sample = |v: i32| i64::from(v) * i64::from(span) / i64::from(UNIT);
    let mut add = |poly: &[(i32, i32)]| {
        for i in 0..poly.len() {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % poly.len()];
            if y0 != y1 {
                edges.push((to_sample(x0), to_sample(y0), to_sample(x1), to_sample(y1)));
            }
        }
    };
    // No painter here, so nothing caps the detail: draw the curves smooth.
    add(&disc(256));
    add(&peaks(256));
    add(RIBBON);
    add(MIST);

    let mut xs: Vec<i64> = Vec::with_capacity(16);
    for row in 0..span {
        let sy = i64::from(row);
        xs.clear();
        for &(x0, y0, x1, y1) in &edges {
            let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
            if sy < lo || sy >= hi {
                continue;
            }
            xs.push(x0 + (sy - y0) * (x1 - x0) / (y1 - y0));
        }
        xs.sort_unstable();
        // Even-odd: every other span is inside.
        for pair in xs.chunks_exact(2) {
            let clamp = |v: i64| u32::try_from(v.clamp(0, i64::from(span))).unwrap_or(0);
            for sx in clamp(pair[0])..clamp(pair[1]) {
                let idx = (row / SUB) as usize * size as usize + (sx / SUB) as usize;
                if let Some(slot) = counts.get_mut(idx) {
                    *slot += 1;
                }
            }
        }
    }

    counts
        .into_iter()
        .map(|n| u8::try_from((n * 255) / (SUB * SUB)).unwrap_or(u8::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CENTRE, MIST, RADIUS, RIBBON, RIDGE, disc, mask, peaks};

    /// Distance from the disc's centre, for checking a point sits on its edge.
    fn radius_at(p: (i32, i32)) -> f64 {
        f64::from(p.0 - CENTRE.0).hypot(f64::from(p.1 - CENTRE.1))
    }

    #[test]
    fn the_ridgeline_starts_and_ends_on_the_disc() {
        // If it did not, closing it along the arc would leave a wedge of disc
        // showing under the peaks, or cut into the mist.
        for end in [RIDGE[0], RIDGE[RIDGE.len() - 1]] {
            assert!(
                (radius_at(end) - f64::from(RADIUS)).abs() < 2.0,
                "{end:?} is {} from the centre, not {RADIUS}",
                radius_at(end)
            );
        }
    }

    #[test]
    fn closing_the_peaks_keeps_them_inside_the_disc() {
        for p in peaks(256) {
            assert!(
                radius_at(p) <= f64::from(RADIUS) + 2.0,
                "{p:?} lies outside the disc"
            );
        }
    }

    #[test]
    fn the_mist_lies_inside_the_disc() {
        for &p in MIST {
            assert!(
                radius_at(p) <= f64::from(RADIUS) + 2.0,
                "{p:?} lies outside the disc"
            );
        }
    }

    #[test]
    fn the_disc_is_a_closed_ring_of_points_at_the_radius() {
        let ring = disc(64);
        assert_eq!(ring.len(), 64);
        for p in ring {
            assert!((radius_at(p) - f64::from(RADIUS)).abs() < 2.0);
        }
    }

    #[test]
    fn the_outlines_fit_a_painter_that_takes_thirty_two_vertices() {
        // Denise drops a polygon with more than 32 vertices rather than
        // drawing it wrongly, so the constants and the counts
        // `render::paint_badge` asks for have to stay under that.
        assert!(RIBBON.len() <= 32, "ribbon has {}", RIBBON.len());
        assert!(MIST.len() <= 32, "mist has {}", MIST.len());
        assert!(disc(32).len() <= 32);
        assert!(peaks(64).len() <= 32, "peaks has {}", peaks(64).len());
    }

    #[test]
    fn the_mask_is_square() {
        let size = 64;
        assert_eq!(mask(size).len(), (size * size) as usize);
    }

    #[test]
    fn the_corners_are_empty_and_the_crown_is_solid() {
        let size = 128;
        let m = mask(size);
        let at = |x: u32, y: u32| m[(y * size + x) as usize];
        for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
            assert_eq!(at(x, y), 0, "the corner at ({x}, {y}) is outside the disc");
        }
        // Just under the top of the disc, above both peaks.
        assert_eq!(at(size / 2, size / 12), 255);
    }

    #[test]
    fn the_peaks_are_cut_out_and_the_mist_is_not() {
        let size = 128;
        let m = mask(size);
        let at = |x: u32, y: u32| m[(y * size + x) as usize];
        // Mid-badge sits on the tall peak's face, which is a hole.
        assert_eq!(at(size / 2, size / 2), 0);
        // Low and left is the bank of mist, which fills back in.
        assert_eq!(at(size * 3 / 10, size * 4 / 5), 255);
    }

    #[test]
    fn every_size_a_bar_or_a_splash_might_ask_for_is_drawn() {
        for size in [1, 8, 16, 18, 64, 256] {
            let m = mask(size);
            assert_eq!(m.len(), (size * size) as usize);
            if size >= 16 {
                assert!(m.contains(&255), "{size} drew nothing solid");
                assert!(m.contains(&0), "{size} drew no hole");
            }
        }
    }
}

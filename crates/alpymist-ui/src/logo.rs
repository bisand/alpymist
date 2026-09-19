//! The Alpymist mark: two peaks, drawn rather than drawn *by* a font.
//!
//! One silhouette serves twice. It is the brand mark — rendered to an icon for
//! the bar — and it is the shape of the furthest ridge in the backdrop, so the
//! wallpaper quietly repeats the logo without announcing it. The ridge adds
//! terrain noise on top, which is what keeps the homage from looking stamped.
//!
//! Pure integer maths and no rendering, so the shape can be tested on its own.

/// The silhouette's coordinate space: `0..=UNIT` across and up.
///
/// A thousand steps is finer than any screen this is drawn on, so scaling is
/// always down and never shows the grid.
pub const UNIT: u32 = 1000;

/// A mountain outline as a left-to-right polyline over [`UNIT`].
///
/// `x` runs 0 to `UNIT` across; `y` is elevation above the base, 0 at the
/// ground and `UNIT` at the highest possible peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Silhouette {
    /// Points in ascending `x`, at least two.
    pub points: &'static [(u32, u32)],
}

/// The Alpymist mark: a high peak left of centre, a lower companion to its
/// right, and a broad saddle between them. The deliberately spare outline is
/// the small-size form of the logo; larger brand assets add the mist sweep.
pub const MARK: Silhouette = Silhouette {
    points: &[
        (0, 0),
        (323, 779),
        (448, 1000),
        (686, 362),
        (793, 587),
        (1000, 0),
    ],
};

impl Silhouette {
    /// Elevation at `x`, linearly interpolated between the points.
    ///
    /// Outside the outline the elevation is zero: the mark sits on the ground
    /// rather than floating.
    #[must_use]
    pub fn elevation(&self, x: u32) -> u32 {
        let pts = self.points;
        if pts.len() < 2 {
            return 0;
        }
        let x = x.min(UNIT);
        let Some(i) = pts.windows(2).position(|w| x >= w[0].0 && x <= w[1].0) else {
            return 0;
        };
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[i + 1];
        let span = x1.saturating_sub(x0);
        if span == 0 {
            return y0;
        }
        let t = x - x0;
        // Integer interpolation, rounding to nearest.
        let (lo, hi) = (u64::from(y0), u64::from(y1));
        let step = u64::from(t) * (hi.max(lo) - hi.min(lo)) + u64::from(span) / 2;
        let delta = u32::try_from(step / u64::from(span)).unwrap_or(0);
        if y1 >= y0 {
            y0 + delta
        } else {
            y0.saturating_sub(delta)
        }
    }

    /// The highest point, as `(x, elevation)`.
    #[must_use]
    pub fn summit(&self) -> (u32, u32) {
        self.points
            .iter()
            .copied()
            .max_by_key(|(_, y)| *y)
            .unwrap_or((0, 0))
    }

    /// How tall the mark stands relative to its width, as a percentage.
    ///
    /// Mountains seen from a distance are wider than they are tall, and the
    /// mark is meant to read as a range rather than a spike.
    pub const ASPECT: u32 = 62;

    /// Coverage for an icon `size` wide and `size * ASPECT / 100` tall.
    ///
    /// Supersampled, because at the size a bar shows an icon a hard edge is
    /// the difference between a mountain and a staircase. The mark is inset
    /// slightly so it does not touch the edges.
    #[must_use]
    pub fn mask(&self, size: u32) -> Vec<u8> {
        const SUB: u32 = 4;
        /// Percent of the icon left as margin on each side.
        const INSET: u32 = 6;
        let size = size.max(1);
        let height = (size * Self::ASPECT / 100).max(1);
        let mut mask = vec![0u8; (size * height) as usize];
        let samples = size * SUB;
        let rows = height * SUB;
        let inset = samples * INSET / 100;
        let drawn = samples.saturating_sub(inset * 2).max(1);
        let tall = rows.saturating_sub(inset * 2).max(1);

        for sy in 0..rows {
            for sx in 0..samples {
                // Where this sample sits inside the drawn box.
                let Some(dx) = sx.checked_sub(inset).filter(|v| *v < drawn) else {
                    continue;
                };
                let Some(dy) = sy.checked_sub(inset).filter(|v| *v < tall) else {
                    continue;
                };
                let x = dx * UNIT / drawn;
                // Height above the ground, measured from the bottom up.
                let up = (tall - 1 - dy) * UNIT / tall;
                if up <= self.elevation(x) {
                    let i = ((sy / SUB) * size + (sx / SUB)) as usize;
                    // Each pixel holds SUB*SUB samples, so a full pixel is 255
                    // once every one of them has landed.
                    let per_sample = u8::try_from(255 / (SUB * SUB)).unwrap_or(1) + 1;
                    mask[i] = mask[i].saturating_add(per_sample);
                }
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::{MARK, UNIT};

    #[test]
    fn the_mark_stands_on_the_ground_at_both_ends() {
        assert_eq!(MARK.elevation(0), 0);
        assert_eq!(MARK.elevation(UNIT), 0);
    }

    #[test]
    fn it_has_a_high_peak_left_of_centre_and_a_lower_one_to_its_right() {
        let (x, top) = MARK.summit();
        assert!(x < UNIT / 2, "the main peak should sit left of centre");
        assert_eq!(top, UNIT);
        let saddle = (390..700).map(|x| MARK.elevation(x)).min().unwrap();
        let second = (610..900).map(|x| MARK.elevation(x)).max().unwrap();
        assert!(saddle < second, "there is a saddle between the peaks");
        assert!(second < top, "the second peak is lower than the first");
    }

    #[test]
    fn elevation_moves_smoothly_between_points() {
        // No step between neighbouring x is larger than the steepest slope.
        let steps: Vec<i64> = (1..=UNIT)
            .map(|x| i64::from(MARK.elevation(x)) - i64::from(MARK.elevation(x - 1)))
            .collect();
        assert!(
            steps.iter().all(|s| s.abs() <= 4),
            "{:?}",
            steps.iter().max()
        );
    }

    #[test]
    fn the_icon_mask_is_a_mountain_not_a_block_or_a_blank() {
        use super::Silhouette;
        let size = 64;
        let tall = size * Silhouette::ASPECT / 100;
        let mask = MARK.mask(size);
        assert_eq!(mask.len(), (size * tall) as usize, "wider than it is tall");
        let ink = u32::try_from(mask.iter().filter(|a| **a > 127).count()).unwrap();
        // Somewhere between an empty icon and a filled block.
        assert!(ink > size * tall / 12, "too little ink: {ink}");
        assert!(ink < size * tall / 2, "too much ink: {ink}");
        let row = |y: u32| -> usize {
            (0..size)
                .filter(|x| mask[(y * size + x) as usize] > 127)
                .count()
        };
        // Inside the drawn box, well below the summit and well above it.
        assert!(
            row(tall * 4 / 5) > row(tall / 5),
            "it does not widen downwards"
        );
        assert_eq!(row(0), 0, "the mark is inset from the top edge");
        assert_eq!(row(tall - 1), 0, "the mark is inset from the bottom edge");
    }

    /// Soft edges are the point of the mask.
    #[test]
    fn slopes_are_drawn_with_partial_coverage() {
        let soft = MARK
            .mask(64)
            .iter()
            .filter(|a| (20..=235).contains(*a))
            .count();
        assert!(soft > 40, "only {soft} partly covered pixels");
    }

    #[test]
    fn no_size_panics() {
        for size in [0, 1, 2, 3, 16, 17] {
            let _ = MARK.mask(size);
        }
    }
}

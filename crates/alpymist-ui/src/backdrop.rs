//! Composing the mountain scene from sky, ridges and mist.
//!
//! This module decides *what* the backdrop contains and where each piece sits.
//! It does no drawing — a renderer walks the returned layers and paints them.
//! Keeping the composition separate from the painting means the whole scene can
//! be tested without a framebuffer, and reused by any backend Denise offers.

use crate::convert::{px, rows};
use crate::logo::MARK;
use crate::palette::{Palette, Rgb};
use crate::terrain::Ridge;

/// One painted element of the scene, in back-to-front order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    /// A horizontal band of sky, from `y` for `height` rows.
    Sky {
        /// Top of the band.
        y: u32,
        /// Band height in rows.
        height: u32,
        /// Colour of the band.
        colour: Rgb,
    },
    /// A filled mountain ridge, as one vertical span per screen column.
    ///
    /// Spans rather than a polygon because Denise's rasteriser caps polygons at
    /// 32 vertices — far too few to describe a ridge — and silently draws
    /// nothing beyond that.
    Mountain {
        /// `(x, top, height)` per column, left to right.
        columns: Vec<(i32, i32, i32)>,
        /// `(x, y, coverage)` for the partly covered pixels along the skyline,
        /// painted blended so the ridge does not read as a staircase.
        edge: Vec<(i32, i32, u8)>,
        /// Fill colour, already hazed for its distance.
        colour: Rgb,
    },
    /// A translucent mist band lying across the ridges.
    Mist {
        /// Top of the band.
        y: u32,
        /// Band height in rows.
        height: u32,
        /// Mist colour.
        colour: Rgb,
        /// Opacity, 0-255.
        alpha: u8,
    },
}

/// The full backdrop scene for a given screen size.
#[derive(Debug, Clone)]
pub struct Backdrop {
    /// Layers in back-to-front paint order.
    pub layers: Vec<Layer>,
}

/// How many ridges the scene draws. Enough for depth, few enough to stay quick
/// to fill on a CPU that is the reason this machine is on the Potato tier.
const RIDGE_COUNT: u32 = 5;
/// How much of the horizon the mark's range occupies, as a percentage.
const MARK_SPREAD: u32 = 72;
/// Where that range starts, as a percentage of the width. Off-centre, because
/// a range that is centred looks placed.
const MARK_SHIFT: u32 = 15;
/// Sky gradient bands. More is smoother; this is imperceptible from banding at
/// typical panel sizes while staying cheap.
const SKY_BANDS: u32 = 64;

impl Backdrop {
    /// Compose the scene.
    ///
    /// `seed` fixes the mountains: the installer and the login screen pass the
    /// same value, so both show the same range.
    #[must_use]
    pub fn compose(width: u32, height: u32, palette: &Palette, seed: u64) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let mut layers = Vec::new();

        // Sky, as horizontal bands approximating a vertical gradient.
        let band = (height / SKY_BANDS).max(1);
        let mut y = 0;
        while y < height {
            layers.push(Layer::Sky {
                y,
                height: band.min(height - y),
                colour: palette.sky_at(y, height),
            });
            y += band;
        }

        // Ridges, furthest first so nearer ones paint over them.
        for depth in (0..RIDGE_COUNT).rev() {
            let t = depth + 1;
            // Further ridges sit higher up the screen and are gentler.
            let base_y = height - (height * (2 + t) / (RIDGE_COUNT + 4));
            let amplitude = px(height / (6 + depth * 2));
            // Roughness falls with distance. Near ridges are jagged because you
            // can see their detail; far ones smooth into a silhouette. Getting
            // this backwards makes the horizon look like an audio waveform.
            let roughness = 72 - depth * 8;
            let base_y = px(base_y);

            let seed = seed.wrapping_add(u64::from(depth) * 0x9E37_79B9);
            let ridge = if depth == RIDGE_COUNT - 1 {
                // The furthest range is the Alpymist mark, weathered: the logo
                // is in the scenery for anyone who looks twice, and looks like
                // mountains to everyone else.
                Ridge::from_silhouette(
                    width,
                    height,
                    base_y,
                    px(height / 14),
                    amplitude * 2 / 3,
                    roughness,
                    MARK_SPREAD,
                    MARK_SHIFT,
                    seed,
                    &MARK,
                )
            } else {
                Ridge::generate(width, height, base_y, amplitude, roughness, seed)
            };
            layers.push(Layer::Mountain {
                columns: ridge.columns(height),
                edge: ridge.edge(),
                colour: palette.ridge_at(depth, RIDGE_COUNT),
            });

            // Mist pools in the valley in front of each ridge but the nearest,
            // which is what sells the name.
            if depth > 0 {
                layers.push(Layer::Mist {
                    y: rows(base_y).saturating_add(height / 60),
                    height: (height / 22).max(2),
                    colour: palette.mist,
                    // Thinner than it looks: the renderer ramps alpha to zero at
                    // both edges, so this is the value at the band's centre.
                    alpha: u8::try_from((18 + depth * 6).min(54)).unwrap_or(54),
                });
            }
        }

        Self { layers }
    }

    /// The number of mountain ridges in the scene.
    #[must_use]
    pub fn ridge_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|l| matches!(l, Layer::Mountain { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::{Backdrop, Layer, RIDGE_COUNT};
    use crate::convert::px;
    use crate::palette::Palette;

    fn scene(w: u32, h: u32) -> Backdrop {
        Backdrop::compose(w, h, &Palette::alpymist(), 0xA1_B2_C3)
    }

    #[test]
    fn the_scene_has_the_expected_number_of_ridges() {
        assert_eq!(scene(1920, 1080).ridge_count(), RIDGE_COUNT as usize);
    }

    #[test]
    fn sky_is_painted_before_any_mountain() {
        let s = scene(1280, 800);
        let first_mountain = s
            .layers
            .iter()
            .position(|l| matches!(l, Layer::Mountain { .. }))
            .unwrap();
        assert!(
            s.layers[..first_mountain]
                .iter()
                .all(|l| matches!(l, Layer::Sky { .. }))
        );
    }

    #[test]
    fn sky_bands_tile_the_screen_exactly_with_no_gap_or_overhang() {
        let h = 768;
        let s = scene(1024, h);
        let mut covered = 0;
        for layer in &s.layers {
            if let Layer::Sky { y, height, .. } = layer {
                assert_eq!(*y, covered, "sky band starts at a gap");
                covered += height;
            }
        }
        assert_eq!(covered, h, "sky must cover the screen exactly");
    }

    #[test]
    fn every_mountain_stays_on_screen() {
        let (w, h) = (800, 600);
        for layer in &scene(w, h).layers {
            if let Layer::Mountain { columns, .. } = layer {
                assert_eq!(columns.len(), w as usize, "a span per column");
                assert!(
                    columns
                        .iter()
                        .all(|&(x, top, height)| (0..px(w)).contains(&x)
                            && (0..px(h)).contains(&top)
                            && top + height == px(h)),
                    "a mountain column left the screen"
                );
            }
        }
    }

    #[test]
    fn the_same_seed_composes_the_identical_scene() {
        let p = Palette::alpymist();
        let a = Backdrop::compose(1024, 768, &p, 7);
        let b = Backdrop::compose(1024, 768, &p, 7);
        assert_eq!(
            a.layers, b.layers,
            "splash and installer would visibly differ"
        );
    }

    #[test]
    fn mist_is_translucent_and_never_fully_opaque() {
        for layer in &scene(1024, 768).layers {
            if let Layer::Mist { alpha, .. } = layer {
                assert!((1..=90).contains(alpha), "mist alpha {alpha} out of range");
            }
        }
    }

    /// Tiny framebuffers are real: some of this hardware reports 640x480, and a
    /// VM with no display can report smaller still.
    #[test]
    fn absurdly_small_screens_do_not_panic_or_produce_nothing() {
        for (w, h) in [(1, 1), (16, 16), (320, 240), (640, 480)] {
            let s = scene(w, h);
            assert!(!s.layers.is_empty(), "{w}x{h} produced an empty scene");
            assert_eq!(s.ridge_count(), RIDGE_COUNT as usize);
        }
    }
}

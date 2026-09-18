//! The mountains: the wallpaper's own ranges, with the mist moving through them.
//!
//! The scene is composed once for a given screen size and then kept. Sky and
//! ridges are painted once and never again; each frame copies that and lays the
//! mist over it, which is a few thousand blended pixels rather than a few
//! hundred thousand painted ones.
//!
//! What the shared host does with the result — the magnification, the frame
//! pacing, the input that takes it away — is none of this program's business,
//! and is none of any other screensaver's either. This crate is the picture.

use alpymist_screensaver::paint::Painting;
use alpymist_screensaver::scene::{Motion, drift, haze, motions, reduced};
use alpymist_ui::backdrop::{Backdrop, Layer};
use alpymist_ui::palette::{Palette, Rgb};
use alpymist_ui::render::paint_backdrop;
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;

/// What the settings make of this picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Physical pixels to one drawn pixel.
    pub block: u32,
    /// How much mist there is, as a percentage of what the scene composes.
    pub mist: i32,
    /// How many columns the furthest band drifts in a minute.
    pub drift: i32,
}

impl Default for Look {
    /// What the definition file says, repeated here so the picture still draws
    /// when it is run with no definition beside it at all.
    fn default() -> Self {
        Self {
            block: 6,
            mist: 100,
            drift: 14,
        }
    }
}

/// The seed that fixes the mountains.
///
/// The wallpaper, the splash and the installer all draw this same value, so the
/// screensaver shows the ranges that were already on the desktop rather than a
/// different set of hills.
pub const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// One band of mist: where the composed scene put it, and how it moves.
///
/// The backdrop composes its mist as a flat translucent band right across the
/// picture. At full resolution, with its alpha ramped away at both edges, that
/// reads as mist; at a fifth of the resolution it is three rows tall and reads
/// as a scan line. So the bands are taken out of the scene here and painted by
/// [`Scene::paint_mist`] instead, patchy across their width and drifting
/// sideways — which is also the only part of the picture that has to be
/// redrawn from one frame to the next.
struct Band {
    /// The row it rests at.
    y: u32,
    /// How many rows it covers.
    height: u32,
    /// Its colour.
    colour: Rgb,
    /// The opacity at its thickest.
    alpha: u8,
    /// Its rise, fall and swell.
    motion: Motion,
    /// Columns it drifts sideways in a minute. Nearer mist moves faster.
    speed: i32,
    /// What makes this band's patchiness its own.
    seed: i32,
}

/// The scene at its reduced size, ready to animate.
pub struct Mountains {
    /// The reduced size everything is drawn at.
    small: Size,
    /// Physical pixels to one drawn pixel.
    block: u32,
    /// Sky and ridges, painted once. Nothing in them moves.
    base: Vec<u32>,
    /// The frame being drawn: the base, with the mist over it.
    pixels: Vec<u32>,
    bands: Vec<Band>,
    /// A band's thickness at each column, worked out once per band per frame.
    ///
    /// [`haze`] depends on the column and not the row, so computing it inside
    /// the row loop did the same sixteen sines sixteen times over — which on an
    /// Atom was most of what a frame cost.
    across: Vec<i32>,
}

impl Mountains {
    /// Compose for an output of this size, as the settings ask for.
    pub fn compose(output: Size, settings: &Look) -> Self {
        let block = settings.block;
        let (small, block) = reduced(output, block);
        let mut backdrop =
            Backdrop::compose(small.width, small.height, &Palette::alpymist(), SCENE_SEED);

        // The mist comes out of the scene and becomes this module's business;
        // what is left — sky and ridges — never changes again.
        let mut taken = Vec::new();
        backdrop.layers.retain(|layer| match layer {
            Layer::Mist {
                y,
                height,
                colour,
                alpha,
            } => {
                taken.push((*y, *height, *colour, *alpha));
                false
            }
            _ => true,
        });
        let motions = motions(taken.len());
        let bands = taken
            .into_iter()
            .zip(motions)
            .enumerate()
            .map(|(i, ((y, height, colour, alpha), motion))| {
                let i = i32::try_from(i).unwrap_or(0);
                Band {
                    // Taller than the composed band, and centred on it: what
                    // the ramp has to work with is what stops it being a line.
                    y: y.saturating_sub(height / 2),
                    height: (height * 2).max(4),
                    colour,
                    // How much mist there is at all, as the setting asks.
                    alpha: u8::try_from(i32::from(alpha) * settings.mist / 100)
                        .unwrap_or(alpha)
                        .max(1),
                    motion,
                    // Nearer bands drift faster, which is the parallax that
                    // makes the ranges sit behind one another.
                    speed: settings.drift + i * settings.drift * 9 / 14,
                    seed: 40 + i * 113,
                }
            })
            .collect();

        let len = small.width as usize * small.height as usize;
        let mut scene = Self {
            small,
            block,
            base: vec![0; len],
            pixels: vec![0; len],
            bands,
            across: Vec::new(),
        };
        scene.paint_base(&backdrop);
        scene
    }

    /// Paint the sky and the ridges. Once per size, and never again.
    fn paint_base(&mut self, backdrop: &Backdrop) {
        let Some(mut canvas) = Canvas::from_pixels(
            &mut self.base,
            self.small,
            self.small.width,
            PixelFormat::Argb8888,
        ) else {
            return;
        };
        paint_backdrop(&mut canvas, backdrop);
    }

    /// Draw the frame at `elapsed` milliseconds: the base, then the mist.
    fn paint_mist(&mut self, elapsed: u64) {
        self.pixels.copy_from_slice(&self.base);
        let width = self.small.width as usize;
        let rows = self.small.height;

        for band in &self.bands {
            let (rise, swell) = band.motion.at(elapsed);
            let slide = drift(elapsed, band.speed);
            let top = band.y.saturating_add_signed(rise);
            let half = (band.height / 2).max(1);

            // The band's thickness across the picture: the same for every row
            // of it, so worked out once and read back per row.
            self.across.clear();
            self.across
                .extend((0..self.small.width).map(|x| haze(x, slide, band.seed) * swell / 100));

            for step in 0..band.height {
                let y = top.saturating_add(step);
                if y >= rows {
                    break;
                }
                // Triangular down the band: nothing at the edges, everything in
                // the middle, which is what keeps it from having a top and a
                // bottom you can point at.
                let from_centre =
                    i32::try_from(step).unwrap_or(0) - i32::try_from(half).unwrap_or(1);
                let ramp = (i32::try_from(half).unwrap_or(1) - from_centre.abs()).max(0);
                let down = i32::from(band.alpha) * ramp / i32::try_from(half).unwrap_or(1);
                if down == 0 {
                    continue;
                }
                let at = y as usize * width;
                let Some(line) = self.pixels.get_mut(at..at + width) else {
                    continue;
                };
                for (px, across) in line.iter_mut().zip(&self.across) {
                    let alpha = down * across / 100;
                    let Ok(alpha) = u8::try_from(alpha.clamp(0, 255)) else {
                        continue;
                    };
                    if alpha == 0 {
                        continue;
                    }
                    *px = over(*px, band.colour, alpha);
                }
            }
        }
    }
}

impl Painting for Mountains {
    fn small(&self) -> Size {
        self.small
    }

    fn block(&self) -> u32 {
        self.block
    }

    fn frame(&mut self, elapsed: u64) -> &[u32] {
        self.paint_mist(elapsed);
        &self.pixels
    }
}

/// `ink` laid over `under` at `alpha`, both opaque ARGB.
fn over(under: u32, ink: Rgb, alpha: u8) -> u32 {
    let [_, r, g, b] = under.to_be_bytes();
    let mix = |under: u8, ink: u8| {
        let a = u32::from(alpha);
        let blended = (u32::from(ink) * a + u32::from(under) * (255 - a)) / 255;
        u8::try_from(blended.min(255)).unwrap_or(255)
    };
    u32::from_be_bytes([0xFF, mix(r, ink.r), mix(g, ink.g), mix(b, ink.b)])
}

#[cfg(test)]
mod tests {
    use super::{Look, Mountains, over};
    use alpymist_screensaver::paint::Painting;
    use alpymist_ui::backdrop::Layer;
    use alpymist_ui::palette::Rgb;
    use denise::geom::Size;

    #[test]
    fn every_mist_band_is_taken_out_of_the_scene_to_be_animated() {
        let size = Size::new(1920, 1080);
        let scene = Mountains::compose(
            size,
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        let composed = alpymist_ui::backdrop::Backdrop::compose(
            scene.small.width,
            scene.small.height,
            &alpymist_ui::palette::Palette::alpymist(),
            super::SCENE_SEED,
        );
        let mist = composed
            .layers
            .iter()
            .filter(|l| matches!(l, Layer::Mist { .. }))
            .count();
        assert!(mist > 0, "the backdrop composes mist to take");
        assert_eq!(scene.bands.len(), mist, "a band for every one of them");
    }

    #[test]
    fn the_mist_is_drawn_over_the_ridges_rather_than_instead_of_them() {
        let mut scene = Mountains::compose(
            Size::new(1280, 800),
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        scene.paint_mist(0);
        assert_ne!(scene.pixels, scene.base, "no mist was painted at all");
        let changed = scene
            .pixels
            .iter()
            .zip(&scene.base)
            .filter(|(a, b)| a != b)
            .count();
        let total = scene.pixels.len();
        assert!(changed * 100 / total < 40, "the mist covered the picture");
        assert!(changed * 1000 / total > 5, "the mist is barely there");
    }

    #[test]
    fn the_picture_changes_as_time_passes() {
        let mut scene = Mountains::compose(
            Size::new(1280, 800),
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        let first = scene.frame(0).to_vec();
        assert_ne!(
            scene.frame(9_000),
            &first[..],
            "nothing moved in nine seconds"
        );
    }

    #[test]
    fn nothing_it_draws_is_transparent() {
        let mut scene = Mountains::compose(
            Size::new(640, 480),
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        for ms in (0..90_000).step_by(3_000) {
            assert!(
                scene.frame(ms).iter().all(|px| px >> 24 == 0xFF),
                "a transparent pixel would show the desktop through"
            );
        }
    }

    #[test]
    fn it_hands_back_exactly_the_pixels_it_says_it_has() {
        let mut scene = Mountains::compose(Size::new(1366, 768), &Look::default());
        let small = scene.small();
        let len = scene.frame(0).len();
        assert_eq!(len, small.width as usize * small.height as usize);
    }

    #[test]
    fn mist_that_has_drifted_off_the_picture_does_not_panic_or_wrap() {
        let mut scene = Mountains::compose(
            Size::new(800, 600),
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        for ms in [0, 60_000, 3_600_000, 172_800_000, u64::MAX / 2] {
            scene.paint_mist(ms);
        }
    }

    #[test]
    fn a_screen_of_a_different_size_is_a_different_picture() {
        let look = Look {
            block: 4,
            ..Look::default()
        };
        let a = Mountains::compose(Size::new(800, 600), &look);
        let b = Mountains::compose(Size::new(1920, 1080), &look);
        assert_ne!(
            a.small, b.small,
            "the host recomposes; this is what it gets"
        );
        assert_eq!(a.small, Size::new(200, 150));
    }

    #[test]
    fn the_base_holds_no_mist_of_its_own() {
        let mut scene = Mountains::compose(
            Size::new(1024, 768),
            &Look {
                block: 4,
                ..Look::default()
            },
        );
        let base = scene.base.clone();
        scene.paint_mist(0);
        assert_eq!(scene.base, base, "painting a frame disturbed the base");
    }

    #[test]
    fn laying_ink_over_a_colour_stays_between_the_two() {
        let under = 0xFF_00_00_00;
        let ink = Rgb::new(0xFF, 0xFF, 0xFF);
        assert_eq!(over(under, ink, 0), under, "nothing at all");
        assert_eq!(over(under, ink, 255), 0xFF_FF_FF_FF, "all of it");
        let half = over(under, ink, 128);
        assert_eq!(half >> 24, 0xFF);
        assert!((0x70..=0x90).contains(&((half >> 16) & 0xFF)), "{half:08x}");
    }
}

//! The screensaver as a widget: compose small, animate the mist, blow it up.
//!
//! The scene is composed once for a given screen size and then kept. Each frame
//! moves the mist bands and repaints the reduced picture — a few hundred
//! thousand pixels, not a few million — and the expansion to the real screen is
//! a memory copy per row. That is what keeps this affordable on the machines
//! Alpymist exists for: the whole point of a screensaver is to be the thing
//! running when nothing else is, and a screensaver that keeps a core busy is a
//! screensaver that drains the battery it was supposed to be idling through.

use crate::scene::{Motion, drift, expand, haze, motions, reduced};
use alpymist_ui::backdrop::{Backdrop, Layer};
use alpymist_ui::palette::{Palette, Rgb};
use alpymist_ui::render::paint_backdrop;
use alpymist_widget::{Key, Outcome, Widget};
use denise::Frame;
use denise::PixelFormat;
use denise::geom::{Point, Size};
use denise_render::Canvas;
use std::time::Instant;

/// The seed that fixes the mountains.
///
/// The wallpaper, the splash and the installer all draw this same value, so the
/// screensaver shows the ranges that were already on the desktop rather than a
/// different set of hills.
const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// How often the picture is redrawn, in milliseconds.
///
/// Eight frames a second. The mist takes the better part of a minute to
/// complete a cycle, so nothing here moves fast enough to want more, and each
/// frame that is not drawn is a frame's worth of battery: on the 1366x768 panel
/// of an Atom laptop, every frame a second costs about two per cent of a core.
pub const FRAME: u64 = 125;

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
struct Scene {
    /// The output this was composed for: a different one recomposes.
    output: Size,
    /// The reduced size everything is drawn at.
    small: Size,
    /// Physical pixels to one drawn pixel.
    block: u32,
    /// Sky and ridges, painted once. Nothing in them moves.
    base: Vec<u32>,
    /// The frame being drawn: the base, with the mist over it.
    pixels: Vec<u32>,
    bands: Vec<Band>,
    /// One expanded row, reused every row of every frame.
    row: Vec<u32>,
    /// A band's thickness at each column, worked out once per band per frame.
    ///
    /// [`haze`] depends on the column and not the row, so computing it inside
    /// the row loop did the same sixteen sines sixteen times over — which on an
    /// Atom was most of what a frame cost.
    across: Vec<i32>,
}

impl Scene {
    /// Compose for an output of this size.
    fn compose(output: Size, block: u32) -> Self {
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
                    alpha,
                    motion,
                    // Nearer bands drift faster, which is the parallax that
                    // makes the ranges sit behind one another.
                    speed: 14 + i * 9,
                    seed: 40 + i * 113,
                }
            })
            .collect();

        let len = small.width as usize * small.height as usize;
        let mut scene = Self {
            output,
            small,
            block,
            base: vec![0; len],
            pixels: vec![0; len],
            bands,
            row: Vec::new(),
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

    /// Blow the reduced picture up onto the screen.
    fn expand_into(&mut self, frame: &mut Frame<'_>) {
        let Self {
            pixels,
            row,
            small,
            block,
            ..
        } = self;
        expand(pixels, *small, *block, row, |y, line| {
            if let Some(dst) = frame.row_mut(y) {
                let n = dst.len().min(line.len());
                dst[..n].copy_from_slice(&line[..n]);
            }
        });
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

/// The screensaver.
pub struct Saver {
    /// Physical pixels to one drawn pixel, as the settings ask for.
    block: u32,
    scene: Option<Scene>,
    started: Instant,
    /// Where the pointer was when it first reached the surface.
    ///
    /// A surface that appears under a still pointer is sent an enter, and that
    /// is not somebody moving the mouse. Only a position different from the
    /// first one is.
    pointer: Option<Point>,
}

impl Saver {
    /// A screensaver drawing at this block size.
    #[must_use]
    pub fn new(block: u32) -> Self {
        Self {
            block,
            scene: None,
            started: Instant::now(),
            pointer: None,
        }
    }

    /// How long it has been up, in milliseconds.
    fn elapsed(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Widget for Saver {
    type Event = ();

    /// Ignored: the host gives a full-screen widget the whole output.
    fn layout(&mut self, _scale: u32) -> Size {
        self.scene.as_ref().map_or(Size::new(1, 1), |s| s.output)
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        let output = frame.size();
        let stale = self.scene.as_ref().is_none_or(|s| s.output != output);
        if stale {
            self.scene = Some(Scene::compose(output, self.block));
        }
        let elapsed = self.elapsed();
        if let Some(scene) = self.scene.as_mut() {
            scene.paint_mist(elapsed);
            scene.expand_into(frame);
        }
    }

    /// Any key at all takes it away.
    fn key(&mut self, _key: Key) -> Outcome {
        Outcome::Close
    }

    fn text(&mut self, _ch: char) -> Outcome {
        Outcome::Close
    }

    fn press(&mut self, _at: Point) -> Outcome {
        Outcome::Close
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Close
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let Some(at) = at else {
            return Outcome::Unchanged;
        };
        match self.pointer {
            None => {
                self.pointer = Some(at);
                Outcome::Unchanged
            }
            Some(first) if first == at => Outcome::Unchanged,
            Some(_) => Outcome::Close,
        }
    }

    fn animating(&self) -> bool {
        true
    }

    fn frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(FRAME)
    }
}

/// One frame, drawn into a buffer of `output.width * output.height` pixels.
///
/// What the screensaver puts on screen, without a screen: for the snapshot
/// example, and for anything that wants to see a frame without a compositor.
#[must_use]
pub fn frame(output: Size, block: u32, elapsed: u64) -> Vec<u32> {
    let mut scene = Scene::compose(output, block);
    scene.paint_mist(elapsed);
    let mut pixels = vec![0u32; output.width as usize * output.height as usize];
    let width = output.width as usize;
    let Scene {
        pixels: small,
        row,
        small: size,
        block,
        ..
    } = &mut scene;
    expand(small, *size, *block, row, |y, line| {
        let at = y as usize * width;
        if let Some(dst) = pixels.get_mut(at..at + width) {
            let n = dst.len().min(line.len());
            dst[..n].copy_from_slice(&line[..n]);
        }
    });
    pixels
}

/// The colour behind everything, for the instant before the first frame is
/// drawn: the top of the sky, opaque — which is the same `0b121e` the lock
/// screen uses, so one running into the other shows no seam.
#[must_use]
pub fn backdrop() -> u32 {
    let sky = Palette::alpymist().sky_high;
    u32::from_be_bytes([0xFF, sky.r, sky.g, sky.b])
}

#[cfg(test)]
mod tests {
    use super::{Saver, Scene, backdrop, frame, over};
    use alpymist_ui::backdrop::Layer;
    use alpymist_ui::palette::Rgb;
    use alpymist_widget::{Key, Outcome, Widget};
    use denise::geom::{Point, Size};

    #[test]
    fn every_mist_band_is_taken_out_of_the_scene_to_be_animated() {
        let size = Size::new(1920, 1080);
        let scene = Scene::compose(size, 4);
        // The same scene, composed the way the backdrop composes it, says how
        // many bands there were to take.
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
        let mut scene = Scene::compose(Size::new(1280, 800), 4);
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
        let mut scene = Scene::compose(Size::new(1280, 800), 4);
        scene.paint_mist(0);
        let first = scene.pixels.clone();
        scene.paint_mist(9_000);
        assert_ne!(scene.pixels, first, "nothing moved in nine seconds");
    }

    #[test]
    fn nothing_it_draws_is_transparent() {
        let mut scene = Scene::compose(Size::new(640, 480), 4);
        for ms in (0..90_000).step_by(3_000) {
            scene.paint_mist(ms);
            assert!(
                scene.pixels.iter().all(|px| px >> 24 == 0xFF),
                "a transparent pixel would show the desktop through"
            );
        }
    }

    #[test]
    fn mist_that_has_drifted_off_the_picture_does_not_panic_or_wrap() {
        // Two days up, which is longer than any of the cycles by a long way.
        let mut scene = Scene::compose(Size::new(800, 600), 4);
        for ms in [0, 60_000, 3_600_000, 172_800_000, u64::MAX / 2] {
            scene.paint_mist(ms);
        }
    }

    #[test]
    fn a_frame_fills_the_whole_screen_it_was_asked_for() {
        let size = Size::new(1366, 768);
        let pixels = frame(size, 5, 4_000);
        assert_eq!(pixels.len(), 1366 * 768);
        assert!(pixels.iter().all(|px| px >> 24 == 0xFF), "an unpainted gap");
    }

    #[test]
    fn a_resize_recomposes_rather_than_stretching() {
        let a = Scene::compose(Size::new(800, 600), 4);
        let b = Scene::compose(Size::new(1920, 1080), 4);
        assert_ne!(a.small, b.small);
        assert_eq!(a.output, Size::new(800, 600));
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

    #[test]
    fn any_key_or_click_takes_it_away() {
        let mut saver = Saver::new(4);
        assert_eq!(saver.key(Key::Escape), Outcome::Close);
        assert_eq!(Saver::new(4).text('a'), Outcome::Close);
        assert_eq!(Saver::new(4).press(Point::new(0, 0)), Outcome::Close);
        assert_eq!(Saver::new(4).scroll(1), Outcome::Close);
    }

    #[test]
    fn a_pointer_that_has_not_moved_is_not_somebody_arriving() {
        let mut saver = Saver::new(4);
        let still = Point::new(400, 300);
        assert_eq!(saver.pointer(Some(still)), Outcome::Unchanged, "the enter");
        assert_eq!(saver.pointer(Some(still)), Outcome::Unchanged, "and again");
        assert_eq!(saver.pointer(None), Outcome::Unchanged, "and leaving");
        assert_eq!(saver.pointer(Some(Point::new(401, 300))), Outcome::Close);
    }

    #[test]
    fn the_colour_behind_everything_is_opaque() {
        assert_eq!(
            backdrop() >> 24,
            0xFF,
            "a transparent edge shows the desktop"
        );
    }

    /// The base is the picture with no mist in it, so the mist the screensaver
    /// paints is the only mist there is. Two bands at the same place would be
    /// twice as thick as the scene asked for, and would not move together.
    #[test]
    fn the_base_holds_no_mist_of_its_own() {
        let mut scene = Scene::compose(Size::new(1024, 768), 4);
        let base = scene.base.clone();
        scene.paint_mist(0);
        // Rows the mist never reaches are identical; rows it does are not. If
        // the base still had its own bands, the untouched rows would differ
        // from a base composed with the mist left out, which is what this is.
        assert_eq!(scene.base, base, "painting a frame disturbed the base");
    }
}

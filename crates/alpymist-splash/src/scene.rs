//! What the splash draws, independent of which backend draws it.
//!
//! Kept separate from the backend so the layout maths can be tested without a
//! window, and so the DRM and framebuffer builds share exactly this code.

use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::convert::px;
use alpymist_ui::palette::Palette;
use alpymist_ui::picture::Picture;
use alpymist_ui::render::{colour, paint_backdrop, paint_badge};
use alpymist_ui::typeface::Typeface;
use denise::PixelFormat;
use denise::color::Color;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise::pixels::PixelView;
use denise_render::Canvas;

/// The seed that fixes the mountains.
///
/// Only for when there is no [`PICTURE`]. The installer draws the same value,
/// so then the picture does not change when the splash hands over to it.
pub const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// The picture behind the splash: Alpymist's own, which the boot menus are
/// made from and the login screen shows next, so the boot is one picture from
/// the bootloader to the password.
pub const PICTURE: &str = alpymist_ui::picture::SYSTEM;

/// The wordmark, spaced out because the built-in font is tight at large scales.
pub const WORDMARK: &str = "A L P Y M I S T";

/// Shown under the wordmark while the machine is still starting.
pub const TAGLINE: &str = "a cold, quiet Alpine desktop";

/// Where each piece of the splash sits, for a given screen size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Top-left of the badge, which is square.
    pub badge_at: (i32, i32),
    /// The badge's side, in pixels.
    pub badge_size: i32,
    /// The wordmark's text size, in pixels.
    pub wordmark_px: u16,
    /// Where the wordmark is drawn from: its line box's top-left.
    pub wordmark_at: (i32, i32),
    /// How wide its ink is, and how tall its line.
    pub wordmark_size: (i32, i32),
    /// The tagline's text size, in pixels.
    pub tagline_px: u16,
    /// Top-left of the tagline.
    pub tagline_at: (i32, i32),
    /// How wide its ink is, and how tall its line.
    pub tagline_size: (i32, i32),
    /// The progress bar under the tagline, where the screen has room for it.
    pub bar: Option<Rect>,
}

/// The size text is chosen in steps of: the built-in bitmap font's cell
/// height, so the fallback face draws at whole multiples and stays crisp.
const STEP_PX: i32 = 8;

/// `text`'s width and height at `size_px`, in the face that will draw it.
fn measure(face: &mut Typeface, size_px: u16, text: &str) -> (i32, i32) {
    let size = face.measure(size_px, text);
    (
        i32::try_from(size.width).unwrap_or(0),
        i32::try_from(size.height).unwrap_or(0),
    )
}

/// Where `text`'s ink starts and how wide it is, drawn at `size_px` from an
/// x of 0. A face's measured width is its advances, which include the space
/// after the last letter and the side bearings of the first; centring that
/// leaves the letters a few pixels to one side. So the word is drawn once,
/// into a scratch buffer as tall as a line, and the ink is found there.
fn ink(face: &mut Typeface, size_px: u16, text: &str) -> (i32, i32) {
    let (width, height) = measure(face, size_px, text);
    let (Ok(w), Ok(h)) = (u32::try_from(width.max(1)), u32::try_from(height.max(1))) else {
        return (0, width);
    };
    let mut pixels = vec![0u32; (w as usize) * (h as usize)];
    if let Some(mut canvas) =
        Canvas::from_pixels(&mut pixels, Size::new(w, h), w, PixelFormat::Argb8888)
    {
        let mut pen = Pen::new(&mut canvas);
        face.draw(
            &mut pen,
            Point::new(0, 0),
            size_px,
            text,
            Color::rgb(255, 255, 255),
        );
    }
    let row = w as usize;
    let lit = |x: usize| {
        pixels
            .iter()
            .skip(x)
            .step_by(row)
            .any(|p| p & 0x00ff_ffff != 0)
    };
    let Some(left) = (0..row).find(|&x| lit(x)) else {
        return (0, width);
    };
    let right = (0..row).rev().find(|&x| lit(x)).unwrap_or(left);
    let (left, right) = (px_of(left), px_of(right));
    (left, right - left + 1)
}

/// A column index as a coordinate.
fn px_of(x: usize) -> i32 {
    i32::try_from(x).unwrap_or(i32::MAX)
}

impl Layout {
    /// Place everything for a screen of this size, measuring the words with
    /// `face`, which is the face that draws them.
    ///
    /// Measured, not calculated: this was first written against the built-in
    /// bitmap's fixed advance, and the words were then drawn in Fira Mono,
    /// whose advance is not that — so the wordmark sat off the badge's axis.
    ///
    /// The wordmark's size follows the width, so the splash fills a 1920-wide
    /// panel and still fits a 640-wide one — a real resolution on the
    /// hardware this targets.
    #[must_use]
    pub fn for_screen(width: u32, height: u32, face: &mut Typeface) -> Self {
        let w = px(width.max(1));
        let h = px(height.max(1));

        // Aim for the wordmark occupying roughly half the width.
        let unit = measure(face, 8, WORDMARK).0.max(1);
        let scale = ((w / 2) / unit).clamp(1, 12);
        let wordmark_px = u16::try_from(scale * STEP_PX).unwrap_or(96);
        let tagline_px = u16::try_from((scale / 3).max(1) * STEP_PX).unwrap_or(8);
        let (_, wm_h) = measure(face, wordmark_px, WORDMARK);
        let (_, tl_h) = measure(face, tagline_px, TAGLINE);
        let (wm_ink, wm_w) = ink(face, wordmark_px, WORDMARK);
        let (tl_ink, tl_w) = ink(face, tagline_px, TAGLINE);

        // Sit the block above centre: the mountains want the lower half.
        let wm_y = (h / 2) - (h / 8) - (wm_h / 2);
        let gap = wm_h / 2;

        // The badge stands above the wordmark, sized from the screen rather
        // than from the text, so it stays a mark and not an illustration. On
        // a short screen it gives way rather than pushing the block off the
        // top: what must survive is the wordmark.
        let badge_size = (h / 5).clamp(32, 320);
        let badge_size = badge_size.min((wm_y - gap).max(0));
        let badge_at = ((w - badge_size) / 2, wm_y - gap - badge_size);

        let tagline_y = wm_y + wm_h + gap;

        // A thin line as wide as the wordmark, under the tagline. Like the
        // badge it gives way on a short screen, and the words do not.
        let thickness = (i32::from(wordmark_px) / 16).max(3);
        let bar_y = tagline_y + tl_h + gap;
        let bar = (bar_y + thickness * 2 <= h).then(|| {
            let bar_w = wm_w.min(w - 2 * thickness).max(1);
            Rect::new((w - bar_w) / 2, bar_y, bar_w, thickness)
        });

        Self {
            badge_at,
            badge_size,
            wordmark_px,
            // Drawn from where the ink lands centred, not the line box.
            wordmark_at: ((w - wm_w) / 2 - wm_ink, wm_y),
            wordmark_size: (wm_w, wm_h),
            tagline_px,
            tagline_at: ((w - tl_w) / 2 - tl_ink, tagline_y),
            tagline_size: (tl_w, tl_h),
            bar,
        }
    }
}

/// How far the boot has got, as the bar shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    /// This much of it, from 0 to 1.
    Fraction(f32),
    /// Unknown: a short piece slides back and forth, this many steps in.
    Waiting(u32),
}

/// What is behind the badge and the words.
pub enum Background {
    /// The boot picture, scaled to cover the screen, a row at a time.
    Picture(Vec<u32>),
    /// The drawn mountains, when there is no picture.
    Drawn(Backdrop),
}

/// The whole splash: a background plus where the text goes.
pub struct Scene {
    /// The picture, or the mountain scene.
    pub background: Background,
    /// Text placement.
    pub layout: Layout,
    /// Colours.
    pub palette: Palette,
    /// The size this was composed for, so a resize can be detected.
    pub size: (u32, u32),
}

impl Scene {
    /// Compose the splash for a screen of this size, over `picture` if there
    /// is one, with its words measured in `face`.
    #[must_use]
    pub fn new(width: u32, height: u32, picture: Option<&Picture>, face: &mut Typeface) -> Self {
        let palette = Palette::alpymist();
        let background = match picture.and_then(|p| p.cover(width, height)) {
            Some(pixels) => Background::Picture(pixels),
            None => Background::Drawn(Backdrop::compose(width, height, &palette, SCENE_SEED)),
        };
        Self {
            background,
            layout: Layout::for_screen(width, height, face),
            palette,
            size: (width, height),
        }
    }

    /// Recompose if the screen size changed; returns whether it did.
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        picture: Option<&Picture>,
        face: &mut Typeface,
    ) -> bool {
        if self.size == (width, height) {
            return false;
        }
        *self = Self::new(width, height, picture, face);
        true
    }

    /// Paint the background over the whole of `canvas`.
    pub fn paint_background(&self, canvas: &mut Canvas<'_>) {
        let size = Size::new(self.size.0, self.size.1);
        match &self.background {
            Background::Picture(pixels) => {
                if let Some(view) = PixelView::new(pixels, size, size.width) {
                    canvas.copy_from(&view, &[Rect::from_size(size)]);
                }
            }
            Background::Drawn(backdrop) => paint_backdrop(canvas, backdrop),
        }
    }

    /// Paint the badge, the wordmark and the tagline over the background.
    pub fn paint_marks(&self, canvas: &mut Canvas<'_>, face: &mut Typeface) {
        let layout = self.layout;
        let palette = self.palette;
        paint_badge(canvas, layout.badge_at, layout.badge_size, &palette);

        let words = [
            (
                layout.wordmark_at,
                layout.wordmark_px,
                WORDMARK,
                palette.ink,
            ),
            (
                layout.tagline_at,
                layout.tagline_px,
                TAGLINE,
                palette.ink_dim,
            ),
        ];

        let mut pen = Pen::new(canvas);
        // The bar's shadow, once: the bar itself is opaque and redrawn over
        // and over without touching anything around it.
        if let Some(bar) = layout.bar
            && matches!(self.background, Background::Picture(_))
        {
            let d = (bar.height / 2).max(1);
            pen.fill_rect(
                Rect::new(bar.x + d, bar.y + d, bar.width, bar.height),
                self.shadow(),
            );
        }
        for ((x, y), size, text, ink) in words {
            // The drawn sky is dark and even wherever the words go; a
            // photograph is not, and small type over a lit peak disappears
            // into it. A shadow in the night sky's colour keeps its edges.
            if matches!(self.background, Background::Picture(_)) {
                let d = i32::from(size / 16).max(1);
                face.draw(
                    &mut pen,
                    Point::new(x + d, y + d),
                    size,
                    text,
                    self.shadow(),
                );
            }
            face.draw(&mut pen, Point::new(x, y), size, text, colour(ink));
        }
    }
}

impl Scene {
    /// The shadow under the words and the bar over a picture: the night
    /// sky's colour, mostly opaque.
    fn shadow(&self) -> Color {
        let sky = self.palette.sky_high;
        Color::rgba(sky.r, sky.g, sky.b, SHADOW_ALPHA)
    }

    /// Paint the progress bar, and nothing outside it: opaque from edge to
    /// edge, so it can be painted again and again over the last frame.
    pub fn paint_bar(&self, canvas: &mut Canvas<'_>, shown: Shown) {
        let Some(bar) = self.layout.bar else {
            return;
        };
        let p = self.palette;
        let mut pen = Pen::new(canvas);
        pen.fill_rect(bar, colour(p.sky_high));
        let (from, to) = match shown {
            Shown::Fraction(f) => (0, filled(bar.width, f)),
            Shown::Waiting(step) => {
                // A quarter of the bar, there and back again.
                let piece = (bar.width / 4).max(1);
                let travel = (bar.width - piece).max(1);
                let period = u32::try_from(travel).unwrap_or(1) * 2;
                let at =
                    i32::try_from(step.wrapping_mul(WAITING_STEP) % period.max(1)).unwrap_or(0);
                let at = if at > travel { 2 * travel - at } else { at };
                (at, at + piece)
            }
        };
        if to > from {
            pen.fill_rect(
                Rect::new(bar.x + from, bar.y, to - from, bar.height),
                colour(p.ink_dim),
            );
        }
    }
}

/// How many pixels of `width` a fraction fills.
#[must_use]
pub fn filled(width: i32, fraction: f32) -> i32 {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    // The bar is at most a screen wide, well inside f32's exact integers.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let filled = (width as f32 * fraction).round() as i32;
    filled.clamp(0, width.max(0))
}

/// How far the waiting piece moves each step, in pixels.
const WAITING_STEP: u32 = 6;

/// How dark the words' shadow is over a picture.
const SHADOW_ALPHA: u8 = 200;

#[cfg(test)]
mod tests {
    use super::{Background, Layout, Scene, Shown, filled};
    use alpymist_ui::convert::px;
    use alpymist_ui::picture::Picture;
    use alpymist_ui::typeface::{self, Typeface};
    use denise::PixelFormat;
    use denise::geom::{Rect, Size};
    use denise_render::Canvas;

    const SIZES: [(u32, u32); 6] = [
        (320, 240),
        (640, 480),
        (800, 600),
        (1024, 768),
        (1366, 768),
        (1920, 1080),
    ];

    /// The built-in bitmap, which is there on every machine.
    fn bitmap() -> Typeface {
        typeface::load_from("/nonexistent/font.ttf")
    }

    /// Every face this machine can draw with: the bitmap, and the real font
    /// where it is installed or `ALPYMIST_FONT` points at it.
    fn faces() -> Vec<Typeface> {
        let real = typeface::load();
        if real.status.is_loaded() {
            vec![bitmap(), real]
        } else {
            vec![bitmap()]
        }
    }

    #[test]
    fn everything_stays_on_screen_in_order_top_to_bottom() {
        for mut face in faces() {
            for (w, h) in SIZES {
                let l = Layout::for_screen(w, h, &mut face);
                let (sw, sh) = (px(w), px(h));
                assert!(l.badge_at.1 >= 0, "{w}x{h}: badge above the screen");
                assert!(
                    l.badge_at.1 + l.badge_size <= l.wordmark_at.1,
                    "{w}x{h}: badge"
                );
                let wm_bottom = l.wordmark_at.1 + l.wordmark_size.1;
                assert!(l.tagline_at.1 >= wm_bottom, "{w}x{h}: tagline overlaps");
                for (at, size) in [
                    (l.wordmark_at, l.wordmark_size),
                    (l.tagline_at, l.tagline_size),
                ] {
                    assert!(
                        at.0 >= 0 && at.0 + size.0 <= sw,
                        "{w}x{h}: overflows sideways"
                    );
                }
                if let Some(bar) = l.bar {
                    assert!(bar.y >= l.tagline_at.1 + l.tagline_size.1, "{w}x{h}: bar");
                    assert!(bar.x >= 0 && bar.x + bar.width <= sw && bar.y + bar.height <= sh);
                }
            }
        }
        // Room for the bar on every screen the hardware has, if not on a
        // postage stamp.
        let l = Layout::for_screen(1366, 768, &mut bitmap());
        assert!(l.bar.is_some());
    }

    /// The splash as drawn, words only, on black.
    fn drawn(face: &mut Typeface, w: u32, h: u32) -> (Scene, Vec<u32>) {
        let scene = Scene::new(w, h, None, face);
        let mut pixels = vec![0u32; (w * h) as usize];
        let mut canvas =
            Canvas::from_pixels(&mut pixels, Size::new(w, h), w, PixelFormat::Argb8888).unwrap();
        scene.paint_marks(&mut canvas, face);
        (scene, pixels)
    }

    /// The left and right margins of whatever is drawn in rows `rows`.
    fn margins(pixels: &[u32], w: u32, rows: std::ops::Range<i32>) -> (i32, i32) {
        let w = px(w);
        let lit = |x: i32| {
            rows.clone()
                .any(|y| pixels[usize::try_from(y * w + x).unwrap()] & 0x00ff_ffff != 0)
        };
        let left = (0..w).find(|&x| lit(x)).expect("something drawn");
        let right = (0..w).rev().find(|&x| lit(x)).unwrap();
        (left, w - 1 - right)
    }

    #[test]
    fn the_words_are_centred_as_drawn_not_only_as_calculated() {
        for mut face in faces() {
            for (w, h) in [(1024, 768), (1366, 768), (1920, 1080)] {
                let (scene, pixels) = drawn(&mut face, w, h);
                let l = scene.layout;
                for (name, at, size) in [
                    ("wordmark", l.wordmark_at, l.wordmark_size),
                    ("tagline", l.tagline_at, l.tagline_size),
                ] {
                    let (left, right) = margins(&pixels, w, at.1..at.1 + size.1);
                    // A glyph's own side bearings are the only slack: a
                    // pixel or two, against tens before.
                    let tolerance = i32::from(l.wordmark_px / 24).max(2);
                    assert!(
                        (left - right).abs() <= tolerance,
                        "{w}x{h} {name} ({}): {left} left, {right} right",
                        face.status.describe()
                    );
                }
                let (left, right) = margins(&pixels, w, l.badge_at.1..l.badge_at.1 + l.badge_size);
                assert!(
                    (left - right).abs() <= 1,
                    "{w}x{h} badge: {left} vs {right}"
                );
            }
        }
    }

    #[test]
    fn the_bar_fills_with_the_fraction_and_paints_nothing_outside_itself() {
        assert_eq!(filled(200, 0.0), 0);
        assert_eq!(filled(200, 0.5), 100);
        assert_eq!(filled(200, 1.5), 200);
        assert_eq!(filled(200, f32::NAN), 0);

        let (w, h) = (1024, 768);
        let (scene, mut pixels) = drawn(&mut bitmap(), w, h);
        let before = pixels.clone();
        let bar = scene.layout.bar.unwrap();
        for shown in [Shown::Fraction(0.4), Shown::Waiting(7)] {
            let mut canvas =
                Canvas::from_pixels(&mut pixels, Size::new(w, h), w, PixelFormat::Argb8888)
                    .unwrap();
            scene.paint_bar(&mut canvas, shown);
            for (i, (now, was)) in pixels.iter().zip(&before).enumerate() {
                let (x, y) = (
                    px(u32::try_from(i).unwrap() % w),
                    px(u32::try_from(i).unwrap() / w),
                );
                if !Rect::contains(&bar, denise::geom::Point::new(x, y)) {
                    assert_eq!(now, was, "{shown:?} painted outside the bar at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn resizing_recomposes_only_when_the_size_actually_changed() {
        let mut face = bitmap();
        let mut s = Scene::new(1024, 768, None, &mut face);
        assert!(
            !s.resize(1024, 768, None, &mut face),
            "same size must not recompose"
        );
        assert!(s.resize(1280, 800, None, &mut face));
        assert_eq!(s.size, (1280, 800));
    }

    #[test]
    fn a_picture_fills_the_screen_and_its_absence_draws_the_mountains() {
        let mut face = bitmap();
        let picture = Picture::from_rgb(16, 9, vec![0x20; 16 * 9 * 3]).unwrap();
        match Scene::new(1366, 768, Some(&picture), &mut face).background {
            Background::Picture(pixels) => assert_eq!(pixels.len(), 1366 * 768),
            Background::Drawn(_) => panic!("had a picture and drew the mountains"),
        }
        assert!(matches!(
            Scene::new(1366, 768, None, &mut face).background,
            Background::Drawn(_)
        ));
    }

    #[test]
    fn a_bigger_screen_gets_a_bigger_wordmark() {
        let mut face = bitmap();
        let small = Layout::for_screen(640, 480, &mut face).wordmark_px;
        let large = Layout::for_screen(1920, 1080, &mut face).wordmark_px;
        assert!(large > small, "size did not grow: {small} vs {large}");
    }
}

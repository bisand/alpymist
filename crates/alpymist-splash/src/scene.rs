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

/// The picture behind the splash, installed by the `alpymist-splash` package.
///
/// Also what the boot menus are made from, so the splash follows them with the
/// same picture. If it is missing or will not decode, the splash draws the
/// mountains instead: a boot never waits on a photograph.
pub const PICTURE: &str = "/usr/share/alpymist/boot/splash.jpg";

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
    /// Scale factor for the wordmark's bitmap glyphs.
    pub wordmark_scale: i32,
    /// Top-left of the wordmark.
    pub wordmark_at: (i32, i32),
    /// Scale factor for the tagline.
    pub tagline_scale: i32,
    /// Top-left of the tagline.
    pub tagline_at: (i32, i32),
}

/// Advance width of one glyph cell in the built-in font, before scaling.
const ADVANCE: i32 = 6;
/// Height of one glyph cell in the built-in font, before scaling.
const CELL_HEIGHT: i32 = 8;

/// Text width in pixels at a given scale.
fn text_width(text: &str, scale: i32) -> i32 {
    let chars = i32::try_from(text.chars().count()).unwrap_or(0);
    // The last glyph contributes its cell but not a trailing advance gap.
    (chars * ADVANCE * scale) - scale
}

impl Layout {
    /// Place the wordmark and tagline for a screen of this size.
    ///
    /// The wordmark scale is derived from the width rather than fixed, so the
    /// splash fills a 1920-wide panel and still fits a 640-wide one — which is
    /// a real resolution on the hardware this targets.
    #[must_use]
    pub fn for_screen(width: u32, height: u32) -> Self {
        let w = px(width.max(1));
        let h = px(height.max(1));

        // Aim for the wordmark occupying roughly half the width.
        let ideal = (w / 2) / text_width(WORDMARK, 1).max(1);
        let wordmark_scale = ideal.clamp(1, 12);
        let tagline_scale = (wordmark_scale / 3).max(1);

        let wm_w = text_width(WORDMARK, wordmark_scale);
        let tl_w = text_width(TAGLINE, tagline_scale);

        // Sit the block above centre: the mountains want the lower half.
        let wm_y = (h / 2) - (h / 8) - (CELL_HEIGHT * wordmark_scale / 2);
        let gap = CELL_HEIGHT * wordmark_scale / 2;

        // The badge stands above the wordmark, sized from the screen rather
        // than from the text, so it stays a mark and not an illustration. On
        // a short screen it gives way rather than pushing the block off the
        // top: what must survive is the wordmark.
        let badge_size = (h / 5).clamp(32, 320);
        let badge_size = badge_size.min((wm_y - gap).max(0));
        let badge_at = ((w - badge_size) / 2, wm_y - gap - badge_size);

        Self {
            badge_at,
            badge_size,
            wordmark_scale,
            wordmark_at: ((w - wm_w) / 2, wm_y),
            tagline_scale,
            tagline_at: ((w - tl_w) / 2, wm_y + CELL_HEIGHT * wordmark_scale + gap),
        }
    }
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
    /// is one.
    #[must_use]
    pub fn new(width: u32, height: u32, picture: Option<&Picture>) -> Self {
        let palette = Palette::alpymist();
        let background = match picture.and_then(|p| p.cover(width, height)) {
            Some(pixels) => Background::Picture(pixels),
            None => Background::Drawn(Backdrop::compose(width, height, &palette, SCENE_SEED)),
        };
        Self {
            background,
            layout: Layout::for_screen(width, height),
            palette,
            size: (width, height),
        }
    }

    /// Recompose if the screen size changed; returns whether it did.
    pub fn resize(&mut self, width: u32, height: u32, picture: Option<&Picture>) -> bool {
        if self.size == (width, height) {
            return false;
        }
        *self = Self::new(width, height, picture);
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

        // Bitmap scales are glyph-cell multiples; a real font wants pixel
        // heights, and a cell is eight pixels tall.
        let wordmark_px = u16::try_from(layout.wordmark_scale * 8).unwrap_or(96);
        let tagline_px = u16::try_from(layout.tagline_scale * 8).unwrap_or(16);
        let words = [
            (layout.wordmark_at, wordmark_px, WORDMARK, palette.ink),
            (layout.tagline_at, tagline_px, TAGLINE, palette.ink_dim),
        ];

        let mut pen = Pen::new(canvas);
        for ((x, y), size, text, ink) in words {
            // The drawn sky is dark and even wherever the words go; a
            // photograph is not, and small type over a lit peak disappears
            // into it. A shadow in the night sky's colour keeps its edges.
            if matches!(self.background, Background::Picture(_)) {
                let d = i32::from(size / 16).max(1);
                let sky = palette.sky_high;
                let shade = Color::rgba(sky.r, sky.g, sky.b, SHADOW_ALPHA);
                face.draw(&mut pen, Point::new(x + d, y + d), size, text, shade);
            }
            face.draw(&mut pen, Point::new(x, y), size, text, colour(ink));
        }
    }
}

/// How dark the words' shadow is over a picture.
const SHADOW_ALPHA: u8 = 200;

#[cfg(test)]
mod tests {
    use super::{Background, Layout, Scene, TAGLINE, WORDMARK, text_width};
    use alpymist_ui::convert::px;
    use alpymist_ui::picture::Picture;

    #[test]
    fn the_badge_stands_above_the_wordmark_and_stays_on_screen() {
        for (w, h) in [
            (320, 240),
            (640, 480),
            (1024, 768),
            (1366, 768),
            (1920, 1080),
        ] {
            let l = Layout::for_screen(w, h);
            assert!(l.badge_size >= 0, "{w}x{h}: negative badge");
            assert!(l.badge_at.1 >= 0, "{w}x{h}: badge starts above the screen");
            assert!(
                l.badge_at.1 + l.badge_size <= l.wordmark_at.1,
                "{w}x{h}: badge overlaps the wordmark"
            );
            assert!(
                l.badge_at.0 >= 0 && l.badge_at.0 + l.badge_size <= px(w),
                "{w}x{h}: badge overflows sideways"
            );
        }
    }

    #[test]
    fn the_badge_is_centred_on_the_same_axis_as_the_text() {
        let (w, h) = (1366, 768);
        let l = Layout::for_screen(w, h);
        let badge_mid = l.badge_at.0 + l.badge_size / 2;
        let word_mid = l.wordmark_at.0 + text_width(WORDMARK, l.wordmark_scale) / 2;
        assert!(
            (badge_mid - word_mid).abs() <= 1,
            "off by {}",
            badge_mid - word_mid
        );
    }

    #[test]
    fn the_wordmark_fits_within_the_screen_at_every_size_we_care_about() {
        for (w, h) in [
            (640, 480),
            (800, 600),
            (1024, 768),
            (1366, 768),
            (1920, 1080),
        ] {
            let l = Layout::for_screen(w, h);
            let width = text_width(WORDMARK, l.wordmark_scale);
            assert!(l.wordmark_at.0 >= 0, "{w}x{h}: wordmark starts off-screen");
            assert!(
                l.wordmark_at.0 + width <= px(w),
                "{w}x{h}: wordmark {width}px overflows"
            );
        }
    }

    #[test]
    fn the_tagline_fits_too() {
        for (w, h) in [(640, 480), (1024, 768), (1920, 1080)] {
            let l = Layout::for_screen(w, h);
            let width = text_width(TAGLINE, l.tagline_scale);
            assert!(
                l.tagline_at.0 >= 0 && l.tagline_at.0 + width <= px(w),
                "{w}x{h}"
            );
        }
    }

    #[test]
    fn text_is_horizontally_centred() {
        let (w, h) = (1024, 768);
        let l = Layout::for_screen(w, h);
        let left = l.wordmark_at.0;
        let right = px(w) - (left + text_width(WORDMARK, l.wordmark_scale));
        assert!(
            (left - right).abs() <= 1,
            "off-centre by {}",
            (left - right).abs()
        );
    }

    #[test]
    fn the_tagline_sits_below_the_wordmark_without_overlapping() {
        let l = Layout::for_screen(1024, 768);
        let wordmark_bottom = l.wordmark_at.1 + 8 * l.wordmark_scale;
        assert!(
            l.tagline_at.1 > wordmark_bottom,
            "tagline overlaps the wordmark"
        );
    }

    #[test]
    fn a_tiny_screen_still_produces_a_usable_scale() {
        let l = Layout::for_screen(320, 240);
        assert!(l.wordmark_scale >= 1 && l.tagline_scale >= 1);
    }

    #[test]
    fn resizing_recomposes_only_when_the_size_actually_changed() {
        let mut s = Scene::new(1024, 768, None);
        assert!(!s.resize(1024, 768, None), "same size must not recompose");
        assert!(s.resize(1280, 800, None));
        assert_eq!(s.size, (1280, 800));
    }

    #[test]
    fn a_picture_fills_the_screen_and_its_absence_draws_the_mountains() {
        let picture = Picture::from_rgb(16, 9, vec![0x20; 16 * 9 * 3]).unwrap();
        match Scene::new(1366, 768, Some(&picture)).background {
            Background::Picture(pixels) => assert_eq!(pixels.len(), 1366 * 768),
            Background::Drawn(_) => panic!("had a picture and drew the mountains"),
        }
        assert!(matches!(
            Scene::new(1366, 768, None).background,
            Background::Drawn(_)
        ));
    }

    #[test]
    fn a_bigger_screen_gets_a_bigger_wordmark() {
        let small = Layout::for_screen(640, 480).wordmark_scale;
        let large = Layout::for_screen(1920, 1080).wordmark_scale;
        assert!(large > small, "scale did not grow: {small} vs {large}");
    }
}

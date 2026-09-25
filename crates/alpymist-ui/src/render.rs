//! Painting a composed [`Backdrop`] with Denise.
//!
//! Deliberately thin: [`crate::backdrop`] decides what the scene contains, and
//! this walks the result handing primitives to a [`Painter`]. Keeping the two
//! apart is what lets the scene be tested without a graphics stack.

use crate::backdrop::{Backdrop, Layer};
use crate::badge;
use crate::chrome::Chrome;
use crate::convert::px;
use crate::logo::UNIT;
use crate::palette::{Palette, Rgb};
use denise::PixelFormat;
use denise::color::Color;
use denise::geom::Point;
use denise::geom::Rect;
use denise::geom::Size;
use denise::paint::Paint;
use denise::painter::{Painter, Pen};
use denise::pixels::PixelView;
use denise::theme::Theme;
use denise_render::Canvas;
use denise_ui::cursor::{ARROW, Cursor};

/// The rasteriser takes polygon vertices in 8.8 fixed point.
const FX_SHIFT: u32 = 8;

/// The most vertices Denise fills in one polygon.
///
/// Past it the shape is dropped rather than drawn wrongly, which is a silent
/// failure — so every outline [`paint_badge`] hands over has to fit, and
/// `badge`'s own tests hold it to that.
const MAX_VERTICES: usize = 32;

/// Convert a pixel coordinate to the rasteriser's 8.8 fixed point.
///
/// Saturating rather than wrapping: a coordinate large enough to overflow is a
/// bug elsewhere, and clamping it draws a wrong-but-visible shape instead of
/// one that wraps to the opposite edge of the screen.
#[must_use]
pub fn to_fixed(v: i32) -> i32 {
    v.saturating_mul(1 << FX_SHIFT)
}

/// Our palette colour as a Denise colour.
#[must_use]
pub fn colour(c: Rgb) -> Color {
    Color::rgb(c.r, c.g, c.b)
}

/// Paint the [`badge`] with its top-left corner at `at`, `size` pixels across.
///
/// Opaque, in four flat fills rather than the master's gradients: this is
/// drawn over the backdrop's own mountains, and a badge that let them show
/// through its peaks would read as a hole rather than as a mark. The colours
/// are the palette's, so the badge sits in the same light as everything else
/// on the screen.
///
/// Positions are computed in the rasteriser's fixed point rather than rounded
/// to whole pixels first, so the disc stays a circle at splash sizes.
pub fn paint_badge<P: Painter + ?Sized>(
    painter: &mut P,
    at: (i32, i32),
    size: i32,
    palette: &Palette,
) {
    if size <= 0 {
        return;
    }
    let place = |p: &(i32, i32)| -> (i32, i32) {
        let scale = |v: i32| -> i32 {
            let fx = (i64::from(v) * i64::from(size) * i64::from(1 << FX_SHIFT)) / i64::from(UNIT);
            i32::try_from(fx).unwrap_or(i32::MAX)
        };
        (
            to_fixed(at.0).saturating_add(scale(p.0)),
            to_fixed(at.1).saturating_add(scale(p.1)),
        )
    };
    // The disc, and the sliver of it that separates the two peaks.
    let face = palette.sky_low.mix(palette.sky_high, 30);
    let mist = palette.mist.mix(palette.sky_low, 28);

    let ring: Vec<_> = badge::disc(MAX_VERTICES).iter().map(place).collect();
    painter.fill_polygon_fx(&ring, Paint::new(colour(face)));

    // A whole disc's worth of segments, of which the peaks' closing arc takes
    // only its share -- which is what leaves room for the ridgeline itself.
    let peaks: Vec<_> = badge::peaks(64).iter().map(place).collect();
    painter.fill_polygon_fx(&peaks, Paint::new(colour(palette.sky_high)));

    let ribbon: Vec<_> = badge::RIBBON.iter().map(place).collect();
    painter.fill_polygon_fx(&ribbon, Paint::new(colour(face)));

    let bank: Vec<_> = badge::MIST.iter().map(place).collect();
    painter.fill_polygon_fx(&bank, Paint::new(colour(mist)));
}

/// Paint the whole scene, back to front.
pub fn paint_backdrop<P: Painter + ?Sized>(painter: &mut P, backdrop: &Backdrop) {
    let width = px(painter.size().width);

    for layer in &backdrop.layers {
        match layer {
            Layer::Sky {
                y,
                height,
                colour: c,
            } => {
                painter.fill_rect(
                    Rect::new(0, px(*y), width, px(*height)),
                    Paint::new(colour(*c)),
                );
            }
            Layer::Mountain {
                columns,
                edge,
                colour: c,
            } => {
                let rects: Vec<Rect> = columns
                    .iter()
                    .map(|&(x, top, h)| Rect::new(x, top, 1, h))
                    .collect();
                painter.fill_rects(&rects, Paint::new(colour(*c)));
                // The skyline's topmost pixel, blended by how much of it the
                // mountain covers: the difference between a ridge and a
                // staircase, at one thin rectangle per column.
                for &(x, y, coverage) in edge {
                    painter.fill_rect(
                        Rect::new(x, y, 1, 1),
                        Paint::new(Color::rgba(c.r, c.g, c.b, coverage)),
                    );
                }
            }
            Layer::Mist {
                y,
                height,
                colour: c,
                alpha,
            } => paint_mist(painter, width, *y, *height, *c, *alpha),
        }
    }
}

/// A composed scene that rasterises itself once and is copied thereafter.
///
/// [`paint_backdrop`] replays the whole picture every time it is called: one
/// rectangle per screen column per ridge, plus a separate blended pixel along
/// every skyline, which is some thirteen thousand operations. The picture does
/// not change -- [`Backdrop::compose`] already runs only on a resize -- so
/// every frame after the first is paying to draw something it already drew.
///
/// Measured on the Atom this is written for, at 1366x768: painting the scene
/// costs 42.6 ms and copying a painted one costs 2.8 ms. The second number is
/// also why nothing here paints straight to the screen; see
/// [`crate::display::Screen`].
pub struct Scenery {
    /// What the scene contains.
    backdrop: Backdrop,
    /// The scene rasterised, `size.width` words to a row, once it has been.
    painted: Option<Vec<u32>>,
    /// The size both of those are for.
    size: Size,
    /// Whether `painted` is a picture handed in rather than the mountains.
    picture: bool,
}

impl Scenery {
    /// Compose the scene, without yet painting it.
    ///
    /// Deliberately lazy: an [`App`](crate) is built far more often in tests
    /// than it is drawn, and rasterising in the constructor would make every
    /// one of them pay for a picture nobody looks at.
    #[must_use]
    pub fn compose(width: u32, height: u32, palette: &Palette, seed: u64) -> Self {
        Self {
            backdrop: Backdrop::compose(width, height, palette, seed),
            painted: None,
            size: Size::new(width, height),
            picture: false,
        }
    }

    /// The scene with `pixels` shown in place of the mountains: a picture
    /// already scaled to this size, as opaque `0xFFRRGGBB` words a row at a
    /// time. A buffer of the wrong length is ignored and the mountains stay.
    #[must_use]
    pub fn with_picture(mut self, pixels: Vec<u32>) -> Self {
        let len = u64::from(self.size.width) * u64::from(self.size.height);
        if u64::try_from(pixels.len()).is_ok_and(|n| n == len) {
            self.painted = Some(pixels);
            self.picture = true;
        }
        self
    }

    /// Whether this is a picture rather than the drawn mountains: what goes
    /// over it may need more help to stand out from an uneven background.
    #[must_use]
    pub fn is_picture(&self) -> bool {
        self.picture
    }

    /// The composed scene, for anything that wants the layers themselves.
    #[must_use]
    pub fn backdrop(&self) -> &Backdrop {
        &self.backdrop
    }

    /// Paint the scene over `canvas`: rasterised the first time, copied after.
    pub fn paint_onto(&mut self, canvas: &mut Canvas<'_>) {
        if self.painted.is_none() {
            self.painted = Self::rasterise(&self.backdrop, self.size);
        }
        if let Some(pixels) = &self.painted
            && let Some(view) = PixelView::new(pixels, self.size, self.size.width)
        {
            canvas.copy_from(&view, &[Rect::from_size(self.size)]);
            return;
        }
        // A buffer that size could not be made, which needs a display larger
        // than this machine can index. Theoretical, but a slow picture beats a
        // blank screen, so fall back to painting it every frame.
        paint_backdrop(canvas, &self.backdrop);
    }

    /// Rasterise `backdrop` at `size`, or `None` if no buffer that size fits.
    ///
    /// Always `Argb8888`, whatever it will be copied onto. Denise's two formats
    /// are `0xAARRGGBB` and `0xXXRRGGBB` -- the same channels in the same
    /// order, differing only in whether the high byte means anything -- and
    /// every pixel of a backdrop is opaque, so these words are equally right
    /// for an `Xrgb8888` scanout buffer.
    fn rasterise(backdrop: &Backdrop, size: Size) -> Option<Vec<u32>> {
        let len = usize::try_from(u64::from(size.width) * u64::from(size.height)).ok()?;
        let mut pixels = vec![0u32; len];
        {
            let mut canvas =
                Canvas::from_pixels(&mut pixels, size, size.width, PixelFormat::Argb8888)?;
            paint_backdrop(&mut canvas, backdrop);
        }
        Some(pixels)
    }
}

/// Paint a mist band with alpha ramping to zero at both edges.
///
/// A flat translucent rectangle reads as a banding artefact, not as mist — the
/// hard edge is what gives it away. Ramping the alpha across the band costs one
/// `fill_rect` per row and is the whole difference between the two.
fn paint_mist<P: Painter + ?Sized>(
    painter: &mut P,
    width: i32,
    y: u32,
    height: u32,
    c: Rgb,
    peak: u8,
) {
    let rows = px(height.max(1));
    let half = (rows / 2).max(1);
    for row in 0..rows {
        // Triangular profile: zero at the edges, `peak` in the middle.
        let distance_from_centre = (row - half).abs();
        let falloff = (half - distance_from_centre).max(0);
        let alpha = u8::try_from(i32::from(peak) * falloff / half).unwrap_or(peak);
        if alpha == 0 {
            continue;
        }
        painter.fill_rect(
            Rect::new(0, px(y).saturating_add(row), width, 1),
            Paint::new(Color::rgba(c.r, c.g, c.b, alpha)),
        );
    }
}

/// Paint the wizard panel: a translucent slab with a hairline edge.
///
/// Translucent rather than opaque so the mountains still read through it. The
/// edge is what stops it dissolving into the backdrop on a washed-out panel.
pub fn paint_panel<P: Painter + ?Sized>(painter: &mut P, chrome: &Chrome, palette: &Palette) {
    let (x, y, w, h) = chrome.panel;
    let radius = (chrome.padding / 2).max(2);
    let slab = Color::rgba(
        palette.sky_high.r,
        palette.sky_high.g,
        palette.sky_high.b,
        216,
    );
    painter.fill_rounded_rect(Rect::new(x, y, w, h), radius, Paint::new(slab));
    painter.stroke_rounded_rect(
        Rect::new(x, y, w, h),
        radius,
        1,
        Paint::new(Color::rgba(
            palette.mist.r,
            palette.mist.g,
            palette.mist.b,
            64,
        )),
    );

    // A rule under the header, so the title reads as a heading.
    painter.fill_rect(
        Rect::new(chrome.header.0, chrome.rule_y, chrome.header.2, 1),
        Paint::new(Color::rgba(
            palette.ink_dim.r,
            palette.ink_dim.g,
            palette.ink_dim.b,
            72,
        )),
    );
}

/// How prominent a button is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// The action the screen expects next.
    Primary,
    /// Available, but not what most people want here.
    Quiet,
    /// Present so the layout does not jump, but not usable right now.
    Disabled,
}

/// Paint a button: a soft slab with a hairline edge, not a raised 3D control.
///
/// Deliberately understated. These sit on a photograph-like backdrop, and a
/// heavy button would look pasted on; the job is to read as pressable without
/// competing with the mountains.
pub fn paint_button<P: Painter + ?Sized>(
    painter: &mut P,
    rect: (i32, i32, i32, i32),
    style: ButtonStyle,
    palette: &Palette,
) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    let radius = (h / 3).max(2);
    let bounds = Rect::new(x, y, w, h);

    let (fill, edge) = match style {
        ButtonStyle::Primary => (
            Color::rgba(palette.accent.r, palette.accent.g, palette.accent.b, 46),
            Color::rgba(palette.accent.r, palette.accent.g, palette.accent.b, 190),
        ),
        ButtonStyle::Quiet => (
            Color::rgba(palette.mist.r, palette.mist.g, palette.mist.b, 18),
            Color::rgba(palette.mist.r, palette.mist.g, palette.mist.b, 84),
        ),
        ButtonStyle::Disabled => (
            Color::rgba(palette.mist.r, palette.mist.g, palette.mist.b, 8),
            Color::rgba(palette.mist.r, palette.mist.g, palette.mist.b, 28),
        ),
    };

    painter.fill_rounded_rect(bounds, radius, Paint::new(fill));
    painter.stroke_rounded_rect(bounds, radius, 1, Paint::new(edge));
}

/// The colour a button's label should be drawn in.
#[must_use]
pub fn button_ink(style: ButtonStyle, palette: &Palette) -> Rgb {
    match style {
        ButtonStyle::Primary => palette.ink,
        ButtonStyle::Quiet => palette.ink_dim,
        ButtonStyle::Disabled => palette.ink_dim.mix(palette.sky_high, 55),
    }
}

/// Paint the mouse pointer, if it has ever moved.
///
/// Composited into the frame rather than put on the hardware cursor plane.
/// The plane is the better answer for a panel redrawing at speed — it moves the
/// pointer without touching the framebuffer — but this screen only repaints
/// when something changes, and a cursor that leaves a trail when the display
/// is otherwise idle would be worse than one that costs a repaint. The plane is
/// available behind `denise-drm`'s `CursorPlane` if that trade ever changes.
pub fn paint_cursor(pen: &mut Pen<'_>, cursor: &Cursor, theme: &Theme) {
    if cursor.visible {
        cursor.paint(theme, pen);
    }
}

/// The pointer sprite, hidden until the pointer first moves.
///
/// Hidden to begin with on purpose: a machine with no mouse should never show
/// one, and on this hardware that is a real possibility rather than a corner
/// case.
#[must_use]
pub fn new_cursor() -> Cursor {
    Cursor {
        image: &ARROW,
        position: Point::new(0, 0),
        visible: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{FX_SHIFT, Scenery, colour, paint_backdrop, to_fixed};
    use crate::backdrop::Backdrop;
    use crate::palette::{Palette, Rgb};
    use denise::PixelFormat;
    use denise::geom::Size;
    use denise_render::Canvas;

    /// Paint at `size` with `paint` and hand back the words.
    fn pixels(size: Size, paint: impl FnOnce(&mut Canvas<'_>)) -> Vec<u32> {
        let len = (size.width as usize) * (size.height as usize);
        let mut buffer = vec![0u32; len];
        {
            let mut canvas =
                Canvas::from_pixels(&mut buffer, size, size.width, PixelFormat::Argb8888)
                    .expect("a canvas that size");
            paint(&mut canvas);
        }
        buffer
    }

    /// The bug this guards: caching the scene is a speed change and must not be
    /// a visual one. It also covers the second call, which takes the copy path
    /// rather than the painting one.
    #[test]
    fn a_cached_scene_is_the_scene_it_cached() {
        let size = Size::new(320, 200);
        let palette = Palette::alpymist();
        let seed = 0x_A1B2_C3D4_E5F6;

        let backdrop = Backdrop::compose(size.width, size.height, &palette, seed);
        let painted = pixels(size, |canvas| paint_backdrop(canvas, &backdrop));

        let mut scenery = Scenery::compose(size.width, size.height, &palette, seed);
        let first = pixels(size, |canvas| scenery.paint_onto(canvas));
        let second = pixels(size, |canvas| scenery.paint_onto(canvas));

        assert_eq!(first, painted, "the first frame rasterises the scene");
        assert_eq!(second, painted, "every frame after it copies the same one");
    }

    /// A picture handed in is what is painted, every frame; one of the wrong
    /// size is refused and the mountains stay.
    #[test]
    fn a_picture_replaces_the_mountains_only_when_it_fits() {
        let size = Size::new(32, 20);
        let palette = Palette::alpymist();
        let flat = vec![0xFF20_4060_u32; 32 * 20];

        let mut scenery = Scenery::compose(32, 20, &palette, 1).with_picture(flat.clone());
        assert!(scenery.is_picture());
        assert_eq!(pixels(size, |c| scenery.paint_onto(c)), flat);
        assert_eq!(pixels(size, |c| scenery.paint_onto(c)), flat);

        let refused = Scenery::compose(32, 20, &palette, 1).with_picture(vec![0; 7]);
        assert!(!refused.is_picture());
    }

    #[test]
    fn a_scene_still_offers_the_layers_it_composed() {
        let palette = Palette::alpymist();
        let scenery = Scenery::compose(320, 200, &palette, 7);
        assert_eq!(
            scenery.backdrop().ridge_count(),
            Backdrop::compose(320, 200, &palette, 7).ridge_count()
        );
    }

    #[test]
    fn one_pixel_is_one_shifted_unit() {
        assert_eq!(to_fixed(1), 1 << FX_SHIFT);
        assert_eq!(to_fixed(0), 0);
        assert_eq!(to_fixed(1920), 1920 * 256);
    }

    #[test]
    fn negative_coordinates_convert_symmetrically() {
        assert_eq!(to_fixed(-10), -2560);
    }

    /// Wrapping here would put a mountain on the wrong side of the screen,
    /// which is a far more confusing failure than a clamped one.
    #[test]
    fn absurd_coordinates_saturate_rather_than_wrapping() {
        assert_eq!(to_fixed(i32::MAX), i32::MAX);
        assert_eq!(to_fixed(i32::MIN), i32::MIN);
        assert!(to_fixed(100_000_000) > 0, "must not wrap negative");
    }

    #[test]
    fn a_disabled_button_is_dimmer_than_a_primary_one() {
        let p = crate::palette::Palette::alpymist();
        let primary = super::button_ink(super::ButtonStyle::Primary, &p).luminance();
        let disabled = super::button_ink(super::ButtonStyle::Disabled, &p).luminance();
        assert!(
            disabled < primary,
            "disabled {disabled} should be dimmer than {primary}"
        );
    }

    #[test]
    fn palette_colours_survive_the_conversion() {
        let c = colour(Rgb::new(0x12, 0x34, 0x56));
        assert_eq!(c, denise::color::Color::rgb(0x12, 0x34, 0x56));
    }
}

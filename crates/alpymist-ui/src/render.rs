//! Painting a composed [`Backdrop`] with Denise.
//!
//! Deliberately thin: [`crate::backdrop`] decides what the scene contains, and
//! this walks the result handing primitives to a [`Painter`]. Keeping the two
//! apart is what lets the scene be tested without a graphics stack.

use crate::backdrop::{Backdrop, Layer};
use crate::chrome::Chrome;
use crate::convert::px;
use crate::palette::{Palette, Rgb};
use denise::color::Color;
use denise::geom::Rect;
use denise::paint::Paint;
use denise::painter::Painter;

/// The rasteriser takes polygon vertices in 8.8 fixed point.
const FX_SHIFT: u32 = 8;

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
            Layer::Mountain { columns, colour: c } => {
                let rects: Vec<Rect> = columns
                    .iter()
                    .map(|&(x, top, h)| Rect::new(x, top, 1, h))
                    .collect();
                painter.fill_rects(&rects, Paint::new(colour(*c)));
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

#[cfg(test)]
mod tests {
    use super::{FX_SHIFT, colour, to_fixed};
    use crate::palette::Rgb;

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
    fn palette_colours_survive_the_conversion() {
        let c = colour(Rgb::new(0x12, 0x34, 0x56));
        assert_eq!(c, denise::color::Color::rgb(0x12, 0x34, 0x56));
    }
}

//! Painting a composed [`Backdrop`] with Denise.
//!
//! Deliberately thin: [`crate::backdrop`] decides what the scene contains, and
//! this walks the result handing primitives to a [`Painter`]. Keeping the two
//! apart is what lets the scene be tested without a graphics stack.

use crate::backdrop::{Backdrop, Layer};
use crate::convert::px;
use crate::palette::Rgb;
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
            Layer::Mountain { polygon, colour: c } => {
                let points: Vec<(i32, i32)> = polygon
                    .iter()
                    .map(|&(x, y)| (to_fixed(x), to_fixed(y)))
                    .collect();
                painter.fill_polygon_fx(&points, Paint::new(colour(*c)));
            }
            Layer::Mist {
                y,
                height,
                colour: c,
                alpha,
            } => {
                painter.fill_rect(
                    Rect::new(0, px(*y), width, px(*height)),
                    Paint::new(Color::rgba(c.r, c.g, c.b, *alpha)),
                );
            }
        }
    }
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

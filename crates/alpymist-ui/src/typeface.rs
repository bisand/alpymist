//! Loading the interface typeface, and getting by without it.
//!
//! Alpymist draws its text with Fira Mono where it can and Denise's built-in
//! 5×8 bitmap where it cannot. The bitmap is not a placeholder: it already
//! covers `ÆØÅ æøå ÄÖÜ äöü Éé ß °`, so a system with no font file still reads
//! correctly in Norwegian — it just looks blockier.
//!
//! Why Fira Mono and not Fira Code: at this tier Denise does not shape text, so
//! Fira Code's programming ligatures never apply, and Fira Code is Fira Mono
//! plus exactly those ligatures. The two render identically here, and Fira Mono
//! is a tenth the size and already in Alpine's `font-fira-ttf`. Ligatures would
//! also be actively unwanted — a hostname field turning `->` into an arrow is a
//! bug, not a feature.

use denise::color::Color;
use denise::geom::{Point, Size};
use denise::painter::Pen;
use denise_text::{FontId, TextEngine, TextStyle, TrueTypeSource};

/// Where the interface font lives on an Alpymist system.
///
/// This is Alpine's own path from `font-fira-ttf`, not a copy of our own: the
/// image already installs that package, and copying the file somewhere else
/// would mean shipping the same 171 KB twice and keeping them in step.
pub const FONT_PATH: &str = "/usr/share/fonts/TTF/FiraMono-Regular.ttf";

/// Environment variable that overrides [`FONT_PATH`], for development.
pub const FONT_PATH_ENV: &str = "ALPYMIST_FONT";

/// What happened when we tried to load the typeface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontStatus {
    /// Fira Mono loaded, and text will be drawn with it.
    Loaded {
        /// Where it was read from.
        path: String,
    },
    /// The file was missing; the built-in bitmap is in use.
    Missing {
        /// Where we looked.
        path: String,
    },
    /// The file was there but unusable.
    Invalid {
        /// Where we looked.
        path: String,
        /// What the font parser said.
        reason: String,
    },
}

impl FontStatus {
    /// Whether a real typeface is in use.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded { .. })
    }

    /// A line worth putting in the install log.
    ///
    /// Falling back is not an error — the system is still perfectly usable —
    /// but it should never happen silently, or nobody will notice the image
    /// stopped shipping the font.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Loaded { path } => format!("typeface: Fira Mono from {path}"),
            Self::Missing { path } => {
                format!("typeface: {path} not found; using the built-in bitmap font")
            }
            Self::Invalid { path, reason } => {
                format!("typeface: {path} could not be read ({reason}); using the built-in font")
            }
        }
    }
}

/// Where to look for the typeface, honouring the development override.
#[must_use]
pub fn font_path() -> String {
    std::env::var(FONT_PATH_ENV).unwrap_or_else(|_| FONT_PATH.to_string())
}

/// The interface typeface, whichever one we ended up with.
///
/// Call sites do not branch on which: [`Typeface::draw`] behaves the same
/// either way, so a system without the font file renders blockier text and
/// nothing else changes.
pub struct Typeface {
    engine: TextEngine,
    font: FontId,
    /// What happened when the font was loaded.
    pub status: FontStatus,
}

impl Typeface {
    /// The style for text of this pixel height.
    #[must_use]
    pub fn style(&self, size_px: u16) -> TextStyle {
        TextStyle {
            font: self.font,
            size_px,
        }
    }

    /// Draw `text` with its top-left corner at `at`.
    pub fn draw(
        &mut self,
        pen: &mut Pen<'_>,
        at: Point,
        size_px: u16,
        text: &str,
        colour: Color,
    ) -> Size {
        let style = self.style(size_px);
        self.engine.draw(pen, style, at, text, colour)
    }

    /// Where to draw `label` so it sits centred in `rect`.
    ///
    /// Measured rather than calculated from a glyph cell. The layout was first
    /// written against Denise's 5x8 bitmap, whose advance and line height are
    /// fixed multiples; a real font's are neither, so assuming them put every
    /// button label a few pixels low and slightly off-centre.
    ///
    /// `TextEngine::draw` takes the top-left of the line box, not the baseline,
    /// so centring the measured extent is all that is needed — no ascent or
    /// descent arithmetic, and nothing that changes when the font does.
    pub fn centre_in(&mut self, rect: (i32, i32, i32, i32), size_px: u16, label: &str) -> Point {
        let extent = self.measure(size_px, label);
        let (x, y, w, h) = rect;
        let text_w = i32::try_from(extent.width).unwrap_or(0);
        let text_h = i32::try_from(extent.height).unwrap_or(0);
        Point::new(x + (w - text_w) / 2, y + (h - text_h) / 2)
    }

    /// How much room `text` would take.
    pub fn measure(&mut self, size_px: u16, text: &str) -> Size {
        let style = self.style(size_px);
        self.engine.measure(style, text)
    }
}

/// Build a text engine, loading Fira Mono from wherever [`font_path`] points.
///
/// Never fails: see [`load_from`].
#[must_use]
pub fn load() -> Typeface {
    load_from(&font_path())
}

/// Build a text engine from a specific font file.
///
/// Never fails: a missing or broken font falls back to the built-in bitmap,
/// because an installer that refuses to draw because it cannot find a font is
/// worse in every way than one that draws a little more crudely.
///
/// Takes the path rather than reading the environment so that a test can cover
/// every outcome without mutating process-wide state — which in edition 2024 is
/// `unsafe`, and this crate forbids that.
#[must_use]
pub fn load_from(path: &str) -> Typeface {
    let path = path.to_string();
    let mut engine = TextEngine::new();
    // FontId(0) is Denise's built-in bitmap, which is what every failure below
    // falls back to.
    let built_in = TextStyle::built_in(0).font;

    let Ok(bytes) = std::fs::read(&path) else {
        return Typeface {
            engine,
            font: built_in,
            status: FontStatus::Missing { path },
        };
    };
    match TrueTypeSource::from_bytes("Fira Mono", &bytes) {
        Ok(source) => {
            let font = engine.add_font(Box::new(source));
            engine.set_default_font(font);
            Typeface {
                engine,
                font,
                status: FontStatus::Loaded { path },
            }
        }
        Err(reason) => Typeface {
            engine,
            font: built_in,
            status: FontStatus::Invalid { path, reason },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{FONT_PATH, FONT_PATH_ENV, FontStatus, font_path, load_from};

    #[test]
    fn a_missing_font_falls_back_rather_than_failing() {
        let face = load_from("/nonexistent/NoSuch.ttf");
        assert!(matches!(face.status, FontStatus::Missing { .. }));
        assert!(!face.status.is_loaded());
    }

    #[test]
    fn a_file_that_is_not_a_font_falls_back_too() {
        // Cargo.toml is definitely present and definitely not a TrueType file.
        let face = load_from("Cargo.toml");
        assert!(
            matches!(face.status, FontStatus::Invalid { .. }),
            "expected Invalid, got {:?}",
            face.status
        );
        assert!(!face.status.is_loaded());
    }

    /// The fallback must still be able to draw; a Typeface that cannot is
    /// worse than useless because the failure only shows up on screen.
    #[test]
    fn a_fallen_back_typeface_still_measures_text() {
        let mut face = load_from("/nonexistent/NoSuch.ttf");
        let size = face.measure(16, "Kjærlighet på Øy");
        assert!(
            size.width > 0 && size.height > 0,
            "fallback measured nothing"
        );
    }

    #[test]
    fn the_path_falls_back_to_the_installed_location() {
        // Only meaningful when nothing has overridden it, which is the norm.
        if std::env::var(FONT_PATH_ENV).is_err() {
            assert_eq!(font_path(), FONT_PATH);
        }
    }

    /// The bug this replaces: labels were centred using bitmap glyph metrics,
    /// which are wrong for any real font.
    #[test]
    fn a_label_is_centred_in_its_button_by_measurement() {
        let mut face = load_from("/nonexistent/NoSuch.ttf");
        let rect = (100, 200, 220, 48);
        let label = "Enter  Continue";
        let at = face.centre_in(rect, 16, label);
        let extent = face.measure(16, label);

        let left = at.x - rect.0;
        let right = (rect.0 + rect.2) - (at.x + i32::try_from(extent.width).unwrap());
        assert!(
            (left - right).abs() <= 1,
            "horizontally off by {}",
            (left - right).abs()
        );

        let top = at.y - rect.1;
        let bottom = (rect.1 + rect.3) - (at.y + i32::try_from(extent.height).unwrap());
        assert!(
            (top - bottom).abs() <= 1,
            "vertically off by {}",
            (top - bottom).abs()
        );
    }

    #[test]
    fn a_longer_label_starts_further_left_but_stays_centred() {
        let mut face = load_from("/nonexistent/NoSuch.ttf");
        let rect = (0, 0, 400, 40);
        let short = face.centre_in(rect, 16, "Back").x;
        let long = face.centre_in(rect, 16, "Enter  Continue").x;
        assert!(long < short, "longer label should start further left");
    }

    #[test]
    fn a_label_wider_than_its_button_is_not_pushed_off_to_the_right() {
        let mut face = load_from("/nonexistent/NoSuch.ttf");
        let at = face.centre_in((50, 50, 10, 10), 16, "far too long for this");
        assert!(
            at.x <= 50,
            "an overflowing label should overhang evenly, not shift right"
        );
    }

    #[test]
    fn every_outcome_says_something_a_log_reader_can_act_on() {
        for status in [
            FontStatus::Loaded { path: "/a".into() },
            FontStatus::Missing { path: "/b".into() },
            FontStatus::Invalid {
                path: "/c".into(),
                reason: "bad table".into(),
            },
        ] {
            let line = status.describe();
            assert!(!line.is_empty());
            assert!(line.contains("typeface:"), "{line}");
        }
    }

    #[test]
    fn a_fallback_is_reported_as_a_fallback_not_as_success() {
        assert!(!FontStatus::Missing { path: "/x".into() }.is_loaded());
        assert!(
            !FontStatus::Invalid {
                path: "/x".into(),
                reason: "n".into()
            }
            .is_loaded()
        );
        assert!(FontStatus::Loaded { path: "/x".into() }.is_loaded());
    }
}

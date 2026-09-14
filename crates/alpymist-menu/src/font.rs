//! TrueType faces that cost nothing to open.
//!
//! Denise's own TrueType source is built on fontdue, which prepares the
//! outline of every glyph in the file before it hands the face back. For Fira
//! Sans that is 30 ms; for Symbols Nerd Font, ten thousand icons, it is well
//! over 100 — more than everything else the menu does before its first frame,
//! several times over, to draw perhaps forty glyphs.
//!
//! This source parses the table directory and nothing else, and outlines a
//! glyph the first time it is measured. Denise's glyph cache sits in front of
//! it, so each glyph is outlined and rasterised once per size however many
//! times it is drawn.
//!
//! Sizes mean what they mean to fontdue — pixels per em — so a layout does
//! not change with the source behind it.

use ab_glyph::{Font, FontVec, GlyphId as AbGlyphId, PxScale, ScaleFont, point};
use denise::Size;
use denise_text::{FontMetrics, GlyphId, GlyphMetrics, GlyphSource, Rasterised};

/// A face read from a TrueType or OpenType file, outlined on demand.
pub struct LazyFont {
    name: String,
    font: FontVec,
    /// Font units per em over the units `ab_glyph` scales by.
    em_to_height: f32,
    scratch: Vec<u8>,
}

impl LazyFont {
    /// Parse a face from its bytes.
    ///
    /// # Errors
    /// When the bytes are not a font.
    pub fn from_vec(name: &str, data: Vec<u8>) -> Result<Self, String> {
        let font = FontVec::try_from_vec(data).map_err(|e| e.to_string())?;
        // ab_glyph's PxScale is the height from descender to ascender; fontdue's
        // size, and everyone's idea of a 16 px font, is the em.
        let em = font.units_per_em().unwrap_or(1000.0);
        let em_to_height = font.height_unscaled() / em;
        Ok(Self {
            name: name.to_owned(),
            font,
            em_to_height,
            scratch: Vec::new(),
        })
    }

    fn scale(&self, size_px: u16) -> PxScale {
        PxScale::from(f32::from(size_px) * self.em_to_height)
    }

    fn id(&self, glyph: GlyphId) -> AbGlyphId {
        glyph
            .as_char()
            .map_or(AbGlyphId(0), |ch| self.font.glyph_id(ch))
    }

    /// Metrics and, when `raster` is set, coverage into the scratch buffer.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn glyph(&mut self, glyph: GlyphId, size_px: u16, raster: bool) -> GlyphMetrics {
        let scale = self.scale(size_px);
        let id = self.id(glyph);
        let advance = self.font.as_scaled(scale).h_advance(id).round() as i32;
        let Some(outline) = self
            .font
            .outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0)))
        else {
            return GlyphMetrics {
                advance,
                ..GlyphMetrics::default()
            };
        };
        let bounds = outline.px_bounds();
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        if raster {
            self.scratch.clear();
            self.scratch.resize(width as usize * height as usize, 0);
            let scratch = &mut self.scratch;
            outline.draw(|x, y, coverage| {
                if let Some(px) = scratch.get_mut(y as usize * width as usize + x as usize) {
                    *px = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
        }
        GlyphMetrics {
            advance,
            bearing_x: bounds.min.x as i32,
            // Bounds are y-down from the baseline; the trait wants the top
            // edge's height above it.
            bearing_y: -(bounds.min.y as i32),
            size: Size::new(width, height),
        }
    }
}

impl GlyphSource for LazyFont {
    fn name(&self) -> &str {
        &self.name
    }

    #[allow(clippy::cast_possible_truncation)]
    fn metrics(&self, size_px: u16) -> FontMetrics {
        let scaled = self.font.as_scaled(self.scale(size_px));
        FontMetrics {
            ascent: scaled.ascent().round() as i32,
            descent: (-scaled.descent()).round() as i32,
            line_gap: scaled.line_gap().round() as i32,
        }
    }

    fn glyph_metrics(&mut self, glyph: GlyphId, size_px: u16) -> Option<GlyphMetrics> {
        Some(self.glyph(glyph, size_px, false))
    }

    fn rasterise(&mut self, glyph: GlyphId, size_px: u16) -> Option<Rasterised<'_>> {
        let metrics = self.glyph(glyph, size_px, true);
        Some(Rasterised {
            metrics,
            coverage: &self.scratch,
            stride: metrics.size.width as usize,
        })
    }

    fn fallback_id(&self, _: char) -> Option<GlyphId> {
        // Code point zero maps to glyph zero, `.notdef`: the box.
        Some(GlyphId(0))
    }

    fn contains(&self, ch: char) -> bool {
        self.font.glyph_id(ch).0 != 0
    }
}

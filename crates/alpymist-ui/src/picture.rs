//! A photograph behind the boot and the login, instead of the drawn mountains.
//!
//! The boot menus, the splash, the login screen and the lock show one of the
//! wallpapers in `brand/wallpapers/`, which is a JPEG at 1920x1080. Screens are not, so
//! [`Picture::cover`] scales it the way `swaybg -m fill` does: large enough to
//! cover the screen, centred, with whatever overhangs cut off rather than bars
//! left at the edges.
//!
//! Decoding and scaling only, with no graphics stack: what comes out is the
//! same `0xAARRGGBB` words [`crate::render::Scenery`] keeps, and is put on the
//! screen the same way.

use std::path::Path;

/// Alpymist's own picture: behind the boot menus, the splash, the login
/// screen and the lock. Each account's wallpaper is its own business; this is
/// the machine's, and the `alpymist-splash-picture` package installs it.
///
/// Whatever shows it draws the mountains instead when it is missing or will
/// not decode: nothing that has to appear waits on a photograph.
pub const SYSTEM: &str = "/usr/share/alpymist/picture.jpg";

/// A decoded picture, three bytes to a pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Picture {
    /// A picture from packed RGB bytes, or `None` if there are not
    /// `width * height * 3` of them or either side is zero.
    #[must_use]
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Option<Self> {
        let len = usize::try_from(u64::from(width) * u64::from(height) * 3).ok()?;
        (width > 0 && height > 0 && rgb.len() == len).then_some(Self { width, height, rgb })
    }

    /// Decode a JPEG.
    ///
    /// # Errors
    ///
    /// If the bytes are not a JPEG the decoder can read.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use zune_core::bytestream::ZCursor;
        use zune_core::colorspace::ColorSpace;
        use zune_core::options::DecoderOptions;
        use zune_jpeg::JpegDecoder;

        // RGB whatever the file holds, so a greyscale or CMYK picture still
        // comes out three bytes to a pixel.
        let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
        let rgb = decoder.decode().map_err(|e| format!("{e:?}"))?;
        let (w, h) = decoder
            .dimensions()
            .ok_or("the decoder gave no dimensions")?;
        let (w, h) = (
            u32::try_from(w).map_err(|_| "too wide")?,
            u32::try_from(h).map_err(|_| "too tall")?,
        );
        Self::from_rgb(w, h, rgb).ok_or_else(|| "the decoder gave the wrong number of bytes".into())
    }

    /// Read and decode a JPEG file.
    ///
    /// # Errors
    ///
    /// If the file cannot be read or is not a JPEG.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The picture's size, in pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The picture scaled to cover `width` by `height`, centred and cropped,
    /// as opaque `0xFFRRGGBB` words a row at a time.
    ///
    /// Filtered rather than sampled: from 1920 wide to syslinux's 640 is a
    /// third, and taking every third pixel of a starfield keeps some stars and
    /// loses the rest. Each output pixel averages the source pixels it covers,
    /// under a tent that widens as the picture shrinks; going up, the same
    /// tent is plain bilinear.
    ///
    /// `None` if either side is zero, or a buffer that size cannot be indexed.
    #[must_use]
    pub fn cover(&self, width: u32, height: u32) -> Option<Vec<u32>> {
        if width == 0 || height == 0 {
            return None;
        }
        let (sw, sh) = (f64::from(self.width), f64::from(self.height));
        let (dw, dh) = (f64::from(width), f64::from(height));
        // The larger of the two ratios, so both sides are covered.
        let scale = (dw / sw).max(dh / sh);
        let across = taps(self.width, width, (sw - dw / scale) / 2.0, scale);
        let down = taps(self.height, height, (sh - dh / scale) / 2.0, scale);

        let src_w = usize::try_from(self.width).ok()?;
        let dst_w = usize::try_from(width).ok()?;
        let dst_h = usize::try_from(height).ok()?;
        // Only the source rows some output row reads need scaling across.
        let first = down.iter().map(|t| t.start).min()?;
        let last = down.iter().map(|t| t.start + t.weights.len()).max()?;

        // Across first, into rows of the output's width.
        let mut wide = vec![0.0_f32; (last - first).checked_mul(dst_w)?.checked_mul(3)?];
        for (row, out) in (first..last).zip(wide.chunks_exact_mut(dst_w * 3)) {
            let src = &self.rgb[row * src_w * 3..(row + 1) * src_w * 3];
            for (tap, px) in across.iter().zip(out.chunks_exact_mut(3)) {
                for (k, w) in tap.weights.iter().enumerate() {
                    let at = (tap.start + k) * 3;
                    for c in 0..3 {
                        px[c] += w * f32::from(src[at + c]);
                    }
                }
            }
        }

        // Then down, into the words.
        let mut words = Vec::with_capacity(dst_w.checked_mul(dst_h)?);
        for tap in &down {
            for x in 0..dst_w {
                let mut px = [0.0_f32; 3];
                for (k, w) in tap.weights.iter().enumerate() {
                    let at = ((tap.start - first + k) * dst_w + x) * 3;
                    for c in 0..3 {
                        px[c] += w * wide[at + c];
                    }
                }
                let [r, g, b] = px.map(channel);
                words.push(0xFF00_0000 | (r << 16) | (g << 8) | b);
            }
        }
        Some(words)
    }
}

/// One output pixel's share of a row or column of the source.
#[derive(Debug)]
struct Taps {
    /// The first source pixel it reads.
    start: usize,
    /// How much of each source pixel from `start` on, summing to one.
    weights: Vec<f32>,
}

/// The taps for each of `dst` output pixels along one axis.
///
/// `offset` is where the visible part of the source starts, in source pixels,
/// and `scale` how many output pixels one source pixel becomes.
fn taps(src: u32, dst: u32, offset: f64, scale: f64) -> Vec<Taps> {
    // A tent one source pixel either side going up, and one output pixel
    // either side, measured in source pixels, going down.
    let radius = (1.0 / scale).max(1.0);
    let last = f64::from(src) - 1.0;
    (0..dst)
        .map(|x| {
            // The output pixel's centre, in source pixel coordinates.
            let centre = offset + (f64::from(x) + 0.5) / scale - 0.5;
            let lo = (centre - radius).ceil().clamp(0.0, last);
            let hi = (centre + radius).floor().clamp(0.0, last);
            let mut weights: Vec<f32> = (0..=float_index(hi - lo))
                .map(|k| (1.0 - ((lo + f64::from(k)) - centre).abs() / radius).max(0.0))
                .map(narrow)
                .collect();
            let sum: f32 = weights.iter().sum();
            if sum > 0.0 {
                for w in &mut weights {
                    *w /= sum;
                }
            } else {
                // Only at the very edge, where the tent's one live pixel
                // was clamped away: take the nearest.
                weights = vec![1.0];
            }
            Taps {
                start: float_index(lo) as usize,
                weights,
            }
        })
        .collect()
}

/// A whole, non-negative `f64` as an index. The callers clamp it first.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn float_index(v: f64) -> u32 {
    v.max(0.0) as u32
}

/// A weight, which is between zero and one and loses nothing as an `f32`.
#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// A filtered channel back to a byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(v: f32) -> u32 {
    v.round().clamp(0.0, 255.0) as u32
}

#[cfg(test)]
mod tests {
    use super::Picture;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Picture {
        let bytes = (0..w * h).flat_map(|_| rgb).collect();
        Picture::from_rgb(w, h, bytes).unwrap()
    }

    /// A picture whose left half is one colour and right half another.
    fn halves(w: u32, h: u32, left: [u8; 3], right: [u8; 3]) -> Picture {
        let bytes = (0..h)
            .flat_map(|_| (0..w).flat_map(move |x| if x < w / 2 { left } else { right }))
            .collect();
        Picture::from_rgb(w, h, bytes).unwrap()
    }

    #[test]
    fn the_wrong_number_of_bytes_is_refused() {
        assert!(Picture::from_rgb(2, 2, vec![0; 11]).is_none());
        assert!(Picture::from_rgb(0, 2, vec![]).is_none());
    }

    #[test]
    fn a_flat_colour_stays_that_colour_at_every_size() {
        let p = solid(192, 108, [0x0b, 0x12, 0x1e]);
        for (w, h) in [(640, 480), (1280, 800), (1366, 768), (2560, 1440), (7, 3)] {
            let px = p.cover(w, h).unwrap();
            assert_eq!(px.len(), (w * h) as usize, "{w}x{h}");
            assert!(px.iter().all(|&v| v == 0xFF0B_121E), "{w}x{h}");
        }
    }

    #[test]
    fn a_narrower_screen_loses_the_sides_not_the_top_and_bottom() {
        // 16:9 onto 4:3 keeps the middle three quarters of the width. Red on
        // the left, blue on the right: the edges of the result are still pure
        // red and pure blue, because the crop never reaches a black bar.
        let p = halves(160, 90, [255, 0, 0], [0, 0, 255]);
        let px = p.cover(120, 90).unwrap();
        assert_eq!(px[0], 0xFFFF_0000);
        assert_eq!(px[119], 0xFF00_00FF);
        assert_eq!(px[120 * 89], 0xFFFF_0000);
    }

    #[test]
    fn the_crop_is_centred() {
        // The seam between the halves stays in the middle of the screen.
        let p = halves(200, 90, [255, 255, 255], [0, 0, 0]);
        let px = p.cover(90, 90).unwrap();
        let row = &px[..90];
        let white = row.iter().filter(|&&v| v == 0xFFFF_FFFF).count();
        let black = row.iter().filter(|&&v| v == 0xFF00_0000).count();
        assert!(white.abs_diff(black) <= 1, "{white} white, {black} black");
    }

    #[test]
    fn shrinking_averages_rather_than_samples() {
        // One-pixel stripes, a third the size: sampling would give solid
        // white or solid black depending on the phase. Filtered, it is grey.
        let bytes = (0..300 * 3)
            .flat_map(|i| if i % 2 == 0 { [255; 3] } else { [0; 3] })
            .collect();
        let p = Picture::from_rgb(300, 3, bytes).unwrap();
        let px = p.cover(100, 1).unwrap();
        for v in &px[1..99] {
            let r = (v >> 16) & 0xFF;
            assert!((100..=155).contains(&r), "{r}");
        }
    }

    #[test]
    fn what_is_not_a_jpeg_is_refused() {
        assert!(Picture::decode(b"not a picture").is_err());
        assert!(Picture::decode(&[]).is_err());
    }

    #[test]
    fn a_jpeg_decodes_to_its_own_size_and_colour() {
        let colour = [0x3b, 0x6e, 0x8d];
        let rgb: Vec<u8> = (0..48 * 32).flat_map(|_| colour).collect();
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 95)
            .encode(&rgb, 48, 32, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let picture = Picture::decode(&jpeg).unwrap();
        assert_eq!(picture.size(), (48, 32));
        for (got, want) in picture.rgb[..3].iter().zip(colour) {
            assert!(got.abs_diff(want) <= 3, "{got:#x} vs {want:#x}");
        }
    }
}

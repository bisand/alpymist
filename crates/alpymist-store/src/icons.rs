//! Application icons: PNGs from disk, cut to the size they are drawn at.
//!
//! Flathub's icons come at 64 and 128 pixels, and rows draw them at a size
//! the font decides. Denise scales pictures nearest-neighbour, which is right
//! for pixel art and wrong for an icon shrunk by a third, so each icon is
//! resampled once, by area, to exactly the size it is drawn at, and kept.
//! Only the icons on screen are ever decoded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A decoded icon: premultiplied `0xAARRGGBB`, `size` pixels square.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    /// Pixels, row by row.
    pub pixels: Vec<u32>,
    /// Width and height.
    pub size: u32,
}

/// Decoded icons by file and size. A file that could not be decoded is
/// remembered as such, so it is not tried at every frame.
#[derive(Debug, Default)]
pub struct Icons {
    cache: HashMap<(PathBuf, u32), Option<Icon>>,
}

/// How many icons are kept before the cache starts again.
const KEEP: usize = 512;

impl Icons {
    /// The icon at `path`, `size` pixels square.
    pub fn get(&mut self, path: &Path, size: u32) -> Option<&Icon> {
        let key = (path.to_path_buf(), size);
        if !self.cache.contains_key(&key) {
            if self.cache.len() >= KEEP {
                self.cache.clear();
            }
            let icon = load(path, size);
            self.cache.insert(key.clone(), icon);
        }
        self.cache.get(&key).and_then(Option::as_ref)
    }
}

/// Decode `path` and resample it to `size`.
#[must_use]
pub fn load(path: &Path, size: u32) -> Option<Icon> {
    let file = std::fs::File::open(path).ok()?;
    let image = decode(std::io::BufReader::new(file), 1 << 22)?;
    Some(square(&image, size))
}

/// A decoded picture: premultiplied red, green, blue and alpha, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    /// Pixels.
    pub pixels: Vec<[u32; 4]>,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Decode a PNG, refusing one of more than `max_pixels` pixels before its
/// pixels are allocated: a picture from the web could claim any size.
#[must_use]
#[allow(clippy::many_single_char_names)] // pixels: r, g, b, a
pub fn decode(source: impl std::io::BufRead + std::io::Seek, max_pixels: u64) -> Option<Rgba> {
    let mut decoder = png::Decoder::new_with_limits(
        source,
        png::Limits {
            bytes: usize::try_from(max_pixels.saturating_mul(4)).unwrap_or(usize::MAX),
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if u64::from(width) * u64::from(height) > max_pixels {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let bytes = buf.get(..info.buffer_size())?;
    let pixels = bytes
        .chunks_exact(channels)
        .map(|p| {
            let (r, g, b, a) = match *p {
                [v] => (v, v, v, 255),
                [v, a] => (v, v, v, a),
                [r, g, b] => (r, g, b, 255),
                [r, g, b, a] => (r, g, b, a),
                _ => (0, 0, 0, 0),
            };
            // Premultiplied before averaging, so a transparent pixel's colour
            // cannot bleed into the edge beside it.
            let a32 = u32::from(a);
            [
                u32::from(r) * a32 / 255,
                u32::from(g) * a32 / 255,
                u32::from(b) * a32 / 255,
                a32,
            ]
        })
        .collect();
    Some(Rgba {
        pixels,
        width: info.width,
        height: info.height,
    })
}

/// The size `width` by `height` becomes to fit inside `max_w` by `max_h`,
/// keeping its proportions; never zero.
#[must_use]
pub fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (max_w.max(1), max_h.max(1));
    }
    let by_width = (
        max_w,
        u32::try_from(u64::from(height) * u64::from(max_w) / u64::from(width)).unwrap_or(u32::MAX),
    );
    if by_width.1 <= max_h {
        return (by_width.0.max(1), by_width.1.max(1));
    }
    let w =
        u32::try_from(u64::from(width) * u64::from(max_h) / u64::from(height)).unwrap_or(u32::MAX);
    (w.max(1), max_h.max(1))
}

/// `image` letterboxed into a `size` square.
fn square(image: &Rgba, size: u32) -> Icon {
    let size = size.max(1);
    let (dw, dh) = fit(image.width, image.height, size, size);
    let scaled = scale(image, dw, dh);
    let mut pixels = vec![0u32; (size * size) as usize];
    let (ox, oy) = ((size - dw) / 2, (size - dh) / 2);
    for (y, row) in scaled.chunks_exact(dw as usize).enumerate() {
        let start = ((oy as usize) + y) * size as usize + ox as usize;
        pixels[start..start + dw as usize].copy_from_slice(row);
    }
    Icon { pixels, size }
}

/// Area-average `src` to exactly `dw` by `dh`, as premultiplied
/// `0xAARRGGBB` words.
// Indices stay below the picture's pixel count, which fits in usize.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]
pub fn scale(src: &Rgba, dw: u32, dh: u32) -> Vec<u32> {
    let (w, h) = (u64::from(src.width), u64::from(src.height));
    let (dw, dh) = (u64::from(dw.max(1)), u64::from(dh.max(1)));
    let mut pixels = vec![0u32; (dw * dh) as usize];
    if w == 0 || h == 0 || (src.pixels.len() as u64) < w * h {
        return pixels;
    }
    for dy in 0..dh {
        // Source rows covered by this destination row, at least one.
        let y0 = dy * h / dh;
        let y1 = ((dy + 1) * h).div_ceil(dh).max(y0 + 1).min(h);
        for dx in 0..dw {
            let x0 = dx * w / dw;
            let x1 = ((dx + 1) * w).div_ceil(dw).max(x0 + 1).min(w);
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                let row = (sy * w) as usize;
                for p in &src.pixels[row + x0 as usize..row + x1 as usize] {
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += u64::from(*v);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1);
            let c = |i: usize| u32::try_from(sum[i] / n).unwrap_or(255).min(255);
            pixels[(dy * dw + dx) as usize] = (c(3) << 24) | (c(0) << 16) | (c(1) << 8) | c(2);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::{Rgba, fit, scale, square};

    fn image(pixels: Vec<[u32; 4]>, width: u32, height: u32) -> Rgba {
        Rgba {
            pixels,
            width,
            height,
        }
    }

    #[test]
    fn halving_averages_and_keeps_alpha_premultiplied() {
        // A 2x2 of opaque white and transparent: half-covered grey-white.
        let white = [255, 255, 255, 255];
        let clear = [0, 0, 0, 0];
        let icon = square(&image(vec![white, clear, clear, white], 2, 2), 1);
        assert_eq!(icon.pixels, vec![0x7F7F_7F7F]);
    }

    #[test]
    fn a_wide_picture_is_letterboxed() {
        let red = [255, 0, 0, 255];
        let icon = square(&image(vec![red; 4 * 2], 4, 2), 4);
        // Rows 0 and 3 are empty; 1 and 2 are red.
        assert_eq!(icon.pixels[0], 0);
        assert_eq!(icon.pixels[4], 0xFFFF_0000);
        assert_eq!(icon.pixels[12], 0);
    }

    #[test]
    fn pictures_fit_their_box_and_keep_their_shape() {
        assert_eq!(fit(1248, 787, 624, 600), (624, 393));
        assert_eq!(fit(800, 1200, 400, 300), (200, 300));
        assert_eq!(
            scale(&image(vec![[255, 255, 255, 255]; 16], 4, 4), 3, 2).len(),
            6
        );
    }
}

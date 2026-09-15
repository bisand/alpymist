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
#[allow(clippy::many_single_char_names)] // pixels: r, g, b, a
pub fn load(path: &Path, size: u32) -> Option<Icon> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let bytes = buf.get(..info.buffer_size())?;
    let rgba: Vec<[u32; 4]> = bytes
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
    Some(resample(&rgba, w, h, size))
}

/// Area-average `src`, `w` by `h`, into a `size` square, centred and letterboxed
/// when it is not square.
// Indices stay below size², which fits in usize.
#[allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]
fn resample(src: &[[u32; 4]], w: u32, h: u32, size: u32) -> Icon {
    let size = size.max(1);
    let mut pixels = vec![0u32; (size * size) as usize];
    if w == 0 || h == 0 || src.len() < (w * h) as usize {
        return Icon { pixels, size };
    }
    let longest = w.max(h);
    // The picture's box inside the square, in destination pixels.
    let (dw, dh) = (
        (u64::from(w) * u64::from(size) / u64::from(longest)).max(1),
        (u64::from(h) * u64::from(size) / u64::from(longest)).max(1),
    );
    let (ox, oy) = ((u64::from(size) - dw) / 2, (u64::from(size) - dh) / 2);
    for dy in 0..dh {
        // Source rows covered by this destination row, at least one.
        let y0 = dy * u64::from(h) / dh;
        let y1 = ((dy + 1) * u64::from(h)).div_ceil(dh).max(y0 + 1);
        for dx in 0..dw {
            let x0 = dx * u64::from(w) / dw;
            let x1 = ((dx + 1) * u64::from(w)).div_ceil(dw).max(x0 + 1);
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for sy in y0..y1.min(u64::from(h)) {
                for sx in x0..x1.min(u64::from(w)) {
                    let p = src[(sy * u64::from(w) + sx) as usize];
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += u64::from(v);
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            let c = |i: usize| u32::try_from(sum[i] / n).unwrap_or(255).min(255);
            let word = (c(3) << 24) | (c(0) << 16) | (c(1) << 8) | c(2);
            pixels[((oy + dy) * u64::from(size) + ox + dx) as usize] = word;
        }
    }
    Icon { pixels, size }
}

#[cfg(test)]
mod tests {
    use super::resample;

    #[test]
    fn halving_averages_and_keeps_alpha_premultiplied() {
        // A 2x2 of opaque white and transparent: half-covered grey-white.
        let white = [255, 255, 255, 255];
        let clear = [0, 0, 0, 0];
        let icon = resample(&[white, clear, clear, white], 2, 2, 1);
        assert_eq!(icon.pixels, vec![0x7F7F_7F7F]);
    }

    #[test]
    fn a_wide_picture_is_letterboxed() {
        let red = [255, 0, 0, 255];
        let icon = resample(&[red; 4 * 2], 4, 2, 4);
        // Rows 0 and 3 are empty; 1 and 2 are red.
        assert_eq!(icon.pixels[0], 0);
        assert_eq!(icon.pixels[4], 0xFFFF_0000);
        assert_eq!(icon.pixels[12], 0);
    }
}

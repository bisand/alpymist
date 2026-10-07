//! The cards' faces: pictures in the program, scaled to the size drawn at.
//!
//! Fifty-two PNGs of 300 by 436 are carried in the binary, 750 KiB of it,
//! and twelve more to choose the backs from.
//! Denise scales pictures to the nearest pixel, which is wrong for a picture
//! shrunk to a third, so each face is resampled by area to exactly the size
//! of a card in this window, the first time it is shown, and kept until the
//! window changes size. A face is decoded only when a card shows it.

use crate::cards::Card;

/// A face at the size it is drawn: opaque `0xFFRRGGBB`, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// Pixels.
    pub pixels: Vec<u32>,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// The faces, at one size, and the picture on the cards' backs.
#[derive(Debug, Default)]
pub struct Faces {
    size: (u32, u32),
    scaled: Vec<Option<Face>>,
    back: Option<(usize, Face)>,
}

/// The backs there are to choose from: what each is called, and its picture.
pub const BACKS: [(&str, &[u8]); 12] = [
    ("Mist", include_bytes!("../cards/back-01.png")),
    ("Night", include_bytes!("../cards/back-02.png")),
    ("Crimson", include_bytes!("../cards/back-03.png")),
    ("Navy", include_bytes!("../cards/back-04.png")),
    ("Aurora", include_bytes!("../cards/back-05.png")),
    ("Sunset", include_bytes!("../cards/back-06.png")),
    ("Deco", include_bytes!("../cards/back-07.png")),
    ("Wave", include_bytes!("../cards/back-08.png")),
    ("Neon", include_bytes!("../cards/back-09.png")),
    ("Garden", include_bytes!("../cards/back-10.png")),
    ("8-bit", include_bytes!("../cards/back-11.png")),
    ("16-bit", include_bytes!("../cards/back-12.png")),
];

macro_rules! pictures {
    ($($name:literal),* $(,)?) => {
        [$(include_bytes!(concat!("../cards/", $name, ".png")).as_slice()),*]
    };
}

/// The PNGs, by [`Card::index`].
const PICTURES: [&[u8]; 52] = pictures![
    "c01", "c02", "c03", "c04", "c05", "c06", "c07", "c08", "c09", "c10", "c11", "c12", "c13",
    "d01", "d02", "d03", "d04", "d05", "d06", "d07", "d08", "d09", "d10", "d11", "d12", "d13",
    "h01", "h02", "h03", "h04", "h05", "h06", "h07", "h08", "h09", "h10", "h11", "h12", "h13",
    "s01", "s02", "s03", "s04", "s05", "s06", "s07", "s08", "s09", "s10", "s11", "s12", "s13",
];

/// A picture's width over its height, as 300 to 436.
pub const ASPECT: (i32, i32) = (300, 436);

impl Faces {
    /// `card`'s face, `width` by `height` pixels.
    pub fn get(&mut self, card: Card, width: u32, height: u32) -> Option<&Face> {
        if self.size != (width, height) || self.scaled.len() != PICTURES.len() {
            self.size = (width, height);
            self.scaled = vec![None; PICTURES.len()];
        }
        let slot = self.scaled.get_mut(card.index())?;
        if slot.is_none() {
            *slot = decode(PICTURES[card.index()]).map(|p| scale(&p, width, height));
        }
        slot.as_ref()
    }

    /// The `which`th of [`BACKS`], `width` by `height` pixels.
    pub fn back(&mut self, which: usize, width: u32, height: u32) -> Option<&Face> {
        let kept = self
            .back
            .as_ref()
            .is_some_and(|(i, b)| (*i, b.width, b.height) == (which, width, height));
        if !kept {
            let (_, bytes) = BACKS.get(which)?;
            self.back = decode(bytes).map(|p| (which, scale(&p, width, height)));
        }
        self.back.as_ref().map(|(_, face)| face)
    }
}

/// A decoded picture: red, green and blue, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb {
    /// Pixels.
    pub pixels: Vec<[u8; 3]>,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Decode one of the program's own PNGs. Transparency is dropped: the faces
/// have none.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<Rgb> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let pixels = buf
        .get(..info.buffer_size())?
        .chunks_exact(channels)
        .map(|p| match *p {
            [v] | [v, _] => [v, v, v],
            [r, g, b] | [r, g, b, _] => [r, g, b],
            _ => [0, 0, 0],
        })
        .collect();
    Some(Rgb {
        pixels,
        width: info.width,
        height: info.height,
    })
}

/// Area-average `src` to exactly `dw` by `dh`.
// Indices stay below the picture's pixel count, which fits in usize.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn scale(src: &Rgb, dw: u32, dh: u32) -> Face {
    let (w, h) = (u64::from(src.width), u64::from(src.height));
    let (width, height) = (dw.max(1), dh.max(1));
    let (dw, dh) = (u64::from(width), u64::from(height));
    let mut pixels = vec![0xFFFF_FFFFu32; (dw * dh) as usize];
    if w == 0 || h == 0 || (src.pixels.len() as u64) < w * h {
        return Face {
            pixels,
            width,
            height,
        };
    }
    for dy in 0..dh {
        // Source rows covered by this destination row, at least one.
        let y0 = dy * h / dh;
        let y1 = ((dy + 1) * h).div_ceil(dh).max(y0 + 1).min(h);
        for dx in 0..dw {
            let x0 = dx * w / dw;
            let x1 = ((dx + 1) * w).div_ceil(dw).max(x0 + 1).min(w);
            let mut sum = [0u64; 3];
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
            pixels[(dy * dw + dx) as usize] = 0xFF00_0000 | (c(0) << 16) | (c(1) << 8) | c(2);
        }
    }
    Face {
        pixels,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::{ASPECT, Faces, PICTURES, Rgb, decode, scale};
    use crate::cards::deck;

    #[test]
    fn every_card_has_a_picture_of_the_size_said() {
        for (card, bytes) in deck().into_iter().zip(PICTURES) {
            let picture = decode(bytes).unwrap_or_else(|| panic!("{card:?} does not decode"));
            assert_eq!(
                (picture.width, picture.height),
                (300, 436),
                "{card:?} is another size"
            );
        }
        assert_eq!(ASPECT, (300, 436));
        let mut faces = Faces::default();
        for (which, (name, bytes)) in super::BACKS.iter().enumerate() {
            let back = decode(bytes).unwrap_or_else(|| panic!("the back {name} does not decode"));
            assert_eq!((back.width, back.height), (300, 436), "{name}");
            assert_eq!(
                faces.back(which, 27, 40).map(|b| b.pixels.len()),
                Some(27 * 40)
            );
        }
        assert!(faces.back(super::BACKS.len(), 27, 40).is_none());
    }

    /// A red card's corner is red and a black one's is not: the pictures are
    /// in the order the cards are.
    #[test]
    fn the_pictures_are_in_the_cards_order() {
        for (card, bytes) in deck().into_iter().zip(PICTURES) {
            let picture = decode(bytes).unwrap();
            let red = picture
                .pixels
                .iter()
                .filter(|[r, g, b]| *r > 180 && *g < 80 && *b < 80)
                .count();
            // A court card of a black suit wears red too, but less of it
            // than a red suit's pips and letters make.
            if card.rank <= 10 {
                assert_eq!(red > 500, card.suit.red(), "{card:?}: {red} red pixels");
            }
        }
    }

    #[test]
    fn scaling_averages_and_is_kept_until_the_size_changes() {
        let src = Rgb {
            pixels: vec![[0, 0, 0], [255, 255, 255], [255, 255, 255], [0, 0, 0]],
            width: 2,
            height: 2,
        };
        let one = scale(&src, 1, 1);
        assert_eq!(one.pixels, [0xFF7F_7F7F]);
        let same = scale(&src, 2, 2);
        assert_eq!(same.pixels[0], 0xFF00_0000);
        assert_eq!(same.pixels[1], 0xFFFF_FFFF);

        let mut faces = Faces::default();
        let card = deck()[0];
        let first = faces.get(card, 30, 44).cloned().unwrap();
        assert_eq!((first.width, first.height), (30, 44));
        assert_eq!(first.pixels.len(), 30 * 44);
        assert_eq!(faces.get(card, 30, 44), Some(&first));
        assert_eq!(faces.get(card, 60, 87).unwrap().width, 60);
    }
}

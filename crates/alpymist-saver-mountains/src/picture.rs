//! The mountains: the wallpaper's own ranges, travelling past.
//!
//! The first version of this drew the ranges once and moved only the mist over
//! them. It was a nice picture and a poor screensaver: the skyline sat in the
//! same pixels for as long as the machine was left alone, which is exactly what
//! a screensaver exists not to do. So everything here moves.
//!
//! The ranges pan sideways, each at its own speed — the nearest fastest, which
//! is the parallax that puts them behind one another — over terrain twice the
//! width of the picture, folded so that panning wraps with no seam. The mist
//! drifts across them on its own slower cycle and breathes as it goes, and the
//! stars cross the sky slowest of all. Between them there is no pixel that
//! holds one colour for long.
//!
//! What the shared host does with the result — the magnification, the frame
//! pacing, the input that takes it away — is none of this program's business,
//! and is none of any other screensaver's either. This crate is the picture.

use alpymist_screensaver::paint::Painting;
use alpymist_screensaver::scene::{Motion, UNIT, drift, haze, motions, reduced, sine};
use alpymist_ui::backdrop::{Backdrop, Layer};
use alpymist_ui::palette::{Palette, Rgb};
use denise::geom::Size;

/// The seed that fixes the mountains.
///
/// The wallpaper, the splash and the installer all draw this same value, so the
/// screensaver opens on the ranges that were already on the desktop and then
/// carries them away.
pub const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// What the settings make of this picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Physical pixels to one drawn pixel.
    pub block: u32,
    /// How much mist there is, as a percentage of what the scene composes.
    pub mist: i32,
    /// How fast everything travels, in columns a minute for the nearest range.
    pub drift: i32,
}

impl Default for Look {
    /// What the definition file says, repeated here so the picture still draws
    /// when it is run with no definition beside it at all.
    fn default() -> Self {
        Self {
            block: 6,
            mist: 100,
            drift: 14,
        }
    }
}

/// One band of mist.
///
/// The backdrop composes its mist as a flat translucent band right across the
/// picture. At full resolution, with its alpha ramped away at both edges, that
/// reads as mist; at a sixth of the resolution it is three rows tall and reads
/// as a scan line. So the bands are taken out of the scene here and painted
/// patchy across their width and drifting sideways instead.
struct Band {
    /// The row it rests at.
    y: u32,
    /// How many rows it covers.
    height: u32,
    /// Its colour.
    colour: Rgb,
    /// The opacity at its thickest.
    alpha: u8,
    /// Its rise, fall and swell.
    motion: Motion,
    /// Columns it drifts sideways in a minute.
    speed: i32,
    /// What makes this band's patchiness its own.
    seed: i32,
}

/// One range of mountains, and how fast it travels.
struct Range {
    /// The skyline's row at each column, over twice the picture's width.
    ///
    /// Twice, with the second half the first half reversed, so that panning
    /// wraps with no seam: the last column and the first are neighbours in the
    /// terrain as well as in the arithmetic. At this resolution, over the
    /// minutes it takes to travel that far, the reflection reads as more
    /// mountains rather than as a mirror.
    tops: Vec<i32>,
    /// Its colour at each column of `tops`, already hazed for its distance and
    /// shaded by how high the ground stands there.
    ///
    /// A range painted in one flat colour is the one thing panning cannot
    /// save: the pixels under the skyline would hold that colour for as long
    /// as the machine was left alone, however far the outline travelled. A
    /// column's own shade travels with it, so they change too.
    colour: Vec<u32>,
    /// Columns it travels in a minute. The nearest travels furthest.
    speed: i32,
}

/// A star in the upper sky.
struct Star {
    /// Its column in the extended sky, which wraps as the ranges do.
    x: u32,
    /// Its row.
    y: u32,
    /// How bright it gets.
    ink: u8,
    /// Where in its own twinkle it starts, in degrees.
    phase: i64,
}

/// The scene at its reduced size, ready to animate.
pub struct Mountains {
    /// The reduced size everything is drawn at.
    small: Size,
    /// Physical pixels to one drawn pixel.
    block: u32,
    /// The sky's colour at each row: it is bands, so one value a row is all.
    sky: Vec<u32>,
    /// The ranges, furthest first, which is the order they are painted in.
    ranges: Vec<Range>,
    stars: Vec<Star>,
    /// The frame being drawn.
    pixels: Vec<u32>,
    bands: Vec<Band>,
    /// A band's thickness at each column, worked out once per band per frame.
    ///
    /// [`haze`] depends on the column and not the row, so computing it inside
    /// the row loop did the same sixteen sines sixteen times over — which on an
    /// Atom was most of what a frame cost.
    across: Vec<i32>,
    /// Each range's skyline at each column after this frame's pan, worked out
    /// once and then read down the rows.
    skyline: Vec<i32>,
    /// And the colour each of those columns is painted in.
    shades: Vec<u32>,
}

/// How far the stars travel for every column the nearest range does.
///
/// Slowest of everything: they are the furthest away. Not still, though — the
/// top of the sky is the one part of the picture the ranges never reach, so if
/// the stars did not move nothing up there ever would.
const STAR_SHARE: i32 = 8;

impl Mountains {
    /// Compose for an output of this size, as the settings ask for.
    pub fn compose(output: Size, settings: &Look) -> Self {
        let (small, block) = reduced(output, settings.block);
        let backdrop =
            Backdrop::compose(small.width, small.height, &Palette::alpymist(), SCENE_SEED);

        let mut sky = vec![opaque(Palette::alpymist().sky_high); small.height as usize];
        let mut ranges: Vec<Range> = Vec::new();
        let mut taken = Vec::new();
        for layer in &backdrop.layers {
            match layer {
                Layer::Sky { y, height, colour } => {
                    let top = (*y as usize).min(sky.len());
                    let end = (top + *height as usize).min(sky.len());
                    for row in &mut sky[top..end] {
                        *row = opaque(*colour);
                    }
                }
                Layer::Mountain {
                    columns, colour, ..
                } => {
                    let tops = seamless(columns, small.width);
                    let shades = shade(&tops, *colour, small.height, &Palette::alpymist());
                    ranges.push(Range {
                        tops,
                        colour: shades,
                        // Filled in below, once it is known how many there are.
                        speed: 0,
                    });
                }
                Layer::Mist {
                    y,
                    height,
                    colour,
                    alpha,
                } => taken.push((*y, *height, *colour, *alpha)),
            }
        }

        // The composed scene puts the furthest range first. The nearest travels
        // at the speed the settings ask for and the rest in proportion, which
        // is what makes them read as being at different distances rather than
        // as one flat picture sliding past.
        let count = i32::try_from(ranges.len()).unwrap_or(1).max(1);
        let fastest = settings.drift.max(0);
        for (i, range) in ranges.iter_mut().enumerate() {
            let nearness = i32::try_from(i).unwrap_or(0) + 1;
            range.speed = (fastest * nearness / count).max(i32::from(fastest > 0));
        }

        let motions = motions(taken.len());
        let bands = taken
            .into_iter()
            .zip(motions)
            .enumerate()
            .map(|(i, ((y, height, colour, alpha), motion))| {
                let i = i32::try_from(i).unwrap_or(0);
                Band {
                    // Taller than the composed band, and centred on it: what
                    // the ramp has to work with is what stops it being a line.
                    y: y.saturating_sub(height / 2),
                    height: (height * 2).max(4),
                    colour,
                    // How much mist there is at all, as the setting asks.
                    alpha: u8::try_from(i32::from(alpha) * settings.mist / 100)
                        .unwrap_or(alpha)
                        .max(1),
                    motion,
                    speed: (fastest + i * fastest / 2).max(1),
                    seed: 40 + i * 113,
                }
            })
            .collect();

        let len = small.width as usize * small.height as usize;
        Self {
            small,
            block,
            sky,
            stars: stars(small),
            ranges,
            pixels: vec![0; len],
            bands,
            across: Vec::new(),
            skyline: Vec::new(),
            shades: Vec::new(),
        }
    }

    /// Draw the frame at `elapsed` milliseconds.
    fn paint(&mut self, elapsed: u64) {
        self.paint_sky();
        self.paint_stars(elapsed);
        self.paint_ranges(elapsed);
        self.paint_mist(elapsed);
    }

    /// The sky, which is one colour a row.
    fn paint_sky(&mut self) {
        let width = self.small.width as usize;
        for (y, row) in self.pixels.chunks_exact_mut(width).enumerate() {
            row.fill(self.sky.get(y).copied().unwrap_or(0xFF00_0000));
        }
    }

    /// The stars, crossing the sky and twinkling as they go.
    fn paint_stars(&mut self, elapsed: u64) {
        let width = self.small.width;
        let extended = width.saturating_mul(2).max(1);
        let fastest = self.ranges.last().map_or(0, |r| r.speed);
        let slide = drift(elapsed, (fastest / STAR_SHARE).max(1));
        for star in &self.stars {
            // Travelling the other way from the ranges would read as the sky
            // sliding over the ground; they go the same way, slower.
            let at = i64::from(star.x) - i64::from(slide);
            let at = at.rem_euclid(i64::from(extended));
            let Ok(x) = u32::try_from(at) else { continue };
            if x >= width {
                continue;
            }
            // A slow, shallow twinkle: never out, never at full for long.
            let turn = i64::try_from(elapsed % 9_000).unwrap_or(0) * 360 / 9_000 + star.phase;
            let lift = 70 + sine(turn) * 30 / UNIT;
            let alpha = u8::try_from((i32::from(star.ink) * lift / 100).clamp(0, 255)).unwrap_or(0);
            let at = star.y as usize * width as usize + x as usize;
            if let Some(px) = self.pixels.get_mut(at) {
                *px = over(*px, Palette::alpymist().ink, alpha);
            }
        }
    }

    /// The ranges, each at the offset its own speed has carried it to.
    fn paint_ranges(&mut self, elapsed: u64) {
        let width = self.small.width as usize;
        let height = self.small.height;
        let count = self.ranges.len();
        if count == 0 {
            return;
        }
        // Every range's skyline for this frame, worked out once. Reading it
        // down the rows afterwards keeps the painting row-major, which on a
        // cache this small is most of the difference.
        self.skyline.clear();
        self.skyline.resize(count * width, i32::MAX);
        self.shades.clear();
        self.shades.resize(count * width, 0xFF00_0000);
        for (r, range) in self.ranges.iter().enumerate() {
            let extended = range.tops.len();
            if extended == 0 {
                continue;
            }
            let slide = drift(elapsed, range.speed);
            let base = r * width;
            for x in 0..width {
                let at = (i64::try_from(x).unwrap_or(0) + i64::from(slide))
                    .rem_euclid(i64::try_from(extended).unwrap_or(1));
                let at = usize::try_from(at).unwrap_or(0);
                self.skyline[base + x] = range.tops.get(at).copied().unwrap_or(i32::MAX);
                self.shades[base + x] = range.colour.get(at).copied().unwrap_or(0xFF00_0000);
            }
        }
        for y in 0..height {
            let row_at = y as usize * width;
            let Some(row) = self.pixels.get_mut(row_at..row_at + width) else {
                continue;
            };
            let row_y = i32::try_from(y).unwrap_or(0);
            for (x, px) in row.iter_mut().enumerate() {
                // Nearest first: the first range whose skyline has been reached
                // is the one you can see, and the ones behind it do not matter.
                for r in (0..count).rev() {
                    if self.skyline[r * width + x] <= row_y {
                        *px = self.shades[r * width + x];
                        break;
                    }
                }
            }
        }
    }

    /// The mist, drifting across whatever is behind it.
    fn paint_mist(&mut self, elapsed: u64) {
        let width = self.small.width as usize;
        let rows = self.small.height;

        for band in &self.bands {
            let (rise, swell) = band.motion.at(elapsed);
            let slide = drift(elapsed, band.speed);
            let top = band.y.saturating_add_signed(rise);
            let half = (band.height / 2).max(1);

            // The band's thickness across the picture: the same for every row
            // of it, so worked out once and read back per row.
            self.across.clear();
            self.across
                .extend((0..self.small.width).map(|x| haze(x, slide, band.seed) * swell / 100));

            for step in 0..band.height {
                let y = top.saturating_add(step);
                if y >= rows {
                    break;
                }
                // Triangular down the band: nothing at the edges, everything in
                // the middle, which is what keeps it from having a top and a
                // bottom you can point at.
                let from_centre =
                    i32::try_from(step).unwrap_or(0) - i32::try_from(half).unwrap_or(1);
                let ramp = (i32::try_from(half).unwrap_or(1) - from_centre.abs()).max(0);
                let down = i32::from(band.alpha) * ramp / i32::try_from(half).unwrap_or(1);
                if down == 0 {
                    continue;
                }
                let at = y as usize * width;
                let Some(line) = self.pixels.get_mut(at..at + width) else {
                    continue;
                };
                for (px, across) in line.iter_mut().zip(&self.across) {
                    let alpha = down * across / 100;
                    let Ok(alpha) = u8::try_from(alpha.clamp(0, 255)) else {
                        continue;
                    };
                    if alpha == 0 {
                        continue;
                    }
                    *px = over(*px, band.colour, alpha);
                }
            }
        }
    }
}

impl Painting for Mountains {
    fn small(&self) -> Size {
        self.small
    }

    fn block(&self) -> u32 {
        self.block
    }

    fn frame(&mut self, elapsed: u64) -> &[u32] {
        self.paint(elapsed);
        &self.pixels
    }
}

/// A palette colour as an opaque pixel.
fn opaque(c: Rgb) -> u32 {
    u32::from_be_bytes([0xFF, c.r, c.g, c.b])
}

/// `ink` laid over `under` at `alpha`, both opaque ARGB.
fn over(under: u32, ink: Rgb, alpha: u8) -> u32 {
    let [_, r, g, b] = under.to_be_bytes();
    let mix = |under: u8, ink: u8| {
        let a = u32::from(alpha);
        let blended = (u32::from(ink) * a + u32::from(under) * (255 - a)) / 255;
        u8::try_from(blended.min(255)).unwrap_or(255)
    };
    u32::from_be_bytes([0xFF, mix(r, ink.r), mix(g, ink.g), mix(b, ink.b)])
}

/// A range's skyline over twice the picture's width, folded so it wraps.
///
/// The composed terrain is as wide as the picture and its two ends have nothing
/// to do with each other, so panning across it would step off a cliff once a
/// lap. Following it with its own reflection costs one more array and makes the
/// seam impossible rather than merely unlikely.
fn seamless(columns: &[(i32, i32, i32)], width: u32) -> Vec<i32> {
    let width = width.max(1) as usize;
    let mut tops: Vec<i32> = columns.iter().take(width).map(|&(_, top, _)| top).collect();
    if tops.is_empty() {
        return vec![0; width * 2];
    }
    // A short scene still gets a full lap, or the fold would be visible.
    while tops.len() < width {
        let last = *tops.last().unwrap_or(&0);
        tops.push(last);
    }
    let reflected: Vec<i32> = tops.iter().rev().copied().collect();
    tops.extend(reflected);
    tops
}

/// A range's colour at every column, shaded by how high its ground stands.
///
/// Gentle — a tenth of the way towards the haze at most. Enough that the mass
/// below a skyline is not one dead colour, little enough that it reads as
/// slopes rather than as stripes.
fn shade(tops: &[i32], colour: Rgb, height: u32, palette: &Palette) -> Vec<u32> {
    let height = i32::try_from(height.max(1)).unwrap_or(1);
    tops.iter()
        .map(|&top| {
            let depth = top.clamp(0, height);
            // Ground that stands high is darker; ground that lies low catches
            // more of the haze behind it.
            let amount = u32::try_from(depth * 12 / height).unwrap_or(0).min(12);
            opaque(colour.mix(palette.sky_low, amount))
        })
        .collect()
}

/// Where the stars are, for a picture this size.
///
/// Fixed by the scene's own seed, so the same sky comes back every time rather
/// than the picture being subtly different at every appearance.
fn stars(small: Size) -> Vec<Star> {
    let width = small.width.max(1);
    let height = small.height.max(1);
    // Only the upper sky: lower down the ranges cover them within a lap, and a
    // star that spends its life behind a mountain is work for nothing.
    let ceiling = (height / 2).max(1);
    let count = (width / 5).clamp(8, 120);
    let mut seed = SCENE_SEED;
    let mut next = || {
        // A plain 64-bit LCG: it needs to be arbitrary, not unpredictable, and
        // a dependency for that would be a dependency in a screensaver.
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as u32
    };
    (0..count)
        .map(|_| Star {
            x: next() % (width * 2),
            y: next() % ceiling,
            // Mostly faint, a few bright: an even spread reads as a grid.
            ink: u8::try_from(40 + next() % 160).unwrap_or(120),
            phase: i64::from(next() % 360),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Look, Mountains, over, seamless, stars};
    use alpymist_screensaver::paint::Painting;
    use alpymist_ui::palette::Rgb;
    use denise::geom::Size;

    fn scene(w: u32, h: u32) -> Mountains {
        Mountains::compose(
            Size::new(w, h),
            &Look {
                block: 4,
                ..Look::default()
            },
        )
    }

    #[test]
    fn nothing_it_draws_is_transparent() {
        let mut m = scene(640, 480);
        for ms in (0..120_000).step_by(3_000) {
            assert!(
                m.frame(ms).iter().all(|px| px >> 24 == 0xFF),
                "a transparent pixel would show the desktop through"
            );
        }
    }

    #[test]
    fn it_hands_back_exactly_the_pixels_it_says_it_has() {
        let mut m = scene(1366, 768);
        let small = m.small();
        assert_eq!(
            m.frame(0).len(),
            small.width as usize * small.height as usize
        );
    }

    /// The whole point: a screensaver whose picture stands still saves nothing.
    #[test]
    fn every_part_of_the_picture_moves_over_time() {
        let mut m = scene(640, 480);
        let width = m.small().width as usize;
        let height = m.small().height as usize;
        let first = m.frame(0).to_vec();

        // Three minutes, which is less than one lap of even the fastest range.
        let later = m.frame(180_000).to_vec();
        assert_ne!(first, later, "nothing moved at all");

        // Not just the middle: the sky at the top and the ground at the bottom
        // must both have changed, or something is still burning in.
        let band = |px: &[u32], from: usize, to: usize| px[from * width..to * width].to_vec();
        assert_ne!(
            band(&first, 0, height / 4),
            band(&later, 0, height / 4),
            "the top of the sky never changes"
        );
        assert_ne!(
            band(&first, height * 3 / 4, height),
            band(&later, height * 3 / 4, height),
            "the ground never changes"
        );
    }

    #[test]
    fn a_range_pans_without_a_seam_to_step_over() {
        let columns: Vec<(i32, i32, i32)> = (0..8).map(|x| (x, x * 3, 0)).collect();
        let tops = seamless(&columns, 8);
        assert_eq!(tops.len(), 16, "the picture's width, and its reflection");
        assert_eq!(tops[0], tops[15], "the ends meet, so a lap has no cliff");
        assert_eq!(tops[7], tops[8], "and the fold is a plateau, not a jump");
    }

    #[test]
    fn a_range_with_no_terrain_still_gives_a_full_lap() {
        assert_eq!(seamless(&[], 6).len(), 12);
        let short: Vec<(i32, i32, i32)> = (0..2).map(|x| (x, 5, 0)).collect();
        assert_eq!(seamless(&short, 6).len(), 12);
    }

    #[test]
    fn the_stars_are_in_the_upper_sky_and_within_a_lap() {
        let small = Size::new(200, 120);
        let sky = stars(small);
        assert!(!sky.is_empty(), "an empty sky on a screen this size");
        for star in &sky {
            assert!(star.y < small.height / 2, "a star down among the mountains");
            assert!(star.x < small.width * 2, "a star off the end of the lap");
            assert!(star.ink > 0, "a star nobody can see");
        }
    }

    #[test]
    fn the_same_sky_comes_back_rather_than_a_new_one_each_time() {
        let a = stars(Size::new(200, 120));
        let b = stars(Size::new(200, 120));
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(p, q)| p.x == q.x && p.y == q.y));
    }

    #[test]
    fn a_screen_of_a_different_size_is_a_different_picture() {
        let look = Look {
            block: 4,
            ..Look::default()
        };
        let a = Mountains::compose(Size::new(800, 600), &look);
        let b = Mountains::compose(Size::new(1920, 1080), &look);
        assert_ne!(
            a.small(),
            b.small(),
            "the host recomposes; this is what it gets"
        );
        assert_eq!(a.small(), Size::new(200, 150));
    }

    #[test]
    fn nothing_moving_is_a_setting_and_not_a_panic() {
        let still = Look {
            block: 4,
            mist: 0,
            drift: 0,
        };
        let mut m = Mountains::compose(Size::new(320, 240), &still);
        assert!(m.frame(0).iter().all(|px| px >> 24 == 0xFF));
        assert!(m.frame(600_000).iter().all(|px| px >> 24 == 0xFF));
    }

    #[test]
    fn a_picture_left_up_for_days_does_not_panic_or_wrap() {
        let mut m = scene(800, 600);
        for ms in [0, 60_000, 3_600_000, 172_800_000, u64::MAX / 2] {
            m.paint(ms);
        }
    }

    #[test]
    fn laying_ink_over_a_colour_stays_between_the_two() {
        let under = 0xFF_00_00_00;
        let ink = Rgb::new(0xFF, 0xFF, 0xFF);
        assert_eq!(over(under, ink, 0), under, "nothing at all");
        assert_eq!(over(under, ink, 255), 0xFF_FF_FF_FF, "all of it");
        let half = over(under, ink, 128);
        assert_eq!(half >> 24, 0xFF);
        assert!((0x70..=0x90).contains(&((half >> 16) & 0xFF)), "{half:08x}");
    }
}

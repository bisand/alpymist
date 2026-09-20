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
//! The second version moved all of that at fourteen columns a minute, which is
//! a column every four seconds: arithmetically travelling, and to anyone
//! looking at it a still picture. What a side-scroller does — and this is one,
//! the same trick the platform games drew their skylines with — is move about
//! a column a frame, and the whole of the effect is in that rate. So the pace
//! here is [`PACE`] and the frame rate is high enough to carry it, and both
//! are what the settings turn down for a machine that would rather have the
//! battery.
//!
//! What the shared host does with the result — the magnification, the frame
//! pacing, the input that takes it away — is none of this program's business,
//! and is none of any other screensaver's either. This crate is the picture.

use alpymist_screensaver::paint::{Painting, interval};
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
    /// Frames a second. The host clamps what it is given.
    pub fps: i64,
    /// How much mist there is, as a percentage of what the scene composes.
    pub mist: i32,
    /// How fast everything travels, as a percentage of [`PACE`].
    pub speed: i32,
    /// Whether an aircraft crosses the sky at all.
    pub aircraft: bool,
    /// Whether a balloon drifts through now and then.
    pub balloon: bool,
}

impl Default for Look {
    /// What the definition file says, repeated here so the picture still draws
    /// when it is run with no definition beside it at all.
    fn default() -> Self {
        Self {
            block: 6,
            fps: 12,
            mist: 100,
            speed: 100,
            aircraft: true,
            balloon: true,
        }
    }
}

/// How far the *nearest* range travels in a minute, in columns of the drawn
/// picture, when the speed setting is left at a hundred per cent.
///
/// Nine columns a second, which at the default block is fifty-four physical
/// pixels a second: the tree line crosses a laptop panel in about twenty-five
/// seconds. This is the front of the picture and the fastest thing in it;
/// everything behind is [`DEPTH`] slower again, step by step.
pub const PACE: i32 = 540;

/// How much faster each range travels than the one behind it, in hundredths.
///
/// Distance is what this number is. Apparent speed falls with distance, so
/// ranges spaced evenly in speed — which is what this picture had, each range
/// a fifth of the pace more than the last — are ranges spaced evenly in
/// *nothing*: the back three came past within a quarter of each other's speed
/// and read as one flat card a long way off. Each step back being one and
/// three quarters times slower puts them at nine to one front to back rather
/// than five to four, which is the depth of a picture rather than the depth of
/// a diagram.
///
/// It stops where the composed scene stops. Five ranges at this ratio put the
/// furthest at about a column a second, still travelling — at twice this
/// ratio it would be four physical pixels a second, and the horizon would be
/// back to the standing-still the pace was raised to cure.
const DEPTH: i32 = 175;

/// The aircraft, nose to the right: one bit per cell, top row first.
///
/// Seven cells by three, which at one drawn pixel to a cell is a shape the
/// size of a word of this comment on the screen — and it still reads as an
/// aeroplane, because a fin, a fuselage and a wing is all an aeroplane is from
/// the ground. Flying the other way it is drawn mirrored.
const PLANE: [u8; 3] = [0b110_0000, 0b011_1111, 0b001_1000];

/// How many cells across and down [`PLANE`] is.
const PLANE_SIZE: (i32, i32) = (PLANE_WIDE, 3);

/// How many cells across [`PLANE`] is.
const PLANE_WIDE: i32 = 7;

/// How long the aircraft takes to swing from one end of its crossing to the
/// other and back, in milliseconds.
///
/// Prime-ish against the others and against the ranges' own laps, so the sky
/// does not fall into a pattern with the ground. Near the ends of this it is
/// off the side of the picture, which is what gives the sky stretches with
/// nothing in it: an aeroplane that is always there is scenery, and one that
/// arrives is something to notice.
const CROSS: u64 = 79_000;

/// How long it takes to come in from the distance and go back out, and to rise
/// and fall, in milliseconds.
const APPROACH: u64 = 137_000;
const BOB: u64 = 19_000;

/// The navigation lights an aircraft carries: red to port, green to starboard,
/// steady, and one to each wingtip.
///
/// Not from the palette, and deliberately: these are not Alpymist's colours to
/// choose. An aircraft's lights are red and green because the rules of the air
/// say which side is which, and a themed aeroplane would simply be wrong.
/// Muted from the signal colours, because everything in this picture is.
const PORT: Rgb = Rgb::new(0xD8, 0x4C, 0x42);
const STARBOARD: Rgb = Rgb::new(0x56, 0xC0, 0x6A);

/// The white strobe on the tail: a flash, not a light that is on.
const STROBE_EVERY: u64 = 1_800;
const STROBE_FOR: u64 = 150;

/// And the red anti-collision beacon on the belly, which blinks slower.
///
/// A different period from the strobe on purpose: two lights blinking in step
/// read as a decoration, and two that drift past each other read as an
/// aircraft, because that is what an aircraft's do.
const BEACON_EVERY: u64 = 1_300;
const BEACON_FOR: u64 = 320;

/// The balloon from the Commodore 64 manual, as it is printed there.
///
/// Twenty-four cells by twenty-one, which is what a C64 sprite was, and these
/// are the manual's own numbers — the `DATA 0,127,0 : DATA 1,255,192 …` of the
/// chapter on sprites, three bytes to a row, which for a lot of people was the
/// first picture they ever made a computer draw. One colour draws all of it:
/// the Commodore logo in the middle of the envelope is where the bits are
/// *off*, so it is the sky showing through, exactly as it was on a 1982
/// television.
///
/// Here because the screensaver's whole look is a machine of about that
/// vintage, and because a balloon drifting past mountains is what the sprite
/// was always doing on the front of that manual.
const BALLOON: [u32; 21] = [
    0x00_7F00, 0x01_FFC0, 0x03_FFE0, 0x03_E7E0, 0x07_D9F0, 0x07_DFF0, 0x07_D9F0, 0x03_E7E0,
    0x03_FFE0, 0x03_FFE0, 0x02_FFA0, 0x01_7F40, 0x01_3E40, 0x00_9C80, 0x00_9C80, 0x00_4900,
    0x00_4900, 0x00_3E00, 0x00_3E00, 0x00_3E00, 0x00_1C00,
];

/// How many cells across and down [`BALLOON`] is: a C64 sprite's own shape.
const BALLOON_SIZE: (i32, i32) = (24, 21);

/// How often a balloon comes through.
///
/// It is on the picture for somewhere between a quarter and two thirds of
/// this, depending how near that crossing is, and the sky is empty the rest
/// of the time: rare enough to be a thing you catch rather than scenery.
const BALLOON_EVERY: u64 = 210_000;

/// How long after the screensaver appears the first balloon does.
///
/// Much sooner than the gap between them afterwards, and deliberately: this
/// is the one thing in the picture worth waiting for, and at a full gap the
/// machine would have to be left alone for three and a half minutes before it
/// was ever shown one. The first arrives while somebody might still be
/// watching it go.
const BALLOON_FIRST: u64 = 12_000;

/// How often the burner lights, and for how long.
///
/// A hot-air balloon at night is a dark shape that comes alight: the envelope
/// glows from the inside for as long as the burner is on, which is a second
/// here and there. It is the only warm colour anywhere in this picture, and
/// the only thing in it that is not blue, which is most of why it is worth
/// having at all.
const BURN_EVERY: u64 = 11_000;
const BURN_FOR: u64 = 700;

/// How long the climb after a burn lasts, before the long sink back.
const CLIMB_FOR: u64 = 2_600;

/// What the burner throws.
const FLAME: Rgb = Rgb::new(0xF2, 0xA8, 0x4B);

/// Where the balloon is, when one is up.
struct Balloon {
    /// The column its leftmost cell is drawn at.
    x: i32,
    /// The row its top cell is drawn at.
    y: i32,
    /// Drawn pixels to one sprite cell.
    scale: i32,
    /// How many ranges it is in front of.
    depth: usize,
    /// Whether the burner is lit this instant.
    burning: bool,
}

/// How far the balloon has risen above its own height, in drawn pixels.
///
/// A balloon does not fly at a height, it trades for one: the burner goes,
/// the envelope takes a moment to feel it, it climbs for a few seconds, and
/// then it sinks slowly the rest of the way to the next burn. That is why the
/// flare and the climb belong to each other rather than being two animations
/// sharing a picture, and it is most of what makes a shape in the distance
/// read as a balloon being flown rather than a lamp hung in the sky.
fn lift(elapsed: u64, swing: i32) -> i32 {
    let phase = elapsed % BURN_EVERY;
    // The burn itself, and then the climb it buys.
    let climbed = BURN_FOR + CLIMB_FOR;
    let ms = |t: u64| i32::try_from(t).unwrap_or(0);
    if phase < BURN_FOR {
        // Heating, and nothing has happened yet — which is the part that
        // makes it cause and effect rather than a blinking light.
        0
    } else if phase < climbed {
        ms(phase - BURN_FOR) * swing / ms(CLIMB_FOR).max(1)
    } else {
        // And down again, gently, all the way to the next one.
        swing - ms(phase - climbed) * swing / ms(BURN_EVERY - climbed).max(1)
    }
}

/// Where the aircraft is, at one instant.
struct Flight {
    /// The column its leftmost cell is drawn at. Off the picture is allowed;
    /// painting clips.
    x: i32,
    /// The row its top cell is drawn at.
    y: i32,
    /// Drawn pixels to one cell of [`PLANE`]: one when it is far off, three
    /// when it is closest.
    scale: i32,
    /// Whether it is heading left, in which case it is drawn mirrored.
    left: bool,
    /// How many ranges are behind it: nought puts it behind the lot, on the
    /// horizon, and the number of ranges puts it in front of them all.
    depth: usize,
    /// How near it is, nought to a hundred. Its size, its haze and how far it
    /// swings all come from this one number, which is what makes those three
    /// read as one aeroplane at one distance rather than three coincidences.
    near: i32,
}

/// One band of mist.
///
/// The backdrop composes its mist as a flat translucent band right across the
/// picture. At full resolution, with its alpha ramped away at both edges, that
/// reads as mist; at a sixth of the resolution it is three rows tall and reads
/// as a scan line. So the bands are taken out of the scene here and painted
/// patchy across their width and drifting sideways instead.
#[derive(Clone, Copy)]
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
    /// terrain as well as in the arithmetic. At [`PACE`] the nearest range is
    /// most of a minute reaching the fold and twice that coming back round to
    /// where it began, and the four behind it take their own longer laps, so
    /// what comes past is a reflection rather than a repeat — but it is a
    /// reflection, and a picture that wanted a genuinely endless ridge would
    /// have to compose more terrain rather than fold this.
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
    /// Milliseconds between frames, which the settings ask for.
    interval: u64,
    /// Whether an aircraft crosses the sky.
    aircraft: bool,
    /// Whether a balloon drifts through.
    balloon: bool,
    /// The row of the highest peak in the whole scene, which is as low as the
    /// aircraft is allowed to fly.
    horizon: i32,
    /// Columns the stars cross in a minute: one [`DEPTH`] step further off
    /// than the furthest range, because the sky is behind the mountains.
    sky_speed: i32,
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
    /// The row at which the range in front takes over, per range per column:
    /// how far down each range is actually visible.
    cover: Vec<i32>,
}

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

        // The composed scene puts the furthest range first, so this walks it
        // backwards: the nearest travels at the speed the settings ask for and
        // each one behind it at [`DEPTH`] less again. Never quite nothing,
        // though — a range rounded down to a standstill is a band of pixels
        // holding one colour for as long as the machine is left alone, which
        // is the thing this screensaver is for.
        let fastest = PACE * settings.speed.clamp(0, 1000) / 100;
        let mut speed = fastest;
        for range in ranges.iter_mut().rev() {
            range.speed = speed.max(i32::from(fastest > 0));
            speed = speed * 100 / DEPTH;
        }
        // What is behind each band of mist, in the order the bands come in.
        let behind: Vec<i32> = ranges.iter().map(|r| r.speed).collect();

        // The highest peak anywhere in the scene, which is where the sky the
        // aircraft has to itself ends. Worked out from the terrain rather than
        // picked: a scene composed for a different shape of screen puts its
        // ridges somewhere else, and an aeroplane flying at a fixed fraction
        // of the height spends that scene behind a mountain.
        let horizon = ranges
            .iter()
            .filter_map(|r| r.tops.iter().copied().min())
            .min()
            .unwrap_or_else(|| i32::try_from(small.height / 4).unwrap_or(1));
        // The sky is one step further off again than the furthest range.
        let furthest = ranges.first().map_or(0, |r| r.speed);
        let sky_speed = (furthest * 100 / DEPTH).max(i32::from(furthest > 0));

        let motions = motions(taken.len());
        let bands = taken
            .into_iter()
            .zip(motions)
            .enumerate()
            .map(|(at, ((y, height, colour, alpha), motion))| {
                let i = i32::try_from(at).unwrap_or(0);
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
                    // A band pools in front of the range it was composed with
                    // and behind the next one up, so it travels between their
                    // two speeds — a third again as fast as the one behind,
                    // which is halfway between them now that the step is a
                    // ratio and not an addition. Mist that kept pace with the
                    // ground would read as painted on it; this reads as
                    // weather over it.
                    speed: (behind.get(at).copied().unwrap_or(fastest) * 4 / 3)
                        .max(i32::from(fastest > 0)),
                    seed: 40 + i * 113,
                }
            })
            .collect();

        let len = small.width as usize * small.height as usize;
        Self {
            small,
            block,
            interval: interval(settings.fps),
            aircraft: settings.aircraft,
            balloon: settings.balloon,
            horizon,
            sky_speed,
            sky,
            stars: stars(small),
            ranges,
            pixels: vec![0; len],
            bands,
            across: Vec::new(),
            skyline: Vec::new(),
            shades: Vec::new(),
            cover: Vec::new(),
        }
    }

    /// Draw the frame at `elapsed` milliseconds.
    fn paint(&mut self, elapsed: u64) {
        self.paint_sky();
        self.paint_stars(elapsed);
        self.prepare_ranges(elapsed);
        // Back to front, with whatever is in the sky put down at the distance
        // it is at. Painting the aeroplane before all the ranges — which is
        // what this did at first — makes it an aeroplane on the horizon for
        // ever, however near it comes: it would grow, and stay behind a ridge
        // it was supposed to be in front of, and the growing read as a
        // mistake rather than as an approach. What tells you a thing is close
        // is what it passes in front of.
        let count = self.ranges.len();
        let plane = self.flight(elapsed);
        let balloon = self.crossing(elapsed);
        for r in 0..=count {
            if balloon.as_ref().is_some_and(|it| it.depth == r) {
                self.paint_balloon(elapsed);
            }
            if self.aircraft && plane.depth == r {
                self.paint_aircraft(elapsed);
            }
            if r < count {
                self.fill_range(r);
                self.paint_band(elapsed, r);
            }
        }
    }

    /// Where the aircraft is at `elapsed`.
    ///
    /// Three sines of unrelated periods, and the near one governs the other
    /// two: further off it hangs about the middle of the picture, barely
    /// moving and barely rising, because that is what distance does to a
    /// thing's apparent motion — near, the same angular sweep carries it right
    /// across and off both sides. That coupling is the whole of the
    /// perspective. It is also why nothing here is a position that has to be
    /// remembered between frames: a flight is a function of the clock, so a
    /// picture composed at any instant is already in the right place.
    fn flight(&self, elapsed: u64) -> Flight {
        let w = i32::try_from(self.small.width).unwrap_or(1);
        let turn = |period: u64, phase: i64| {
            let period = i64::try_from(period.max(1)).unwrap_or(1);
            i64::try_from(elapsed).unwrap_or(0) % period * 360 / period + phase
        };

        let near = 50 + sine(turn(APPROACH, 0)) * 50 / UNIT;
        let scale = (1 + near * 3 / 100).clamp(1, 3);
        let across = turn(CROSS, 0);
        // How far it swings: half the picture when it is far off, half again
        // beyond both edges when it is near.
        let span = w / 2 + w * near / 100;
        let x = w / 2 + sine(across) * span / UNIT - PLANE_WIDE * scale / 2;
        // How low it flies is how near it is, because that is what near
        // looks like from the ground: a thing on the horizon is on the
        // horizon, and a thing passing close by is across the middle of the
        // view, in front of the hills behind it.
        let climb = self.band(near) * (6 + near / 5) / 100;
        let y = self.band(near) - PLANE_SIZE.1 * scale + sine(turn(BOB, 40)) * climb / UNIT;
        Flight {
            x,
            y,
            scale,
            // Where it is going, which is a quarter turn ahead of where it is.
            left: sine(across + 90) < 0,
            depth: self.depth(near),
            near,
        }
    }

    /// The row a thing at this distance sits on: the highest peak when it is
    /// far off, most of the way down the picture when it is close.
    ///
    /// One line, and it is what ties a flier's size, its speed, its height in
    /// the frame and what it passes in front of to the same number. Getting
    /// any one of them from somewhere else is what makes a scene look assembled.
    fn band(&self, near: i32) -> i32 {
        let h = i32::try_from(self.small.height).unwrap_or(1);
        let far = (self.horizon + 4).clamp(4, h.max(4));
        let close = h * 62 / 100;
        far + (close - far) * near.clamp(0, 100) / 100
    }

    /// How many ranges a thing at this distance is in front of.
    fn depth(&self, near: i32) -> usize {
        let count = self.ranges.len();
        let over = usize::try_from(near.clamp(0, 100)).unwrap_or(0) * (count + 1) / 101;
        over.min(count)
    }

    /// Scratch, for the flight example: the flight as plain numbers.
    #[allow(dead_code)]
    pub fn flight_probe(&self, elapsed: u64) -> (i32, i32, i32, bool, i32) {
        let f = self.flight(elapsed);
        (f.x, f.y, f.scale, f.left, f.near)
    }

    /// The aircraft, if the settings have one at all.
    fn paint_aircraft(&mut self, elapsed: u64) {
        if !self.aircraft {
            return;
        }
        let flight = self.flight(elapsed);
        let palette = Palette::alpymist();
        // Further off is fainter, which is the haze the ranges are given too.
        let body = u8::try_from((110 + flight.near).clamp(0, 255)).unwrap_or(150);
        // Lights carry further than the thing carrying them, so they do not
        // fade with it: a distant aircraft at night is its lights and nothing
        // else, and at one drawn pixel each that is exactly what this becomes.
        let lamp = u8::try_from((190 + flight.near / 2).clamp(0, 255)).unwrap_or(220);
        let strobe = elapsed % STROBE_EVERY < STROBE_FOR;
        let beacon = elapsed % BEACON_EVERY < BEACON_FOR;

        self.stamp(flight.x, flight.y, flight.scale, PLANE_SIZE, |row, col| {
            // Mirrored when it is heading the other way, and read in the
            // silhouette's own coordinates — so that turning round swaps which
            // wingtip the eye sees red on, the way it would if the aircraft
            // had really turned rather than been drawn backwards.
            let bit = if flight.left {
                col
            } else {
                PLANE_WIDE - 1 - col
            };
            let row_bits = usize::try_from(row).ok().and_then(|r| PLANE.get(r))?;
            if row_bits >> bit & 1 == 0 {
                return None;
            }
            Some(match (row, bit) {
                // The wingtips: port and starboard, always on.
                (2, 4) => (PORT, lamp),
                (2, 3) => (STARBOARD, lamp),
                // The tail's white strobe, and the belly beacon.
                (0, 6) if strobe => (palette.ink, 255),
                (1, 4) if beacon => (PORT, 255),
                _ => (palette.ink_dim, body),
            })
        });
    }

    /// Where a balloon is, if one is crossing at all.
    ///
    /// It goes one way at one speed and does not manoeuvre, because that is
    /// what a balloon does: it is in the air, not flying. Each crossing has
    /// its own distance — one comes past on the ridge line, the next much
    /// closer and in front of half the scene — because a balloon that arrived
    /// at the same distance every time would be a flight path rather than
    /// weather. The aeroplane is up there on its own business; every so often
    /// the two of them cross, and nothing arranges that either.
    fn crossing(&self, elapsed: u64) -> Option<Balloon> {
        if !self.balloon {
            return None;
        }
        let width = i32::try_from(self.small.width).unwrap_or(1);
        let (wide, tall) = BALLOON_SIZE;
        // Wound forward, so the first crossing is soon after the screensaver
        // appears rather than a whole gap into it.
        let since = elapsed.saturating_add(BALLOON_EVERY - BALLOON_FIRST);
        let pass = since / BALLOON_EVERY;
        let phase = since % BALLOON_EVERY;
        // The first crossing after the screensaver appears is the middle
        // one: far enough to be a balloon over the mountains, near enough to
        // be worth looking at. The far one and the near one follow.
        let near = match pass % 3 {
            1 => 58,
            2 => 16,
            _ => 94,
        };
        // Nearer is quicker across, which is the perspective the aeroplane
        // gets as well: the same drift covers more of the view up close.
        let crossing = (BALLOON_EVERY * u64::try_from(140 - near).unwrap_or(100) / 200).max(1);
        if phase >= crossing {
            return None;
        }
        // One drawn pixel to a sprite cell on a picture the size a laptop
        // panel reduces to, which is about the share of the screen a sprite
        // took up on a C64. A picture with far more rows than that is a
        // bigger screen rather than a nearer balloon; a near crossing is a
        // step bigger again, the way a C64 sprite could be expanded.
        let scale = i32::try_from(self.small.height / 128)
            .unwrap_or(1)
            .clamp(1, 3)
            + i32::from(near >= 70);
        let travel = width + wide * scale;
        let gone =
            i32::try_from(phase.saturating_mul(u64::try_from(travel).unwrap_or(1)) / crossing)
                .unwrap_or(0);
        Some(Balloon {
            x: gone - wide * scale,
            y: self.band(near) - tall * scale + lift(elapsed, 3 * scale),
            scale,
            depth: self.depth(near),
            burning: elapsed % BURN_EVERY < BURN_FOR,
        })
    }

    /// The balloon, where the sky says it is.
    fn paint_balloon(&mut self, elapsed: u64) {
        let Some(balloon) = self.crossing(elapsed) else {
            return;
        };
        let (wide, _) = BALLOON_SIZE;
        let palette = Palette::alpymist();
        let burning = balloon.burning;
        self.stamp(
            balloon.x,
            balloon.y,
            balloon.scale,
            BALLOON_SIZE,
            |row, col| {
                let bits = usize::try_from(row).ok().and_then(|r| BALLOON.get(r))?;
                if bits >> (wide - 1 - col) & 1 == 0 {
                    return None;
                }
                Some(match (burning, row) {
                    // Lit: the envelope glowing from the inside, and the
                    // burner at the throat brighter still.
                    (true, 0..=12) => (FLAME, 150),
                    (true, _) => (FLAME, 225),
                    // Dark: it is night and this is a long way off, so it is
                    // bright enough to see and no brighter. The rigging and
                    // the basket carry a little more, being solid things.
                    (false, 0..=12) => (palette.ink_dim, 130),
                    (false, _) => (palette.ink_dim, 160),
                })
            },
        );
    }

    /// Put a small bitmap into the picture, one cell to a `scale` square.
    ///
    /// `cell` answers for each row and column of the sprite: the colour and
    /// the opacity to lay there, or nothing where the sprite is empty. Clipped
    /// at all four edges, so a sprite may be half off the picture or entirely
    /// off it without the caller checking.
    fn stamp(
        &mut self,
        x: i32,
        y: i32,
        scale: i32,
        size: (i32, i32),
        cell: impl Fn(i32, i32) -> Option<(Rgb, u8)>,
    ) {
        let width = i32::try_from(self.small.width).unwrap_or(1);
        let height = i32::try_from(self.small.height).unwrap_or(1);
        let scale = scale.max(1);
        let (wide, tall) = size;
        for row in 0..tall {
            for col in 0..wide {
                let Some((ink, alpha)) = cell(row, col) else {
                    continue;
                };
                for down in 0..scale {
                    for right in 0..scale {
                        let px = x + col * scale + right;
                        let py = y + row * scale + down;
                        if px < 0 || py < 0 || px >= width || py >= height {
                            continue;
                        }
                        let at = usize::try_from(py * width + px).unwrap_or(0);
                        if let Some(under) = self.pixels.get_mut(at) {
                            *under = over(*under, ink, alpha);
                        }
                    }
                }
            }
        }
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
        // Slowest of everything, and not still: the top of the sky is the one
        // part of the picture the ranges never reach, so if the stars did not
        // move nothing up there ever would.
        let slide = drift(elapsed, self.sky_speed);
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
    fn prepare_ranges(&mut self, elapsed: u64) {
        let width = self.small.width as usize;
        let height = i32::try_from(self.small.height).unwrap_or(0);
        let count = self.ranges.len();
        self.skyline.clear();
        self.skyline.resize(count * width, i32::MAX);
        self.shades.clear();
        self.shades.resize(count * width, 0xFF00_0000);
        self.cover.clear();
        self.cover.resize(count * width, height);
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
        // How far down each range can be seen before a nearer one takes over.
        // Worked out once, from the front backwards, so that painting the
        // ranges back to front — which is what lets an aeroplane be put down
        // among them — still writes every pixel exactly once, as the old
        // nearest-wins pass over the rows did.
        for x in 0..width {
            let mut nearer = height;
            for r in (0..count).rev() {
                self.cover[r * width + x] = nearer;
                nearer = nearer.min(self.skyline[r * width + x]);
            }
        }
    }

    /// Paint range `r`, where it can be seen: from its own skyline down to
    /// wherever the range in front of it takes over.
    fn fill_range(&mut self, r: usize) {
        let width = self.small.width as usize;
        let height = i32::try_from(self.small.height).unwrap_or(0);
        let base = r * width;
        for x in 0..width {
            let Some(&top) = self.skyline.get(base + x) else {
                continue;
            };
            let bottom = self.cover.get(base + x).copied().unwrap_or(height);
            let colour = self.shades.get(base + x).copied().unwrap_or(0xFF00_0000);
            let mut y = top.max(0);
            while y < bottom.min(height) {
                if let Some(px) = self
                    .pixels
                    .get_mut(usize::try_from(y).unwrap_or(0) * width + x)
                {
                    *px = colour;
                }
                y += 1;
            }
        }
    }

    /// One band of mist, drifting across whatever is behind it.
    ///
    /// Behind it, and no further: a band pools in front of the range it was
    /// composed with, so it is painted between that range and the next one
    /// up. Painting all of them over the finished picture — which is what
    /// this did — laid the furthest valley's mist over the nearest tree line,
    /// and at these opacities that passed for haze until something had to
    /// fly through it.
    fn paint_band(&mut self, elapsed: u64, j: usize) {
        let width = self.small.width as usize;
        let rows = self.small.height;

        // Copied out, not borrowed: what follows writes into `self.pixels`.
        let Some(band) = self.bands.get(j).copied() else {
            return;
        };
        {
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
                    let ink = down * across / 100;
                    let Ok(ink) = u8::try_from(ink.clamp(0, 255)) else {
                        continue;
                    };
                    if ink == 0 {
                        continue;
                    }
                    *px = over(*px, band.colour, ink);
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

    /// More than the host's default, and for the reason the host names: this
    /// picture travels in a straight line, and at that the gap between frames
    /// is not slowness, it is the skyline jumping. Every frame is a wakeup and
    /// a wakeup costs battery, which is why it is a setting and why the low
    /// end of that setting is the host's own default.
    fn interval_ms(&self) -> u64 {
        self.interval
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

    /// What was wrong with the version before this one. It travelled — the
    /// test above passed — at fourteen columns a minute, which is a picture
    /// that stands still to anybody looking at it. The rate is the feature, so
    /// the rate is what is tested: a second of a screensaver scrolling is
    /// something you can see happening.
    #[test]
    fn a_second_of_it_is_a_visible_amount_of_travel() {
        let mut m = Mountains::compose(Size::new(1366, 768), &Look::default());
        let first = m.frame(0).to_vec();
        let second = m.frame(1_000).to_vec();
        let moved = first.iter().zip(&second).filter(|(a, b)| a != b).count();
        let share = moved * 100 / first.len().max(1);
        assert!(
            share >= 10,
            "{share}% of the picture changed in a second: that is a photograph"
        );
    }

    /// And that a frame comes often enough to carry it: at the pace above, a
    /// picture drawn at the host's default rate steps a column and a half at a
    /// time, which is the skyline juddering rather than travelling.
    #[test]
    fn it_asks_for_more_frames_than_a_still_picture_would() {
        let m = Mountains::compose(Size::new(640, 480), &Look::default());
        assert_eq!(m.interval_ms(), 1000 / 12);
        let slow = Look {
            fps: 8,
            ..Look::default()
        };
        assert_eq!(
            Mountains::compose(Size::new(640, 480), &slow).interval_ms(),
            125
        );
        let greedy = Look {
            fps: 500,
            ..Look::default()
        };
        assert!(Mountains::compose(Size::new(640, 480), &greedy).interval_ms() >= 1000 / 30);
    }

    /// The complaint the depth answers: five ranges spaced a fifth of the pace
    /// apart are five ranges at *no* particular distance from each other, and
    /// the back three of them travel within a quarter of one another's speed,
    /// which the eye reads as one card a long way off. Spacing by ratio is
    /// what makes the picture deep.
    #[test]
    fn each_range_is_a_step_further_off_than_the_one_in_front() {
        let m = scene(1366, 768);
        let speeds: Vec<i32> = m.ranges.iter().map(|r| r.speed).collect();
        assert!(speeds.len() >= 3, "a scene with no depth to test");
        for pair in speeds.windows(2) {
            let (behind, front) = (pair[0], pair[1]);
            assert!(
                front * 100 >= behind * 160,
                "{front} in front of {behind} is not a step, it is a rounding error"
            );
        }
        let furthest = speeds[0];
        let nearest = *speeds.last().unwrap_or(&0);
        assert!(furthest > 0, "the horizon stands still");
        assert!(
            nearest >= furthest * 5,
            "{nearest} to {furthest} front to back is a diagram, not a distance"
        );
    }

    /// And the sky is one step further off again than the furthest of them.
    ///
    /// It stopped being that when the depth widened: the stars were a fixed
    /// share of the *nearest* range, so making the front faster eventually had
    /// the sky overtaking the horizon it is supposed to be behind.
    #[test]
    fn the_stars_are_further_off_than_the_mountains() {
        let m = scene(1366, 768);
        let furthest = m.ranges.first().map_or(0, |r| r.speed);
        assert!(m.sky_speed > 0, "a sky that never moves at all");
        assert!(
            m.sky_speed < furthest,
            "the stars at {} overtake the horizon at {furthest}",
            m.sky_speed
        );
    }

    /// The two things in the sky are settings, and both ends of both draw.
    #[test]
    fn the_sky_traffic_can_be_turned_off_and_shows_when_it_is_not() {
        let empty = Look {
            aircraft: false,
            balloon: false,
            ..Look::default()
        };
        let mut bare = Mountains::compose(Size::new(1366, 768), &empty);
        let mut busy = Mountains::compose(Size::new(1366, 768), &Look::default());
        // Two hundred seconds in, both an aeroplane and a balloon are up.
        let at = 200_000;
        assert_ne!(
            bare.frame(at).to_vec(),
            busy.frame(at).to_vec(),
            "turning them on changed nothing, so nothing is being drawn"
        );
        assert!(bare.frame(at).iter().all(|px| px >> 24 == 0xFF));
    }

    /// The complaint this answers: whatever it did, the aeroplane was always
    /// behind the furthest ridge, so it could grow all it liked and still read
    /// as a thing on the horizon. What tells you something is close is what it
    /// passes in front of.
    #[test]
    fn what_is_in_the_sky_comes_past_at_different_distances() {
        let m = scene(1366, 768);
        let count = m.ranges.len();
        let depths: Vec<usize> = (0..140).map(|s| m.flight(s * 1000).depth).collect();
        assert!(
            depths.contains(&0),
            "it is never out on the horizon behind the lot of them"
        );
        assert!(
            depths.iter().any(|&d| d >= count),
            "it is never in front of the mountains, so it never comes close"
        );
        // And it gets there gradually rather than jumping: no step in its
        // distance is more than one range.
        for pair in depths.windows(2) {
            let step = pair[0].abs_diff(pair[1]);
            assert!(
                step <= 1,
                "it moved {step} ranges between one second and the next"
            );
        }
    }

    /// A balloon every three and a half minutes is one nobody ever sees if the
    /// first is a full gap away, which is exactly how it was found that nobody
    /// had seen one.
    #[test]
    fn the_first_balloon_arrives_while_somebody_might_still_be_watching() {
        let m = scene(1366, 768);
        let width = i32::try_from(m.small().width).unwrap_or(1);
        let (wide, _) = super::BALLOON_SIZE;
        let seen = (0..45).any(|s| {
            m.crossing(s * 1000)
                .is_some_and(|b| b.x + wide * b.scale > 0 && b.x < width)
        });
        assert!(seen, "no balloon in the first three quarters of a minute");
        // And they are not all at the same distance, or it is a flight path.
        let distances: Vec<usize> = (0..12)
            .filter_map(|n| m.crossing(n * super::BALLOON_EVERY + 20_000))
            .map(|b| b.depth)
            .collect();
        assert!(
            distances.windows(2).any(|p| p[0] != p[1]),
            "every balloon comes past at the same distance"
        );
    }

    /// The burner and the climb are one thing: it goes up *after* a burn and
    /// sinks between them, which is what flying a balloon is.
    #[test]
    fn the_balloon_rises_on_the_burner_and_sinks_between_burns() {
        let swing = 6;
        assert_eq!(super::lift(0, swing), 0, "it moves before the burner does");
        let during = super::lift(super::BURN_FOR / 2, swing);
        assert_eq!(
            during, 0,
            "it is already climbing while the burner heats it"
        );
        let climbing = super::lift(super::BURN_FOR + super::CLIMB_FOR / 2, swing);
        let topped = super::lift(super::BURN_FOR + super::CLIMB_FOR, swing);
        assert!(
            0 < climbing && climbing < topped,
            "the climb after the burn is not a climb: {climbing} then {topped}"
        );
        assert_eq!(topped, swing, "it does not get the height the burn bought");
        let sinking = super::lift(super::BURN_EVERY - 1, swing);
        assert!(
            sinking < topped / 2,
            "it is still up at {sinking} when the next burn comes"
        );
    }

    /// A sprite is drawn wherever the flight puts it, including half off the
    /// side and entirely off it, and on a picture smaller than the sprite.
    #[test]
    fn what_flies_off_the_edge_is_clipped_and_not_a_panic() {
        for size in [Size::new(120, 90), Size::new(3840, 2160)] {
            let mut m = Mountains::compose(size, &Look::default());
            for ms in (0..420_000).step_by(3_000) {
                assert!(m.frame(ms).iter().all(|px| px >> 24 == 0xFF));
            }
        }
    }

    /// Turned all the way down it is the picture it always was, and turned up
    /// it does not run away with itself: both ends of the setting draw.
    #[test]
    fn the_speed_setting_reaches_both_ends_without_tearing() {
        for speed in [0, 25, 100, 300] {
            let look = Look {
                block: 4,
                speed,
                ..Look::default()
            };
            let mut m = Mountains::compose(Size::new(800, 600), &look);
            for ms in [0, 7_000, 600_000] {
                assert!(m.frame(ms).iter().all(|px| px >> 24 == 0xFF));
            }
        }
    }

    #[test]
    fn nothing_moving_is_a_setting_and_not_a_panic() {
        let still = Look {
            block: 4,
            mist: 0,
            speed: 0,
            ..Look::default()
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

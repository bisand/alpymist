//! The view from the bridge: stars streaming past, and rocks to get round.
//!
//! A screensaver of things travelling straight at you is the one kind where the
//! frame rate is the picture. The mountains can drift at eight frames a second
//! because nothing in them goes anywhere in particular; a star crossing the
//! screen in a second and a half at eight frames is a dotted line. So this asks
//! the shared host for more frames than the default, and says so in its
//! settings, because the person paying for them in battery is the one who
//! should decide how many there are.
//!
//! The rocks are the other way round. They are drawn from a small grid of
//! square cells blown up whole — a sprite, not a shape — and they are *not*
//! turned to follow the ship's roll, because a chunky grid rotated off the
//! screen's grid stops looking like a sprite and starts looking like a mistake.
//! Smooth movement, coarse rocks: that is the whole brief, and the two are
//! separate knobs rather than one.
//!
//! Everything here is integer arithmetic, like [`alpymist_screensaver::scene`]
//! and for the same reason: the machines Alpymist exists for have no floating
//! point worth the name, and a sine from a table cannot drift between
//! architectures.

use alpymist_screensaver::paint::{Painting, interval};
use alpymist_screensaver::scene::{UNIT, reduced, sine};
use alpymist_ui::palette::{Palette, Rgb};
use denise::geom::Size;

/// The seed that fixes the flight.
///
/// Fixed rather than from the clock, so the same run of the same build draws
/// the same thing — which is what makes a snapshot worth looking at and a test
/// worth writing. It is arbitrary, not unpredictable: what it decides is where
/// a rock is, and nobody is guessing.
pub const FLIGHT_SEED: u64 = 0x05EE_D0F5_7A25;

/// How far away things are born, in world units.
const FAR: i32 = 4_096;

/// How near something comes before it is behind you and gone.
const NEAR: i32 = 96;

/// How far off the flight path things are scattered, in world units.
const SPREAD: i32 = 2_600;

/// World units travelled in a second at the speed the settings call 100%.
const CRUISE: i32 = 900;

/// The fastest the ship slides sideways, in world units a second.
const SWERVE: i32 = 760;

/// How quickly it reaches that, in world units a second, per second.
const PUSH: i32 = 620;

/// How far the ship wanders off a straight line when nothing is in the way, as
/// a share of [`SWERVE`] in percent.
///
/// Well under half. The point of the wander is that the vanishing point is
/// never quite still — that the stars stream past at an angle that keeps
/// changing — not that the ship is being flown badly.
const WANDER: i32 = 34;

/// How far the ship banks at full lateral speed, in tenths of a degree.
const BANK: i32 = 170;

/// How long the bank takes to follow a change of direction, in milliseconds.
const BANK_MS: i32 = 900;

/// Milliseconds between asteroid fields, and how much later than that one may
/// be.
///
/// Long enough that a field is an event rather than the weather, short enough
/// that somebody who glances at the screen twice in a few minutes sees one.
const FIELD_EVERY: u64 = 40_000;
/// How much later than [`FIELD_EVERY`] a field may be.
const FIELD_VARY: u64 = 35_000;

/// How wide the corridor kept clear through a field is, in world units.
///
/// The guarantee that the ship gets through. No rock is born within this of the
/// gap, so steering onto the gap is steering into a hole that is certainly
/// there — rather than swerving away from whatever looks nearest and hoping.
///
/// Narrow on purpose, and not much wider than the biggest rock. The perspective
/// is unforgiving: anything further off the flight path than about half its own
/// depth is already off the edge of the picture, so a generous corridor is one
/// where every rock leaves the frame while it is still a pebble and the field
/// is something that happened off to the side. At this width they come past
/// close enough to fill a third of the screen, and still miss by half as much
/// again as their own radius.
const GAP: i32 = 760;

/// How hard the ship is shoved away from a rock that is close and ahead.
///
/// The belt to the gap's braces. Aiming for the hole is what makes the flight
/// read as flying; this is what makes a collision impossible even if the field
/// spawned somewhere the ship could not reach in time.
const SHOVE: i32 = 3;

/// The furthest off the centre anything is projected at all, as a multiple of
/// the picture's width.
///
/// Something a hand's breadth from the camera and away to one side projects to
/// a number with no bearing on anything, and the temptation is to clamp it —
/// which quietly moves it back to the edge of the picture, where it is a rock
/// that has no business being anywhere near. So it is dropped instead: past
/// this it is not on the screen under any reading, and saying so is both
/// cheaper and honest.
const REACH: i32 = 4;

/// What the settings make of this picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Physical pixels to one drawn pixel.
    pub block: u32,
    /// Frames a second. The host clamps what it is given.
    pub fps: i64,
    /// How many stars, as a percentage of what the picture's size suggests.
    pub stars: i32,
    /// How fast the ship travels, as a percentage of [`CRUISE`].
    pub speed: i32,
    /// Whether asteroid fields happen at all.
    pub rocks: bool,
    /// How many cells across a rock is drawn: fewer is chunkier.
    pub grain: u32,
}

impl Default for Look {
    /// What the definition file says, repeated here so the picture still draws
    /// when it is run with no definition beside it at all.
    fn default() -> Self {
        Self {
            block: 4,
            fps: 20,
            stars: 100,
            speed: 100,
            rocks: true,
            grain: 12,
        }
    }
}

/// One star, in the ship's own frame: the camera never moves, the sky does.
struct Star {
    x: i32,
    y: i32,
    z: i32,
    /// How bright it is at its brightest.
    ink: u8,
    /// Where it was drawn last frame, if it was.
    ///
    /// The streak is the path it actually took, so it bends when the ship
    /// turns and shortens when it slows — which is what a streak computed from
    /// the speed alone never does.
    was: Option<(i32, i32)>,
}

/// One asteroid: a lump of rock, and the sprite it is drawn as.
struct Rock {
    x: i32,
    y: i32,
    z: i32,
    /// Its radius in world units.
    radius: i32,
    /// `grain` by `grain` cells. `0` is empty; `1` to `3` are lit to dark.
    cells: Vec<u8>,
    /// How many cells across.
    grain: u32,
}

/// The flight, at its reduced size, ready to animate.
pub struct Starfield {
    /// The reduced size everything is drawn at.
    small: Size,
    /// Physical pixels to one drawn pixel.
    block: u32,
    /// Milliseconds between frames.
    interval: u64,
    /// The picture's centre, which is where the ship is pointed.
    cx: i32,
    cy: i32,
    /// How far a world unit at one unit of depth is, in drawn pixels.
    focal: i32,
    /// World units a second the ship travels forward.
    cruise: i32,
    /// How chunky a rock is drawn.
    grain: u32,
    /// Whether asteroid fields happen.
    fields: bool,

    stars: Vec<Star>,
    rocks: Vec<Rock>,
    /// Where the hole through the current field is, if there is a field.
    gap: Option<(i32, i32)>,
    /// When the next field is due, in milliseconds.
    due: u64,

    /// How fast the ship is sliding sideways, in world units a second.
    vx: i32,
    vy: i32,
    /// How far it is banked, in tenths of a degree.
    bank: i32,
    /// The elapsed time of the last frame, so a step can be a step.
    at: u64,

    /// The frame being drawn.
    pixels: Vec<u32>,
    /// The colour of empty space.
    space: u32,
    /// A star's colour at each of a few distances, worked out once.
    sky: Vec<u32>,
    /// A rock's three shades at each of a few distances, worked out once.
    stone: Vec<[u32; 3]>,
    /// What decides where the next thing is.
    seed: u64,
}

/// How many bands of distance the colours are worked out for.
///
/// Sixteen. Fading by distance is the whole of why a rock emerges rather than
/// appearing, and doing it exactly would be a blend per pixel; doing it in
/// sixteen steps is a table lookup, and at these sizes nobody can see the
/// steps.
const BANDS: usize = 16;

impl Starfield {
    /// Compose for an output of this size, as the settings ask for.
    #[must_use]
    pub fn compose(output: Size, settings: &Look) -> Self {
        let (small, block) = reduced(output, settings.block);
        let palette = Palette::alpymist();
        // Space is the colour the host paints behind the first frame, and the
        // colour the lock screen uses. One running into the other shows no
        // seam — and a screen that is nearly all this colour nearly all the
        // time is the gentlest thing there is to leave lit.
        let space = opaque(palette.sky_high);
        let mut sky = Vec::with_capacity(BANDS);
        let mut stone = Vec::with_capacity(BANDS);
        for band in 0..BANDS {
            let away = u32::try_from(band * 84 / BANDS).unwrap_or(0);
            sky.push(opaque(palette.ink.mix(palette.sky_high, away)));
            let shade = |towards: u32| {
                opaque(
                    palette
                        .ink_dim
                        .mix(palette.ridge_near, towards)
                        .mix(palette.sky_high, away * 3 / 4),
                )
            };
            stone.push([shade(8), shade(38), shade(66)]);
        }

        let mut it = Self {
            small,
            block,
            interval: interval(settings.fps),
            cx: i32::try_from(small.width).unwrap_or(1) / 2,
            cy: i32::try_from(small.height).unwrap_or(1) / 2,
            focal: i32::try_from(small.width).unwrap_or(1).max(1),
            cruise: CRUISE * settings.speed.clamp(1, 1000) / 100,
            grain: settings.grain.clamp(4, 32),
            fields: settings.rocks,
            stars: Vec::new(),
            rocks: Vec::new(),
            gap: None,
            due: FIELD_EVERY / 3,
            vx: 0,
            vy: 0,
            bank: 0,
            at: 0,
            pixels: vec![space; small.width as usize * small.height as usize],
            space,
            sky,
            stone,
            seed: FLIGHT_SEED,
        };
        let area = i64::from(small.width) * i64::from(small.height);
        let count = (area / 200 * i64::from(settings.stars.clamp(5, 500)) / 100).clamp(40, 900);
        it.stars = (0..count).map(|_| it.born(true)).collect();
        it
    }

    /// The next arbitrary number.
    ///
    /// A plain 64-bit LCG. It needs to be arbitrary, not unpredictable, and a
    /// dependency for that would be a dependency in a screensaver.
    fn next(&mut self) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.seed >> 33) as u32
    }

    /// An arbitrary number from `-span` to `span`.
    fn spread(&mut self, span: i32) -> i32 {
        let span = span.max(1);
        i32::try_from(self.next() % u32::try_from(span * 2).unwrap_or(2)).unwrap_or(0) - span
    }

    /// A new star. `anywhere` scatters it through the depth rather than putting
    /// it at the far plane, which is what the first frame's worth need.
    ///
    /// Placed on the picture and then pushed out to its depth, rather than
    /// scattered through a box of world units: the box and what the camera can
    /// see are different shapes, and scattering through the box puts most of
    /// the sky where nobody is looking. Slightly wider than the picture, so
    /// stars arrive at the edges as well as out of the middle.
    fn born(&mut self, anywhere: bool) -> Star {
        let z = if anywhere {
            NEAR + i32::try_from(self.next() % u32::try_from(FAR - NEAR).unwrap_or(1)).unwrap_or(0)
        } else {
            FAR * 3 / 4
                + i32::try_from(self.next() % u32::try_from(FAR / 4).unwrap_or(1)).unwrap_or(0)
        };
        let half_w = i32::try_from(self.small.width).unwrap_or(2) / 2;
        let half_h = i32::try_from(self.small.height).unwrap_or(2) / 2;
        let out = |screen: i32, z: i32, focal: i32| {
            i32::try_from(i64::from(screen) * i64::from(z) / i64::from(focal.max(1))).unwrap_or(0)
        };
        let (sx, sy) = (self.spread(half_w * 6 / 5), self.spread(half_h * 6 / 5));
        // Mostly faint, a few bright: an even spread reads as a grid of dots
        // rather than as a sky.
        let ink = u8::try_from(90 + self.next() % 166).unwrap_or(160);
        Star {
            x: out(sx, z, self.focal),
            y: out(sy, z, self.focal),
            z,
            ink,
            was: None,
        }
    }

    /// Scatter an asteroid field ahead, with a hole through it.
    ///
    /// Rocks go in a ring round the hole rather than anywhere in the volume
    /// ahead. The perspective is why: something further off the flight path
    /// than about half its own depth is off the edge of the picture, so rocks
    /// scattered over a wide volume are mostly rocks that sweep past unseen —
    /// and an asteroid field nobody sees is not an event. A ring whose inner
    /// edge is the corridor puts every rock close enough to come through the
    /// picture and no rock close enough to be hit.
    fn scatter(&mut self) {
        let gap = (self.spread(SPREAD / 2), self.spread(SPREAD / 2));
        let count = 6 + self.next() % 9;
        for _ in 0..count {
            let turn = i64::from(self.next() % 360);
            let reach = GAP * 11 / 10
                + i32::try_from(self.next() % u32::try_from(GAP).unwrap_or(1)).unwrap_or(0);
            let x = gap.0 + reach * sine(turn + 90) / UNIT;
            let y = gap.1 + reach * sine(turn) / UNIT;
            let radius = 130 + i32::try_from(self.next() % 300).unwrap_or(0);
            let z = FAR + i32::try_from(self.next() % u32::try_from(FAR).unwrap_or(1)).unwrap_or(0);
            let grain = self.grain;
            let cells = self.carve(grain);
            self.rocks.push(Rock {
                x,
                y,
                z,
                radius,
                cells,
                grain,
            });
        }
        self.gap = Some(gap);
    }

    /// A lumpy disc of cells, lit from the upper left.
    ///
    /// A disc with bites taken out of its rim, rather than a radius worked out
    /// per angle: the bites need no trigonometry, and at a dozen cells across
    /// nobody can tell which method drew the outline — only that it is not a
    /// circle.
    fn carve(&mut self, grain: u32) -> Vec<u8> {
        let side = usize::try_from(grain).unwrap_or(1).max(1);
        let g = i32::try_from(side).unwrap_or(1);
        // Half-cell units, so the middle of a cell is odd and the centre of the
        // rock is zero however many cells there are.
        let at = |c: usize| 2 * i32::try_from(c).unwrap_or(0) + 1 - g;
        let mut cells = vec![0u8; side * side];
        for cy in 0..side {
            for cx in 0..side {
                let (dx, dy) = (at(cx), at(cy));
                if dx * dx + dy * dy > g * g {
                    continue;
                }
                // Lit from the upper left, in three steps.
                let lit = -(dx + dy);
                let shade = if lit > g / 2 {
                    1
                } else if lit > -g / 2 {
                    2
                } else {
                    3
                };
                cells[cy * side + cx] = shade;
            }
        }
        for _ in 0..3 + self.next() % 4 {
            // Centred on the rim. A bite taken out of the middle would be a
            // hole, and a rock with a hole in it is a doughnut.
            let turn = i64::from(self.next() % 360);
            let bx = g * sine(turn + 90) / UNIT;
            let by = g * sine(turn) / UNIT;
            let bite = 2 + i32::try_from(self.next() % 3).unwrap_or(0);
            for cy in 0..side {
                for cx in 0..side {
                    let (dx, dy) = (at(cx) - bx, at(cy) - by);
                    if dx * dx + dy * dy <= bite * bite {
                        cells[cy * side + cx] = 0;
                    }
                }
            }
        }
        // A crater or two: still rock, but in the shade.
        for _ in 0..2 {
            let bx = self.spread(g * 2 / 3);
            let by = self.spread(g * 2 / 3);
            for cy in 0..side {
                for cx in 0..side {
                    let (dx, dy) = (at(cx) - bx, at(cy) - by);
                    let cell = &mut cells[cy * side + cx];
                    if *cell != 0 && dx * dx + dy * dy <= 8 {
                        *cell = 3;
                    }
                }
            }
        }
        cells
    }

    /// Where the ship would like to be sliding, in world units a second.
    ///
    /// Three things, in order of how much they matter: get onto the hole
    /// through the field ahead, get away from anything close and in front, and
    /// otherwise wander, so that the direction of travel is never quite the
    /// same for long.
    fn steer(&self, elapsed: u64) -> (i32, i32) {
        let (mut tx, mut ty) = if let Some((gx, gy)) = self.gap {
            // Proportional: far from the hole, all the speed there is; near it,
            // slowing onto it rather than swinging past.
            (
                (gx * 5 / 2).clamp(-SWERVE, SWERVE),
                (gy * 5 / 2).clamp(-SWERVE, SWERVE),
            )
        } else {
            {
                let turn = |period: i64, phase: i64| {
                    let ms = i64::try_from(elapsed).unwrap_or(i64::MAX);
                    sine(ms % period * 360 / period + phase) * SWERVE * WANDER / (100 * UNIT)
                };
                (turn(37_000, 0), turn(53_000, 120))
            }
        };
        // Anything close and nearly ahead gets shoved away from, hard.
        for rock in &self.rocks {
            let ahead = rock.z - NEAR;
            if ahead <= 0 || ahead > FAR / 4 {
                continue;
            }
            // Tighter than [`GAP`], so a rock passing where the corridor says
            // it should — however big it is — does not set this off and steer
            // the ship out of the hole it was aimed at.
            let clear = (rock.radius * 3 / 2).min(GAP * 3 / 4);
            if rock.x.abs() > clear || rock.y.abs() > clear {
                continue;
            }
            let away = |d: i32| if d >= 0 { -SWERVE } else { SWERVE };
            tx = (tx + away(rock.x) * SHOVE).clamp(-SWERVE, SWERVE);
            ty = (ty + away(rock.y) * SHOVE).clamp(-SWERVE, SWERVE);
        }
        (tx, ty)
    }

    /// Fly for `dt` milliseconds.
    fn advance(&mut self, dt: i32, elapsed: u64) {
        let (tx, ty) = self.steer(elapsed);
        let most = PUSH * dt / 1000;
        let toward = |now: i32, want: i32| now + (want - now).clamp(-most, most);
        self.vx = toward(self.vx, tx);
        self.vy = toward(self.vy, ty);

        // Bank into the turn, and take a moment about it.
        let want = -self.vx * BANK / SWERVE.max(1);
        let ease = (want - self.bank) * dt / BANK_MS.max(1);
        self.bank += if ease == 0 && want != self.bank {
            (want - self.bank).signum()
        } else {
            ease
        };

        let slide_x = self.vx * dt / 1000;
        let slide_y = self.vy * dt / 1000;
        let ahead = self.cruise * dt / 1000;
        let focal = self.focal;
        let edge = i32::try_from(self.small.width.max(self.small.height)).unwrap_or(2);

        for i in 0..self.stars.len() {
            let gone = {
                let star = &mut self.stars[i];
                star.x -= slide_x;
                star.y -= slide_y;
                star.z -= ahead;
                // Off the side as well as past the camera: a star swept out of
                // frame by a turn would otherwise be projected, and thrown
                // away, every frame until it reached the near plane. Written as
                // a multiply rather than the projection's divide, because this
                // runs for every star of every frame.
                let out = i64::from(star.x.abs().max(star.y.abs())) * i64::from(focal);
                star.z < NEAR || out > i64::from(edge) * i64::from(star.z)
            };
            if gone {
                self.stars[i] = self.born(false);
            }
        }
        for rock in &mut self.rocks {
            rock.x -= slide_x;
            rock.y -= slide_y;
            rock.z -= ahead;
        }
        self.rocks.retain(|r| r.z > NEAR / 2);
        if let Some(gap) = self.gap.as_mut() {
            gap.0 -= slide_x;
            gap.1 -= slide_y;
        }
        if self.rocks.is_empty() && self.gap.is_some() {
            self.gap = None;
            self.due = elapsed + FIELD_EVERY + u64::from(self.next()) % FIELD_VARY;
        }
        if self.fields && self.gap.is_none() && elapsed >= self.due {
            self.scatter();
        }
    }

    /// Where a point in front of the ship lands on the picture.
    ///
    /// `None` when it is level with the camera or behind it, where a
    /// perspective divide means nothing.
    fn project(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32)> {
        if z < NEAR {
            return None;
        }
        let far = i64::from(REACH) * i64::from(self.small.width.max(1));
        let at = |v: i32| {
            let offset = i64::from(v) * i64::from(self.focal) / i64::from(z);
            (offset.abs() <= far).then(|| i32::try_from(offset).unwrap_or(0))
        };
        let (ox, oy) = (at(x)?, at(y)?);
        // Banked, so the whole sky swings when the ship turns.
        let (cos, sin) = (
            sine(i64::from(self.bank) / 10 + 90),
            sine(i64::from(self.bank) / 10),
        );
        Some((
            self.cx + (ox * cos - oy * sin) / UNIT,
            self.cy + (ox * sin + oy * cos) / UNIT,
        ))
    }

    /// Which band of distance `z` falls in, for the colour tables.
    fn band(z: i32) -> usize {
        let away = usize::try_from((z - NEAR).clamp(0, FAR)).unwrap_or(0);
        let depth = usize::try_from(FAR).unwrap_or(1) + 1;
        (away * BANDS / depth).min(BANDS - 1)
    }

    /// One pixel, if it is on the picture at all.
    fn plot(&mut self, x: i32, y: i32, colour: u32) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.small.width || y >= self.small.height {
            return;
        }
        let at = y as usize * self.small.width as usize + x as usize;
        if let Some(px) = self.pixels.get_mut(at) {
            *px = colour;
        }
    }

    /// A straight run of pixels from one point to another.
    ///
    /// Bresenham, which is the cheapest correct line there is and needs no
    /// division in its loop. Two points the same is one pixel, which is what
    /// makes this the only way a star is ever drawn: a far one has not moved
    /// far enough to be a streak, and comes out as the dot it should be.
    fn line(&mut self, from: (i32, i32), to: (i32, i32), colour: u32) {
        let (mut x, mut y) = from;
        let (dx, dy) = ((to.0 - x).abs(), -(to.1 - y).abs());
        let (sx, sy) = (if x < to.0 { 1 } else { -1 }, if y < to.1 { 1 } else { -1 });
        let mut err = dx + dy;
        // A streak longer than the picture is a star that went past the camera;
        // drawing it would be a bright line across everything.
        let most = i32::try_from(self.small.width + self.small.height).unwrap_or(i32::MAX);
        for _ in 0..=most {
            self.plot(x, y, colour);
            if (x, y) == to {
                return;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// A filled square of `size` drawn pixels, from its top left.
    fn square(&mut self, x: i32, y: i32, size: i32, colour: u32) {
        for row in 0..size {
            for col in 0..size {
                self.plot(x + col, y + row, colour);
            }
        }
    }

    /// Draw the frame.
    fn paint(&mut self) {
        self.pixels.fill(self.space);
        self.paint_rocks();
        self.paint_stars();
    }

    /// The stars, each as the run of pixels it covered since the last frame.
    fn paint_stars(&mut self) {
        for i in 0..self.stars.len() {
            let (z, ink, was) = {
                let star = &self.stars[i];
                (star.z, star.ink, star.was)
            };
            let (x, y) = (self.stars[i].x, self.stars[i].y);
            let Some(now) = self.project(x, y, z) else {
                self.stars[i].was = None;
                continue;
            };
            // Bright stars stay bright further out; faint ones fade first.
            let faint = (255 - usize::from(ink)) * 6 / 255;
            let colour = self.sky[(Self::band(z) + faint).min(BANDS - 1)];
            match was {
                Some(before) => self.line(before, now, colour),
                None => self.plot(now.0, now.1, colour),
            }
            self.stars[i].was = Some(now);
        }
    }

    /// The rocks, furthest first, each blown up from its own grid of cells.
    fn paint_rocks(&mut self) {
        let mut order: Vec<usize> = (0..self.rocks.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.rocks[i].z));
        for i in order {
            let (x, y, z, radius, grain) = {
                let rock = &self.rocks[i];
                (rock.x, rock.y, rock.z, rock.radius, rock.grain)
            };
            let Some((px, py)) = self.project(x, y, z) else {
                continue;
            };
            let across =
                i32::try_from(i64::from(radius) * 2 * i64::from(self.focal) / i64::from(z))
                    .unwrap_or(i32::MAX);
            let across_cells = usize::try_from(grain).unwrap_or(1).max(1);
            let g = i32::try_from(across_cells).unwrap_or(1);
            // At least one drawn pixel to a cell: below that the cells would
            // land on top of one another and the rock would come and go.
            let cell = (across / g).max(1);
            let side = cell * g;
            // Off the picture entirely, which most of a field is most of the
            // time: one comparison rather than a few hundred plots.
            if px + side / 2 < 0
                || py + side / 2 < 0
                || px - side / 2 >= i32::try_from(self.small.width).unwrap_or(0)
                || py - side / 2 >= i32::try_from(self.small.height).unwrap_or(0)
            {
                continue;
            }
            let shades = self.stone[Self::band(z)];
            let (left, top) = (px - side / 2, py - side / 2);
            for cy in 0..across_cells {
                for cx in 0..across_cells {
                    let shade = self.rocks[i].cells[cy * across_cells + cx];
                    if shade == 0 {
                        continue;
                    }
                    let colour = shades[(usize::from(shade) - 1).min(2)];
                    let step = |c: usize| i32::try_from(c).unwrap_or(0) * cell;
                    self.square(left + step(cx), top + step(cy), cell, colour);
                }
            }
        }
    }
}

impl Painting for Starfield {
    fn small(&self) -> Size {
        self.small
    }

    fn block(&self) -> u32 {
        self.block
    }

    fn interval_ms(&self) -> u64 {
        self.interval
    }

    fn frame(&mut self, elapsed: u64) -> &[u32] {
        // Clamped: a machine that stalled — a display waking, a snapshot
        // stepping minutes at a time — comes back with a clock that jumped,
        // and flying the whole gap in one step would teleport the ship through
        // whatever was in the way.
        let dt = i32::try_from(elapsed.saturating_sub(self.at).min(200)).unwrap_or(0);
        self.at = elapsed;
        self.advance(dt, elapsed);
        self.paint();
        &self.pixels
    }
}

/// A palette colour as an opaque pixel.
fn opaque(c: Rgb) -> u32 {
    u32::from_be_bytes([0xFF, c.r, c.g, c.b])
}

#[cfg(test)]
mod tests {
    use super::{FAR, GAP, Look, NEAR, Starfield};
    use alpymist_screensaver::paint::Painting;
    use denise::geom::Size;

    fn flight(look: &Look) -> Starfield {
        Starfield::compose(Size::new(1366, 768), look)
    }

    /// Frames as the host asks for them: a step at a time, not a jump.
    fn fly(scene: &mut Starfield, seconds: u64) {
        let step = scene.interval_ms().max(1);
        for n in 1..=(seconds * 1000 / step) {
            scene.frame(n * step);
        }
    }

    #[test]
    fn nothing_it_draws_is_transparent() {
        let mut scene = flight(&Look::default());
        for n in 0..400 {
            assert!(
                scene.frame(n * 50).iter().all(|px| px >> 24 == 0xFF),
                "a transparent pixel would show the desktop through"
            );
        }
    }

    #[test]
    fn it_hands_back_exactly_the_pixels_it_says_it_has() {
        let mut scene = flight(&Look::default());
        let small = scene.small();
        assert_eq!(
            scene.frame(0).len(),
            small.width as usize * small.height as usize
        );
    }

    #[test]
    fn a_smooth_screensaver_asks_for_the_frames_it_needs() {
        assert_eq!(
            flight(&Look::default()).interval_ms(),
            50,
            "twenty a second"
        );
        let slow = flight(&Look {
            fps: 10,
            ..Look::default()
        });
        assert_eq!(slow.interval_ms(), 100);
        // Whatever it is asked for, the shared host's ceiling holds.
        let greedy = flight(&Look {
            fps: 240,
            ..Look::default()
        });
        assert!(greedy.interval_ms() >= 1000 / 30);
    }

    /// The whole point: a screensaver whose picture stands still saves nothing.
    #[test]
    fn the_picture_is_never_the_same_twice() {
        let mut scene = flight(&Look::default());
        let first = scene.frame(0).to_vec();
        let mut same = 0;
        let mut last = first.clone();
        for n in 1..200 {
            let now = scene.frame(n * 50).to_vec();
            if now == last {
                same += 1;
            }
            last = now;
        }
        assert_eq!(same, 0, "the picture stood still");
        assert_ne!(last, first);
    }

    #[test]
    fn the_ship_gets_a_hole_to_fly_through_and_flies_at_it() {
        let mut scene = flight(&Look::default());
        // Long enough for a field or two, a step at a time.
        fly(&mut scene, 200);
        assert!(scene.seed != super::FLIGHT_SEED, "nothing ever happened");
    }

    #[test]
    fn no_rock_is_ever_born_in_the_hole_left_for_the_ship() {
        let mut scene = flight(&Look::default());
        for _ in 0..40 {
            scene.rocks.clear();
            scene.scatter();
            let (gx, gy) = scene.gap.expect("a field has a hole through it");
            for rock in &scene.rocks {
                let (dx, dy) = (i64::from(rock.x - gx), i64::from(rock.y - gy));
                let clear = dx * dx + dy * dy;
                assert!(
                    clear > i64::from(GAP) * i64::from(GAP),
                    "a rock in the corridor the ship is steered down"
                );
                // And the ship gets past its edge, not just its centre.
                let edge = i64::from(GAP - rock.radius);
                assert!(
                    edge > 0 && clear > edge * edge,
                    "a rock wide enough to fill the hole"
                );
            }
        }
    }

    #[test]
    fn nothing_ever_arrives_where_the_ship_is() {
        let mut scene = flight(&Look::default());
        let step = scene.interval_ms().max(1);
        for n in 1..(600_000 / step) {
            scene.frame(n * step);
            for rock in &scene.rocks {
                // Close enough to be a collision: the rock's own radius, ahead
                // of the camera and level with it.
                let near = rock.z < NEAR * 4;
                let hit = rock.x.abs() < rock.radius && rock.y.abs() < rock.radius;
                assert!(
                    !(near && hit),
                    "the ship flew into a rock at {}ms",
                    n * step
                );
            }
        }
    }

    #[test]
    fn turning_it_off_means_there_is_nothing_to_steer_round() {
        let mut scene = flight(&Look {
            rocks: false,
            ..Look::default()
        });
        fly(&mut scene, 400);
        assert!(scene.rocks.is_empty(), "a field arrived after all");
    }

    #[test]
    fn a_stalled_clock_does_not_teleport_the_ship_through_anything() {
        let mut scene = flight(&Look::default());
        scene.frame(0);
        // Ten minutes in one step: the flight moves by one step's worth.
        scene.frame(600_000);
        assert!(
            scene.stars.iter().all(|s| s.z >= NEAR && s.z <= FAR),
            "a jump carried the sky away with it"
        );
    }

    #[test]
    fn a_screen_too_small_for_the_block_it_was_given_still_draws() {
        for (w, h) in [(320u32, 240u32), (640, 480), (1920, 1080)] {
            let mut scene = Starfield::compose(Size::new(w, h), &Look::default());
            let small = scene.small();
            assert!(small.width >= 1 && small.height >= 1);
            assert_eq!(
                scene.frame(1_000).len(),
                small.width as usize * small.height as usize
            );
        }
    }
}

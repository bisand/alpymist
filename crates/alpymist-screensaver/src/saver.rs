//! The screensaver as a widget: whichever picture, blown up onto the screen.
//!
//! Nothing here knows what is being drawn. A [`Painting`] hands over a small
//! picture and says how small; this expands it in square blocks, paces the
//! frames and takes it all away at the first key or movement. That is why
//! adding a screensaver is a module and a variant rather than a second copy of
//! this file.
//!
//! The expansion is one memory copy per row, which is what keeps this
//! affordable on the machines Alpymist exists for: a screensaver is by
//! definition the thing running when nothing else is, and one that keeps a core
//! busy drains the battery it was supposed to be idling through.

use crate::paint::{Compose, FRAME, Painting};
use crate::scene::expand;
use alpymist_ui::palette::Palette;
use alpymist_widget::{Key, Outcome, Widget};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use std::time::Instant;

/// The screensaver: whatever the program handed over, on the screen.
pub struct Saver {
    /// How to compose the picture for a screen of a given size.
    compose: Box<Compose>,
    /// The composed picture, and the screen it was composed for.
    picture: Option<(Size, Box<dyn Painting>)>,
    /// One expanded row, reused every row of every frame.
    row: Vec<u32>,
    /// The small picture as it was last painted, to tell which of its rows
    /// the next one changes.
    last: Vec<u32>,
    /// The parts of the screen the last frame changed. `None` is all of it.
    changed: Option<Vec<Rect>>,
    /// How small the picture in `last` is, and how far it is blown up.
    small: Size,
    block: u32,
    /// Which drawn row `row` holds, blown up, when it holds one.
    expanded: Option<u32>,
    started: Instant,
    /// Where the pointer was when it first reached the surface.
    ///
    /// A surface that appears under a still pointer is sent an enter, and that
    /// is not somebody moving the mouse. Only a position different from the
    /// first one is.
    pointer: Option<Point>,
}

impl Saver {
    /// A screensaver drawing whatever `compose` makes.
    #[must_use]
    pub fn new(compose: Box<Compose>) -> Self {
        Self {
            compose,
            picture: None,
            row: Vec::new(),
            last: Vec::new(),
            changed: None,
            small: Size::new(0, 0),
            block: 1,
            expanded: None,
            started: Instant::now(),
            pointer: None,
        }
    }

    /// Draw the next frame, small, and work out what of the screen it
    /// changes.
    fn next(&mut self, output: Size) {
        // A picture is composed for one screen size. A different one — the
        // output changed, or this is the first frame — composes again.
        if self.picture.as_ref().is_none_or(|(was, _)| *was != output) {
            self.picture = Some((output, (self.compose)(output)));
            self.last.clear();
        }
        let elapsed = self.elapsed();
        let Some((_, picture)) = self.picture.as_mut() else {
            return;
        };
        let small = picture.small();
        let block = picture.block();
        let pixels = picture.frame(elapsed);
        // Much of a picture is the same from one frame to the next — the sky
        // over a drifting ridge — and rows that are the same need not be
        // handed to the compositor again. Telling costs a look at the small
        // picture, which is a sixteenth of the screen or less.
        self.changed = differing(&self.last, pixels, small.width as usize).map(|bands| {
            let screen = |drawn: usize| {
                i32::try_from(drawn)
                    .unwrap_or(i32::MAX)
                    .saturating_mul(i32::try_from(block).unwrap_or(1))
            };
            bands
                .into_iter()
                .map(|rows| {
                    Rect::new(
                        0,
                        screen(rows.start),
                        screen(small.width as usize),
                        screen(rows.end - rows.start),
                    )
                })
                .collect()
        });
        self.last.clear();
        self.last.extend_from_slice(pixels);
        self.small = small;
        self.block = block;
        self.expanded = None;
    }

    /// How long it has been up, in milliseconds.
    fn elapsed(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Widget for Saver {
    type Event = ();

    /// Ignored: the host gives a full-screen widget the whole output.
    fn layout(&mut self, _scale: u32) -> Size {
        self.picture
            .as_ref()
            .map_or(Size::new(1, 1), |(output, _)| *output)
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        self.next(frame.size());
        expand(
            &self.last,
            self.small,
            self.block,
            &mut self.row,
            |y, line| {
                if let Some(dst) = frame.row_mut(y) {
                    let n = dst.len().min(line.len());
                    dst[..n].copy_from_slice(&line[..n]);
                }
            },
        );
        self.expanded = None;
    }

    /// The picture is handed over a row at a time: it is cheap to blow a row
    /// up and the screen is large, so painting the whole of it somewhere
    /// first, to be copied from, is the larger half of what a frame costs.
    fn streams(&mut self, size: Size) -> bool {
        self.next(size);
        true
    }

    fn row(&mut self, y: u32) -> (&[u32], u32) {
        let block = self.block.max(1);
        let wide = self
            .picture
            .as_ref()
            .map_or(0, |(output, _)| output.width as usize);
        let drawn = y / block;
        if self.expanded != Some(drawn) || self.row.len() != wide {
            let width = self.small.width as usize;
            let line = self
                .last
                .get(drawn as usize * width..(drawn as usize + 1) * width)
                .unwrap_or(&[]);
            self.row.clear();
            for px in line {
                self.row.extend(std::iter::repeat_n(*px, block as usize));
            }
            // What the blocks do not reach, where they do not divide the
            // screen, and all of a row past the last: the sky's own colour.
            self.row.resize(wide, backdrop());
            self.expanded = Some(drawn);
        }
        (&self.row, block - y % block)
    }

    fn changed(&self) -> Option<&[Rect]> {
        self.changed.as_deref()
    }

    /// Any key at all takes it away.
    fn key(&mut self, _key: Key) -> Outcome {
        Outcome::Close
    }

    fn text(&mut self, _ch: char) -> Outcome {
        Outcome::Close
    }

    fn press(&mut self, _at: Point) -> Outcome {
        Outcome::Close
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Close
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let Some(at) = at else {
            return Outcome::Unchanged;
        };
        match self.pointer {
            None => {
                self.pointer = Some(at);
                Outcome::Unchanged
            }
            Some(first) if first == at => Outcome::Unchanged,
            Some(_) => Outcome::Close,
        }
    }

    fn animating(&self) -> bool {
        true
    }

    /// What the picture asks for, once there is one to ask.
    ///
    /// Re-read between frames by the host, so the first frame — drawn before
    /// anything has been composed — pacing at the default costs one frame of
    /// waiting and nothing else.
    fn frame_interval(&self) -> std::time::Duration {
        let ms = self
            .picture
            .as_ref()
            .map_or(FRAME, |(_, picture)| picture.interval_ms().max(1));
        std::time::Duration::from_millis(ms)
    }
}

/// The rows `now` has different from `before`, in rows of `width`, as bands
/// of rows that lie together. `None` when they cannot be compared: no picture
/// before, or one of another size.
///
/// Whole rows and not the part of each that changed. Measured on the Atom
/// (ADR 0026): a span to each row is fewer bytes to hand over and hundreds
/// more writes to hand them over in, and the writes cost more than the bytes
/// save — for the screensaver and for the compositor, which is told of every
/// one. Rows that lie together are one write however many they are.
fn differing(before: &[u32], now: &[u32], width: usize) -> Option<Vec<core::ops::Range<usize>>> {
    if width == 0 || before.len() != now.len() {
        return None;
    }
    let mut bands: Vec<core::ops::Range<usize>> = Vec::new();
    let rows = before.chunks(width).zip(now.chunks(width)).enumerate();
    for (row, _) in rows.filter(|(_, (was, is))| was != is) {
        match bands.last_mut() {
            Some(band) if band.end == row => band.end = row + 1,
            _ => bands.push(row..row + 1),
        }
    }
    Some(bands)
}

/// The colour behind everything, for the instant before the first frame is
/// drawn: the top of the sky, opaque — which is the same `0b121e` the lock
/// screen uses, so one running into the other shows no seam.
#[must_use]
pub fn backdrop() -> u32 {
    let sky = Palette::alpymist().sky_high;
    u32::from_be_bytes([0xFF, sky.r, sky.g, sky.b])
}

#[cfg(test)]
// One band of rows is a list of one range, and is meant.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::Saver;
    use crate::paint::{FRAME, Painting};
    use alpymist_widget::{Key, Outcome, Widget};
    use denise::geom::{Point, Rect, Size};

    /// A picture of one flat colour, to exercise the host with no scene behind it.
    struct Flat {
        small: Size,
        block: u32,
        pixels: Vec<u32>,
    }

    impl Painting for Flat {
        fn small(&self) -> Size {
            self.small
        }
        fn block(&self) -> u32 {
            self.block
        }
        fn frame(&mut self, _elapsed: u64) -> &[u32] {
            &self.pixels
        }
    }

    fn saver() -> Saver {
        Saver::new(Box::new(|output: Size| {
            let small = Size::new(
                output.width.div_ceil(4).max(1),
                output.height.div_ceil(4).max(1),
            );
            Box::new(Flat {
                small,
                block: 4,
                pixels: vec![0xFF_00_00_00; small.width as usize * small.height as usize],
            })
        }))
    }

    #[test]
    fn only_the_rows_that_differ_are_said_to_have_changed() {
        use super::differing;
        let before = [1, 1, 2, 2, 3, 3, 4, 4];
        assert_eq!(differing(&before, &before, 2), Some(vec![]));
        assert_eq!(
            differing(&before, &[1, 1, 2, 9, 3, 3, 4, 4], 2),
            Some(vec![1..2])
        );
        assert_eq!(
            differing(&before, &[9, 1, 2, 2, 3, 3, 9, 9], 2),
            Some(vec![0..1, 3..4]),
            "two bands with rows between them that did not change"
        );
        assert_eq!(
            differing(&before, &[1, 9, 9, 2, 3, 9, 4, 4], 2),
            Some(vec![0..3]),
            "rows that lie together are one band"
        );
        assert_eq!(differing(&[], &before, 2), None, "nothing to compare with");
        assert_eq!(differing(&before, &before[..6], 2), None, "another size");
    }

    #[test]
    fn a_frame_says_which_rows_of_the_screen_it_changed() {
        struct Stripe(Vec<u32>, u64);
        impl Painting for Stripe {
            fn small(&self) -> Size {
                Size::new(4, 4)
            }
            fn block(&self) -> u32 {
                4
            }
            fn frame(&mut self, _: u64) -> &[u32] {
                // One drawn pixel changes with every frame: the second of
                // the second row.
                self.1 += 1;
                self.0[5] = 0xFF00_0000 | u32::try_from(self.1).unwrap_or(0);
                &self.0
            }
        }
        let mut saver = Saver::new(Box::new(|_| Box::new(Stripe(vec![0xFF00_0000; 16], 0))));
        let mut pixels = vec![0u32; 16 * 16];
        let mut paint = |saver: &mut Saver| {
            let mut frame = denise::Frame::new(
                &mut pixels,
                Size::new(16, 16),
                16,
                denise::PixelFormat::Argb8888,
                denise::BufferAge::Undefined,
            )
            .expect("a frame");
            saver.paint(&mut frame);
        };
        assert_eq!(saver.changed(), None, "nothing painted yet");
        paint(&mut saver);
        assert_eq!(saver.changed(), None, "the first frame is all new");
        paint(&mut saver);
        assert_eq!(
            saver.changed(),
            Some(&[Rect::new(0, 4, 16, 4)][..]),
            "one drawn row, four high on the screen"
        );
    }

    #[test]
    fn a_row_asked_for_is_the_row_painting_would_have_put_there() {
        // A picture whose every drawn pixel is different, on a screen its
        // blocks do not divide.
        fn stripes() -> Saver {
            Saver::new(Box::new(|_| {
                Box::new(Flat {
                    small: Size::new(4, 3),
                    block: 3,
                    pixels: (0..12).map(|n| 0xFF00_0000 | n).collect(),
                })
            }))
        }
        let size = Size::new(11, 8);
        let mut painted = vec![0u32; 11 * 8];
        {
            let mut frame = denise::Frame::new(
                &mut painted,
                size,
                11,
                denise::PixelFormat::Argb8888,
                denise::BufferAge::Undefined,
            )
            .expect("a frame");
            stripes().paint(&mut frame);
        }
        let mut saver = stripes();
        assert!(saver.streams(size));
        // Out of order, and some twice: rows are asked for as they are
        // wanted.
        for y in [7, 0, 3, 4, 3, 1, 2, 6, 5] {
            let (row, same) = saver.row(y);
            assert_eq!(
                row,
                &painted[y as usize * 11..(y as usize + 1) * 11],
                "row {y}"
            );
            assert_eq!(same, 3 - y % 3, "rows the same as row {y}");
        }
    }

    #[test]
    fn a_row_past_the_picture_is_the_sky() {
        let mut saver = saver();
        assert!(saver.streams(Size::new(8, 8)));
        let (row, same) = saver.row(400);
        assert_eq!(row, [super::backdrop(); 8]);
        assert!(same >= 1);
    }

    #[test]
    fn any_key_or_click_takes_it_away() {
        assert_eq!(saver().key(Key::Escape), Outcome::Close);
        assert_eq!(saver().text('a'), Outcome::Close);
        assert_eq!(saver().press(Point::new(0, 0)), Outcome::Close);
        assert_eq!(saver().scroll(1), Outcome::Close);
    }

    #[test]
    fn a_pointer_that_has_not_moved_is_not_somebody_arriving() {
        let mut saver = saver();
        let still = Point::new(400, 300);
        assert_eq!(saver.pointer(Some(still)), Outcome::Unchanged, "the enter");
        assert_eq!(saver.pointer(Some(still)), Outcome::Unchanged, "and again");
        assert_eq!(saver.pointer(None), Outcome::Unchanged, "and leaving");
        assert_eq!(saver.pointer(Some(Point::new(401, 300))), Outcome::Close);
    }

    /// A picture that wants every frame it can have.
    struct Quick(Flat);

    impl Painting for Quick {
        fn small(&self) -> Size {
            self.0.small()
        }
        fn block(&self) -> u32 {
            self.0.block()
        }
        fn frame(&mut self, elapsed: u64) -> &[u32] {
            self.0.frame(elapsed)
        }
        fn interval_ms(&self) -> u64 {
            33
        }
    }

    #[test]
    fn a_picture_that_asks_for_more_frames_gets_them_once_it_exists() {
        let mut saver = Saver::new(Box::new(|output: Size| {
            let small = Size::new(output.width.max(1), output.height.max(1));
            Box::new(Quick(Flat {
                small,
                block: 1,
                pixels: vec![0xFF00_0000; small.width as usize * small.height as usize],
            }))
        }));
        assert_eq!(
            saver.frame_interval().as_millis(),
            u128::from(FRAME),
            "nothing composed yet, so nothing has asked"
        );
        let mut pixels = vec![0u32; 16 * 16];
        let mut frame = denise::Frame::new(
            &mut pixels,
            Size::new(16, 16),
            16,
            denise::PixelFormat::Argb8888,
            denise::BufferAge::Undefined,
        )
        .expect("a frame");
        saver.paint(&mut frame);
        drop(frame);
        assert_eq!(saver.frame_interval().as_millis(), 33);
    }

    #[test]
    fn it_keeps_animating_and_asks_for_the_frames_it_wants() {
        let saver = saver();
        assert!(saver.animating());
        assert_eq!(saver.frame_interval().as_millis(), u128::from(FRAME));
    }

    #[test]
    fn the_colour_behind_everything_is_opaque() {
        assert_eq!(
            super::backdrop() >> 24,
            0xFF,
            "a transparent edge shows the desktop"
        );
    }
}

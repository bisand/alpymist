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

use crate::paint::{Compose, Painting};
use crate::scene::expand;
use alpymist_ui::palette::Palette;
use alpymist_widget::{Key, Outcome, Widget};
use denise::Frame;
use denise::geom::{Point, Size};
use std::time::Instant;

/// How often the picture is redrawn, in milliseconds.
///
/// Eight frames a second. Nothing a screensaver draws needs to move faster than
/// the eye drifting over it, and each frame that is not drawn is a frame's worth
/// of battery: on the 1366x768 panel of an Atom laptop, every frame a second
/// costs about two per cent of a core.
pub const FRAME: u64 = 125;

/// The screensaver: whatever the program handed over, on the screen.
pub struct Saver {
    /// How to compose the picture for a screen of a given size.
    compose: Box<Compose>,
    /// The composed picture, and the screen it was composed for.
    picture: Option<(Size, Box<dyn Painting>)>,
    /// One expanded row, reused every row of every frame.
    row: Vec<u32>,
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
            started: Instant::now(),
            pointer: None,
        }
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
        let output = frame.size();
        // A picture is composed for one screen size. A different one — the
        // output changed, or this is the first frame — composes again.
        if self.picture.as_ref().is_none_or(|(was, _)| *was != output) {
            self.picture = Some((output, (self.compose)(output)));
        }
        let elapsed = self.elapsed();
        let Some((_, picture)) = self.picture.as_mut() else {
            return;
        };
        let small = picture.small();
        let block = picture.block();
        let pixels = picture.frame(elapsed);
        expand(pixels, small, block, &mut self.row, |y, line| {
            if let Some(dst) = frame.row_mut(y) {
                let n = dst.len().min(line.len());
                dst[..n].copy_from_slice(&line[..n]);
            }
        });
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

    fn frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(FRAME)
    }
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
mod tests {
    use super::Saver;
    use crate::paint::Painting;
    use alpymist_widget::{Key, Outcome, Widget};
    use denise::geom::{Point, Size};

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

    #[test]
    fn it_keeps_animating_and_asks_for_the_frames_it_wants() {
        let saver = saver();
        assert!(saver.animating());
        assert_eq!(saver.frame_interval().as_millis(), u128::from(super::FRAME));
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

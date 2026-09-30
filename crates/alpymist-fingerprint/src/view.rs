//! The fingerprint window, laid out and painted.
//!
//! ```text
//!  Fingerprints
//!  Synaptics Metallica MIS
//!     ┌─┐┌─┐                      ┌─┐┌─┐
//!   ┌─┤ ││ ├─┐                  ┌─┤ ││ ├─┐
//!   │ │ ││ │ │ ┌┐            ┌┐ │ │ ││ │ │
//!   └─┴─┴┴─┴─┘ ││            ││ └─┴─┴┴─┴─┘
//!   │  left    ├┘            └┤   right   │
//!   └──────────┘              └───────────┘
//!  Right index finger · enrolled
//!  Put your finger on the sensor, and lift it again.
//!  [██████░░░░░░░░]
//!                         [ Remove all ] [ Test ] [ Add again ]
//! ```

// Geometry reads best in the letters it is written in.
#![allow(clippy::many_single_char_names)]

use crate::enrol::{Doing, Enrol, FINGERS, Said, spoken};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;

/// The window's width in logical pixels at a 16 px font.
pub const WIDTH: i32 = 560;

/// Something that can be clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A finger, by index into [`FINGERS`].
    Finger(usize),
    /// Add the chosen finger, or add it again.
    Add,
    /// Test a finger.
    Test,
    /// Remove every finger.
    Remove,
    /// Stop enrolling or testing.
    Cancel,
}

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Sizes everything is measured in.
    pub metrics: Metrics,
    /// The whole window.
    pub size: Size,
    title: Rect,
    subtitle: Rect,
    palms: [Rect; 2],
    fingers: [Rect; 10],
    name: Rect,
    status: Rect,
    meter: Rect,
    /// The buttons there are now, right to left.
    buttons: Vec<(Target, Rect, &'static str)>,
}

/// Where a finger is on a hand `w` by `h`, as a left hand: from its left
/// edge and top, and its width and height, in thousandths.
const LEFT_HAND: [(i32, i32, i32, i32); 5] = [
    // thumb, index, middle, ring, little
    (740, 430, 170, 360),
    (620, 100, 130, 520),
    (470, 20, 130, 600),
    (320, 80, 130, 540),
    (170, 220, 120, 400),
];
/// The palm, likewise.
const PALM: (i32, i32, i32, i32) = (160, 540, 640, 440);

impl Layout {
    /// The window for `state`, `size` physical pixels at `scale`.
    #[must_use]
    pub fn new(
        appearance: &Appearance,
        fonts: &mut Fonts,
        state: &Enrol,
        size: Size,
        scale: u32,
    ) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let st = fonts.styles(&m);
        let engine = &mut fonts.engine;
        let u = m.unit;
        let window_w = i32::try_from(size.width).unwrap_or(0);
        let w = m.inner_w().min(window_w - 2 * m.pad);
        let x = (window_w - w) / 2;
        let mut y = m.pad;

        let title = Rect::new(x, y, w, engine.line_height(st.large));
        y = title.bottom();
        let subtitle = Rect::new(x, y, w, engine.line_height(st.text));
        y = subtitle.bottom() + u;

        // The hands, side by side, the palms facing the reader.
        let hand_h = u * 9;
        let hand_w = (w - u * 2) / 2;
        let hands = [
            Rect::new(x, y, hand_w, hand_h),
            Rect::new(x + w - hand_w, y, hand_w, hand_h),
        ];
        let at = |hand: Rect, (fx, fy, fw, fh): (i32, i32, i32, i32), mirror: bool| {
            let rx = if mirror { 1000 - fx - fw } else { fx };
            Rect::new(
                hand.x + hand.width * rx / 1000,
                hand.y + hand.height * fy / 1000,
                hand.width * fw / 1000,
                hand.height * fh / 1000,
            )
        };
        let palms = [at(hands[0], PALM, false), at(hands[1], PALM, true)];
        let mut fingers = [Rect::new(0, 0, 0, 0); 10];
        for (i, place) in LEFT_HAND.iter().enumerate() {
            fingers[i] = at(hands[0], *place, false);
            fingers[5 + i] = at(hands[1], *place, true);
        }
        y += hand_h + u;

        let name = Rect::new(x, y, w, engine.line_height(st.strong));
        y = name.bottom() + u / 4;
        let status = Rect::new(x, y, w, engine.line_height(st.text) * 2);
        y = status.bottom() + u / 4;
        let meter = Rect::new(x, y, w, m.px(6));
        y = meter.bottom() + u;

        let labels: Vec<(Target, &'static str)> = if state.idle() {
            let mut b = vec![(
                Target::Add,
                if state.chosen_enrolled() {
                    "Add again"
                } else {
                    "Add"
                },
            )];
            if !state.enrolled.is_empty() {
                b.push((Target::Test, "Test"));
                b.push((Target::Remove, "Remove all"));
            }
            b
        } else if matches!(state.doing, Doing::Enrolling { .. } | Doing::Testing) {
            vec![(Target::Cancel, "Cancel")]
        } else {
            Vec::new()
        };
        let bh = u * 2;
        let mut right = x + w;
        let mut buttons = Vec::new();
        for (target, label) in labels {
            let bw = (engine.measure_line(st.strong, label) + u * 2).max(u * 5);
            buttons.push((target, Rect::new(right - bw, y, bw, bh), label));
            right -= bw + u / 2;
        }
        y += bh + m.pad;
        let _ = y;
        Self {
            metrics: m,
            size,
            title,
            subtitle,
            palms,
            fingers,
            name,
            status,
            meter,
            buttons,
        }
    }

    /// The height the window wants, in logical pixels, for a width.
    #[must_use]
    pub fn height(appearance: &Appearance, fonts: &mut Fonts, state: &Enrol) -> u32 {
        let l = Self::new(
            appearance,
            fonts,
            state,
            Size::new(WIDTH.unsigned_abs(), 2000),
            1,
        );
        let bottom = l
            .buttons
            .iter()
            .map(|(_, r, _)| r.bottom())
            .max()
            .unwrap_or(l.meter.bottom() + l.metrics.unit * 3);
        u32::try_from(bottom + l.metrics.pad).unwrap_or(440)
    }

    /// What is at `p`.
    #[must_use]
    pub fn hit(&self, p: Point) -> Option<Target> {
        if let Some((t, _, _)) = self.buttons.iter().find(|(_, r, _)| r.contains(p)) {
            return Some(*t);
        }
        self.fingers
            .iter()
            .position(|r| r.contains(p))
            .map(Target::Finger)
    }
}

/// Paint the window.
#[allow(clippy::too_many_lines)] // the window, top to bottom
pub fn paint(
    frame: &mut Frame<'_>,
    l: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    state: &Enrol,
    hover: Option<Target>,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let m = &l.metrics;
    let st = fonts.styles(m);
    let ink = Ink::new(appearance);
    pen.fill_rect(
        Rect::new(
            0,
            0,
            i32::try_from(l.size.width).unwrap_or(0),
            i32::try_from(l.size.height).unwrap_or(0),
        ),
        ink.background,
    );
    let engine = &mut fonts.engine;

    draw::label(
        &mut pen,
        engine,
        st.large,
        l.title,
        "Fingerprints",
        ink.text,
    );
    let subtitle = match &state.reader {
        Some(name) => name.clone(),
        None => match &state.said {
            Said::Failed(why) => why.clone(),
            _ => "Looking for a fingerprint reader…".into(),
        },
    };
    draw::label(&mut pen, engine, st.text, l.subtitle, &subtitle, ink.dim);

    // The hands.
    for palm in l.palms {
        pen.fill_rounded_rect(palm, palm.width / 6, ink.card);
    }
    for (i, r) in l.fingers.iter().enumerate() {
        let enrolled = state.enrolled.iter().any(|f| f == FINGERS[i]);
        let fill = if enrolled { ink.accent } else { ink.card };
        pen.fill_rounded_rect(*r, r.width / 2, fill);
        if i == state.chosen && state.reader.is_some() {
            pen.stroke_rounded_rect(*r, r.width / 2, m.px(3), ink.text);
        } else if hover == Some(Target::Finger(i)) && state.idle() {
            pen.stroke_rounded_rect(*r, r.width / 2, m.px(2), ink.dim);
        }
    }

    if state.reader.is_some() {
        let finger = FINGERS[state.chosen];
        let enrolled = if state.chosen_enrolled() {
            "enrolled"
        } else {
            "not enrolled"
        };
        draw::label(
            &mut pen,
            engine,
            st.strong,
            l.name,
            &format!("{} · {enrolled}", spoken(finger)),
            ink.text,
        );
    }
    let (said, colour) = match &state.said {
        Said::Nothing if state.reader.is_some() => (
            if state.enrolled.is_empty() {
                "Choose a finger, then Add, and touch the sensor with it a few times."
            } else {
                "Choose a finger to add it or add it again, or Test one."
            }
            .to_owned(),
            ink.dim,
        ),
        Said::Nothing => (String::new(), ink.dim),
        Said::Done(t) => (t.clone(), ink.accent),
        Said::Again(t) => (t.clone(), ink.text),
        Said::Failed(t) if state.reader.is_some() => (t.clone(), ink.warn),
        Said::Failed(_) => (String::new(), ink.warn),
    };
    let line = engine.line_height(st.text);
    for (n, text) in engine
        .wrap(st.text, &said, l.status.width)
        .into_iter()
        .take(2)
        .enumerate()
    {
        let y = l.status.y + i32::try_from(n).unwrap_or(0) * line;
        engine.draw(&mut pen, st.text, Point::new(l.status.x, y), text, colour);
    }
    if let Doing::Enrolling { taken, of } = state.doing {
        draw::meter(
            &mut pen,
            l.meter,
            f64::from(taken) / f64::from(of.max(1)),
            ink.accent,
            ink.selection,
        );
    }

    for (target, r, label) in &l.buttons {
        let hovered = hover == Some(*target);
        if matches!(target, Target::Add) {
            let fill = if hovered {
                draw::mix(appearance.accent, appearance.text, 20)
            } else {
                ink.accent
            };
            draw::button(
                &mut pen,
                engine,
                st.strong,
                *r,
                label,
                (Some(fill), ink.on_accent),
                None,
            );
        } else {
            draw::outline_button(&mut pen, engine, st.text, *r, label, hovered, m, &ink);
        }
    }
}

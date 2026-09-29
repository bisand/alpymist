//! `alpymist-settings --identify OUTPUT NUMBER`: the number the Displays page
//! gives a screen, large, in the middle of that screen, for a few seconds.
//! Settings starts one for each screen that is on. It takes no keyboard, and
//! a press anywhere takes it away early.

use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use alpymist_widget::host::{self, Placement};
use alpymist_widget::{Appearance, Key, Outcome, Widget};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;
use denise_text::TextStyle;
use std::time::Duration;

/// How long the number stays.
const SHOWN: Duration = Duration::from_secs(3);

/// The panel's side, in logical pixels.
const SIDE: i32 = 180;

struct Number {
    appearance: Appearance,
    fonts: Fonts,
    text: String,
    name: String,
    metrics: Option<Metrics>,
}

/// Time is up.
struct Done;

impl Widget for Number {
    type Event = Done;

    fn layout(&mut self, scale: u32) -> Size {
        let metrics = Metrics::new(&self.appearance, scale, SIDE);
        let size = metrics.size(metrics.px(SIDE) - metrics.border);
        self.metrics = Some(metrics);
        size
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        let Some(m) = self.metrics else {
            return;
        };
        let mut canvas = Canvas::new(frame);
        let mut pen = Pen::new(&mut canvas);
        let size = pen.size();
        let ink = Ink::new(&self.appearance);
        draw::panel(&mut pen, size, &m, &ink);
        let big = TextStyle {
            font: self.fonts.strong,
            size_px: u16::try_from(m.px(96)).unwrap_or(96),
        };
        let styles = self.fonts.styles(&m);
        let w = i32::try_from(size.width).unwrap_or(0);
        let h = i32::try_from(size.height).unwrap_or(0);
        let engine = &mut self.fonts.engine;
        let number = engine.measure(big, &self.text);
        let nw = i32::try_from(number.width).unwrap_or(0);
        let nh = i32::try_from(number.height).unwrap_or(0);
        let line = engine.line_height(styles.small);
        let top = (h - nh - line) / 2;
        engine.draw(
            &mut pen,
            big,
            Point::new((w - nw) / 2, top),
            &self.text,
            ink.accent,
        );
        draw::centred(
            &mut pen,
            engine,
            styles.small,
            Rect::new(0, top + nh, w, line),
            &self.name,
            ink.dim,
        );
    }

    fn key(&mut self, _: Key) -> Outcome {
        Outcome::Unchanged
    }

    fn press(&mut self, _: Point) -> Outcome {
        Outcome::Close
    }

    fn event(&mut self, _: Done) -> Outcome {
        Outcome::Close
    }
}

/// Show `number` on `output` for a few seconds.
pub fn run(output: &str, number: &str) -> Result<(), String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    let (sender, events) = host::events();
    std::thread::spawn(move || {
        std::thread::sleep(SHOWN);
        let _ = sender.send(Done);
    });
    let widget = Number {
        appearance,
        fonts,
        text: number.to_owned(),
        name: output.to_owned(),
        metrics: None,
    };
    let options = host::Options {
        placement: Placement::Centre,
        output: Some(output.to_owned()),
        keyboard: false,
        ..host::Options::new("alpymist-settings-identify")
    };
    host::run(widget, &options, events, None).map(drop)
}

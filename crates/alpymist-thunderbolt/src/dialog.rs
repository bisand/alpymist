//! The question on screen: a layer surface in the middle of the output, the
//! rest dimmed, holding the keyboard like the password prompt that follows.

use alpymist_thunderbolt::ask::{self, Answer, Dialog, Key as AskKey, Question};
use alpymist_thunderbolt::sysfs::{Device, Domain};
use alpymist_thunderbolt::system::Keep;
use alpymist_thunderbolt::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Placement};
use alpymist_widget::{Appearance, Key, Outcome, Widget};
use denise::Frame;
use denise::geom::{Point, Size};
use std::cell::Cell;
use std::rc::Rc;

struct Window {
    dialog: Dialog,
    appearance: Appearance,
    fonts: Fonts,
    layout: Option<Layout>,
    answer: Rc<Cell<Option<Answer>>>,
}

/// Ask about `device`: `Some(keep)` to let it in, `None` to leave it out.
/// Closing the dialog any other way — a click outside, the focus going
/// elsewhere — leaves it out.
pub fn ask(device: &Device, domain: Option<&Domain>) -> Result<Option<Keep>, String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    let answer = Rc::new(Cell::new(None));
    let window = Window {
        dialog: Dialog::new(Question::new(device, domain)),
        appearance,
        fonts,
        layout: None,
        answer: Rc::clone(&answer),
    };
    let options = host::Options {
        placement: Placement::Centre,
        backdrop: 0x9900_0000,
        ..host::Options::new("alpymist-thunderbolt")
    };
    let (_sender, events) = host::events::<()>();
    host::run(window, &options, events, None)?;
    Ok(match answer.get() {
        Some(Answer::Allow(keep)) => Some(keep),
        Some(Answer::Deny) | None => None,
    })
}

impl Window {
    fn apply(&mut self, outcome: ask::Outcome) -> Outcome {
        match outcome {
            ask::Outcome::Unchanged => Outcome::Unchanged,
            ask::Outcome::Redraw => Outcome::Redraw,
            ask::Outcome::Done(answer) => {
                self.answer.set(Some(answer));
                Outcome::Close
            }
        }
    }
}

impl Widget for Window {
    type Event = ();

    fn layout(&mut self, scale: u32) -> Size {
        let layout = Layout::new(&self.appearance, &mut self.fonts, &self.dialog, scale);
        let size = layout.size;
        self.layout = Some(layout);
        size
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if let Some(layout) = &self.layout {
            view::paint(
                frame,
                layout,
                &self.appearance,
                &mut self.fonts,
                &self.dialog,
            );
        }
    }

    fn key(&mut self, key: Key) -> Outcome {
        let key = match key {
            Key::Tab | Key::Right => AskKey::Next,
            Key::BackTab | Key::Left => AskKey::Previous,
            Key::Enter | Key::Space => AskKey::Activate,
            Key::Escape => AskKey::Cancel,
            _ => return Outcome::Unchanged,
        };
        let outcome = self.dialog.key(key);
        self.apply(outcome)
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let target = at.and_then(|p| self.layout.as_ref().and_then(|l| l.hit(p)));
        let outcome = self.dialog.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> Outcome {
        match self.layout.as_ref().and_then(|l| l.hit(at)) {
            Some(target) => {
                let outcome = self.dialog.click(target);
                self.apply(outcome)
            }
            None => Outcome::Unchanged,
        }
    }
}

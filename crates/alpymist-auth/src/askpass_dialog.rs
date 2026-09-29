//! The askpass dialog on screen: the polkit prompt's layer, dimmed backdrop
//! and exclusive keyboard, and Ctrl+Alt+Delete's check, around ssh's
//! question.

use crate::dialog;
use alpymist_auth::askpass::{Answer, Askpass, Kind, Outcome};
use alpymist_auth::askpass_view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Placement};
use alpymist_widget::{Appearance, Key, Outcome as AnyOutcome, Widget};
use denise::Frame;
use denise::geom::{Point, Size};

/// What happens while the dialog is up.
enum Event {
    /// Ctrl+Alt+Delete found this prompt genuine.
    Verified,
}

struct Dialog {
    appearance: Appearance,
    fonts: Fonts,
    ask: Askpass,
    layout: Option<Layout>,
    answer: std::rc::Rc<std::cell::RefCell<Option<Answer>>>,
}

impl Dialog {
    fn apply(&mut self, outcome: Outcome) -> AnyOutcome {
        match outcome {
            Outcome::Unchanged => AnyOutcome::Unchanged,
            Outcome::Redraw => AnyOutcome::Redraw,
            Outcome::Close(answer) => {
                *self.answer.borrow_mut() = Some(answer);
                AnyOutcome::Close
            }
        }
    }
}

impl Widget for Dialog {
    type Event = Event;

    fn layout(&mut self, scale: u32) -> Size {
        let layout = Layout::new(&self.appearance, &mut self.fonts, &self.ask, scale);
        let size = layout.size;
        self.layout = Some(layout);
        size
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if let Some(layout) = &self.layout {
            askpass_view::paint(frame, layout, &self.appearance, &mut self.fonts, &self.ask);
        }
    }

    fn key(&mut self, key: Key) -> AnyOutcome {
        let outcome = self.ask.key(key);
        self.apply(outcome)
    }

    fn text(&mut self, ch: char) -> AnyOutcome {
        let outcome = self.ask.text(ch);
        self.apply(outcome)
    }

    fn pointer(&mut self, at: Option<Point>) -> AnyOutcome {
        let target = at.and_then(|p| self.layout.as_ref().and_then(|l| l.hit(p)));
        let outcome = self.ask.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> AnyOutcome {
        let outcome = match self.layout.as_ref().and_then(|l| l.hit(at)) {
            Some(target) => self.ask.click(target),
            None => Outcome::Unchanged,
        };
        self.apply(outcome)
    }

    fn caps_lock(&mut self, on: bool) -> AnyOutcome {
        let outcome = self.ask.set_caps_lock(on);
        self.apply(outcome)
    }

    fn event(&mut self, event: Event) -> AnyOutcome {
        let outcome = match event {
            Event::Verified => self.ask.verify(),
        };
        self.apply(outcome)
    }
}

/// Ask `question`, of `kind`. `None` when it was cancelled or denied, or the
/// dialog went away without an answer.
pub fn ask(kind: Kind, question: &str) -> Result<Option<Answer>, String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    let home = std::env::var("HOME").ok();
    let mut ask = Askpass::new(kind, question, home.as_deref());
    let (sender, events) = host::events();
    let socket = dialog::listen(sender, || Event::Verified);
    ask.checkable = socket.is_some();
    let answer = std::rc::Rc::new(std::cell::RefCell::new(None));
    let dialog = Dialog {
        appearance,
        fonts,
        ask,
        layout: None,
        answer: std::rc::Rc::clone(&answer),
    };
    let options = host::Options {
        placement: Placement::Centre,
        backdrop: 0x9900_0000,
        // The polkit prompt's name: Ctrl+Alt+Delete vouches for what is
        // drawn under it only when it is this binary.
        ..host::Options::new(alpymist_auth::attention::NAMESPACE)
    };
    let result = host::run(dialog, &options, events, None);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result?;
    Ok(match answer.borrow_mut().take() {
        Some(Answer::No) | None => None,
        some => some,
    })
}

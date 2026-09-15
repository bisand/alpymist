//! The popup as a widget: what the host sees of it.
//!
//! The popup's logic and its view know nothing of the host, and the host
//! knows nothing of iwd. This joins them: keys and clicks go to the
//! [`Popup`], a [`Command`] it asks for goes to the worker driving iwd, and
//! the worker's and watcher's answers come back as [`Event`]s.

use alpymist_widget::{Appearance, Key as AnyKey, Outcome as AnyOutcome, Widget};
use alpymist_wifi::model::State;
use alpymist_wifi::popup::{Command, Key, Outcome, Popup};
use alpymist_wifi::view::{self, Fonts, Layout};
use denise::Frame;
use denise::geom::{Point, Size};
use std::sync::mpsc::Sender;

/// What the worker and watcher send.
pub enum Event {
    /// A fresh reading of iwd.
    State(State),
    /// A command finished.
    Done(Command, Result<(), String>),
}

/// The Wi-Fi popup, laid out and wired to its worker.
pub struct Wifi {
    appearance: Appearance,
    fonts: Fonts,
    popup: Popup,
    layout: Layout,
    worker: Sender<Command>,
}

impl Wifi {
    /// The popup, before iwd has said anything.
    pub fn new(appearance: Appearance, fonts: Fonts, worker: Sender<Command>) -> Self {
        let popup = Popup::new(view::ROWS);
        let layout = Layout::new(&appearance, &popup, 1);
        Self {
            appearance,
            fonts,
            popup,
            layout,
            worker,
        }
    }

    fn apply(&self, outcome: Outcome) -> AnyOutcome {
        match outcome {
            Outcome::Unchanged => AnyOutcome::Unchanged,
            Outcome::Redraw => AnyOutcome::Redraw,
            Outcome::Run(command) => {
                let _ = self.worker.send(command);
                AnyOutcome::Redraw
            }
            Outcome::Close => AnyOutcome::Close,
        }
    }
}

impl Widget for Wifi {
    type Event = Event;

    fn layout(&mut self, scale: u32) -> Size {
        self.layout = Layout::new(&self.appearance, &self.popup, scale);
        self.layout.size
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        view::paint(
            frame,
            &self.layout,
            &self.appearance,
            &mut self.fonts,
            &self.popup,
        );
    }

    fn key(&mut self, key: AnyKey) -> AnyOutcome {
        let key = match key {
            AnyKey::Up => Key::Up,
            AnyKey::Down => Key::Down,
            AnyKey::Home => Key::Home,
            AnyKey::End => Key::End,
            AnyKey::Tab => Key::Tab,
            AnyKey::BackTab => Key::BackTab,
            AnyKey::Enter => Key::Enter,
            AnyKey::Space => Key::Space,
            AnyKey::Escape => Key::Escape,
            AnyKey::Backspace => Key::Backspace,
            AnyKey::Clear => Key::Clear,
            AnyKey::Left | AnyKey::Right => return AnyOutcome::Unchanged,
        };
        let outcome = self.popup.key(key);
        self.apply(outcome)
    }

    fn text(&mut self, ch: char) -> AnyOutcome {
        let outcome = self.popup.text(ch);
        self.apply(outcome)
    }

    fn pointer(&mut self, at: Option<Point>) -> AnyOutcome {
        let target = at.and_then(|p| self.layout.hit(p));
        let outcome = self.popup.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> AnyOutcome {
        let outcome = self
            .layout
            .hit(at)
            .map_or(Outcome::Unchanged, |t| self.popup.click(t));
        self.apply(outcome)
    }

    fn scroll(&mut self, rows: i32) -> AnyOutcome {
        let outcome = self.popup.scroll_by(rows);
        self.apply(outcome)
    }

    fn row_height(&self) -> f64 {
        f64::from(self.layout.unit * 9 / 4) / f64::from(self.layout.scale)
    }

    fn event(&mut self, event: Event) -> AnyOutcome {
        let outcome = match event {
            Event::State(state) => self.popup.update(state),
            Event::Done(command, result) => self.popup.finished(&command, result),
        };
        self.apply(outcome)
    }
}

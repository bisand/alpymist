//! The popup as a widget: what the host sees of it.
//!
//! Keys and clicks go to the [`popup::Popup`], a [`Command`] it asks for goes
//! to the worker, and readings and finished commands come back as [`Event`]s.

use alpymist_ai_usage::popup::{self, Command, Outcome, Reading, Reply};
use alpymist_ai_usage::view::{self, Fonts, Layout};
use alpymist_widget::{Appearance, Key, Outcome as AnyOutcome, Widget};
use denise::Frame;
use denise::geom::{Point, Size};
use std::sync::mpsc::Sender;

/// What the watcher and worker send.
pub enum Event {
    /// A fresh reading.
    Reading(Reading),
    /// A command finished.
    Done(Command, Reply),
}

/// The AI usage popup, laid out and wired to its worker.
pub struct UsagePopup {
    appearance: Appearance,
    fonts: Fonts,
    popup: popup::Popup,
    layout: Layout,
    worker: Sender<Command>,
}

/// Start a program, and say so in the log if it would not.
fn start(line: &[String]) {
    let Some((program, args)) = line.split_first() else {
        return;
    };
    if let Err(e) = std::process::Command::new(program).args(args).spawn() {
        eprintln!("alpymist-ai-usage: {program}: {e}");
    }
}

impl UsagePopup {
    /// The popup, before anything is read.
    pub fn new(
        appearance: Appearance,
        fonts: Fonts,
        popup: popup::Popup,
        worker: Sender<Command>,
    ) -> Self {
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
            // The vendor's installer, where it can be watched, and whatever
            // the provider asks after it; the terminal stays until read.
            Outcome::Install(id) => {
                let command = [
                    "sh".to_owned(),
                    "-c".to_owned(),
                    format!(
                        "alpymist-ai-usage enable {id}; printf '\\nPress Enter to close. '; read -r _"
                    ),
                ];
                start(&alpymist_core::defaults::terminal_argv(
                    &alpymist_core::defaults::Places::current(),
                    &command,
                ));
                AnyOutcome::Close
            }
            Outcome::Settings => {
                start(&["alpymist-settings".to_owned(), "ai".to_owned()]);
                AnyOutcome::Close
            }
            Outcome::Close => AnyOutcome::Close,
        }
    }
}

impl Widget for UsagePopup {
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

    fn key(&mut self, key: Key) -> AnyOutcome {
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

    fn event(&mut self, event: Event) -> AnyOutcome {
        let outcome = match event {
            Event::Reading(reading) => self.popup.update(reading),
            Event::Done(command, reply) => self.popup.finished(&command, reply),
        };
        self.apply(outcome)
    }
}

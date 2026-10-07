//! The game in a window of the desktop's.

use alpymist_solitaire::kept::Kept;
use alpymist_solitaire::play::{self, Play};
use alpymist_widget::Outcome;
use alpymist_widget::host;
use alpymist_widget::window::{self, App, Cursor, Key, Mods};
use denise::Frame;
use denise::geom::{Point, Size};
use std::time::{SystemTime, UNIX_EPOCH};

/// The app id.
const NAME: &str = "alpymist-solitaire";

struct Window(Play);

/// Open the window and play until it closes. `turn` is how many cards the
/// stock turns when the command line says; otherwise it is as last time.
/// `nearly_out` sets out a game five cards from its end and not a deal.
pub fn run(turn: Option<usize>, nearly_out: bool) -> Result<(), String> {
    let dir = play::state();
    let mut kept = dir.as_deref().map_or_else(Kept::default, play::recall);
    if let Some(turn) = turn {
        kept.turn = turn;
    }
    // No two games alike, and nothing to keep secret: the clock will do.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seed = u64::try_from(now & u128::from(u64::MAX)).unwrap_or(0);
    let play = Play::new(alpymist_widget::appearance(), kept, dir, seed, nearly_out);
    for p in play.font_problems() {
        eprintln!("{NAME}: font {p}");
    }
    let options = window::Options {
        app_id: NAME.into(),
        min_size: (760, 480),
        max_size: None,
    };
    let (_sender, events) = host::events::<()>();
    window::run(Window(play), &options, events, None)
}

fn outcome(outcome: play::Outcome) -> Outcome {
    match outcome {
        play::Outcome::Unchanged => Outcome::Unchanged,
        play::Outcome::Redraw => Outcome::Redraw,
        play::Outcome::Close => Outcome::Close,
    }
}

impl App for Window {
    type Event = ();

    fn title(&self) -> String {
        "Solitaire".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        (1100, 760)
    }

    fn resize(&mut self, size: Size, scale: u32) {
        self.0.resize(size, scale);
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        self.0.paint(frame);
    }

    fn key(&mut self, key: Key, mods: Mods) -> Outcome {
        let key = match key {
            Key::Left => play::Key::Left,
            Key::Right => play::Key::Right,
            Key::Up => play::Key::Up,
            Key::Down => play::Key::Down,
            Key::Enter => play::Key::Enter,
            Key::Space => play::Key::Space,
            Key::Escape => play::Key::Escape,
            Key::Backspace => play::Key::Backspace,
            Key::Chord('z') if mods.ctrl => play::Key::Undo,
            Key::Chord('n') if mods.ctrl => play::Key::New,
            Key::Chord('q' | 'w') if mods.ctrl => play::Key::Quit,
            _ => return Outcome::Unchanged,
        };
        outcome(self.0.key(key))
    }

    fn text(&mut self, ch: char) -> Outcome {
        outcome(self.0.text(ch))
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        outcome(self.0.pointer(at))
    }

    fn press(&mut self, at: Point) -> Outcome {
        outcome(self.0.press(at))
    }

    fn release(&mut self, at: Point) -> Outcome {
        outcome(self.0.release(at))
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Unchanged
    }

    fn cursor(&self, at: Point) -> Cursor {
        if self.0.takes_a_press(at) {
            Cursor::Pointer
        } else {
            Cursor::Default
        }
    }

    fn animating(&self) -> bool {
        self.0.animating()
    }

    fn tick(&mut self) -> Outcome {
        outcome(self.0.tick())
    }

    fn event(&mut self, (): ()) -> Outcome {
        Outcome::Unchanged
    }
}

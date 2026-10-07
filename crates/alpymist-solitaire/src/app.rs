//! The game in a window: the hand that drags, the double click, the
//! keyboard, and a card's flight home.

use alpymist_solitaire::cards::Rng;
use alpymist_solitaire::faces::Faces;
use alpymist_solitaire::game::{Game, PILES, Place};
use alpymist_solitaire::view::{self, Button, Hit, Layout, Lifted, Scene};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host;
use alpymist_widget::window::{self, App, Cursor, Key, Mods};
use alpymist_widget::{Appearance, Outcome};
use denise::Frame;
use denise::geom::{Point, Size};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The app id.
const NAME: &str = "alpymist-solitaire";
/// How long a card takes to fly to where it was sent.
const FLIGHT: Duration = Duration::from_millis(140);
/// Two presses on a card within this are a double click.
const DOUBLE: Duration = Duration::from_millis(400);

/// Cards in the hand.
struct Drag {
    place: Place,
    count: usize,
    /// From the first card's corner to the pointer.
    grip: (i32, i32),
}

/// A card on its way to where the game already has it.
struct Flight {
    place: Place,
    from: Point,
    to: Point,
    since: Instant,
}

struct Solitaire {
    game: Game,
    seeds: Rng,
    appearance: Appearance,
    fonts: Fonts,
    faces: Faces,
    layout: Layout,
    pointer: Option<Point>,
    drag: Option<Drag>,
    flight: Option<Flight>,
    pressed: Option<(Hit, Instant)>,
    focus: Option<Place>,
    chosen: Option<(Place, usize)>,
}

/// Open the window and play until it closes.
pub fn run(turn: usize) -> Result<(), String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    for p in &fonts.problems {
        eprintln!("{NAME}: font {p}");
    }
    // No two games alike, and nothing to keep secret: the clock will do.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut seeds = Rng::new(u64::try_from(now & u128::from(u64::MAX)).unwrap_or(0));
    let layout = Layout::new(&appearance, Size::new(1100, 760), 1);
    let app = Solitaire {
        game: Game::new(seeds.number(), turn),
        seeds,
        appearance,
        fonts,
        faces: Faces::default(),
        layout,
        pointer: None,
        drag: None,
        flight: None,
        pressed: None,
        focus: None,
        chosen: None,
    };
    let options = window::Options {
        app_id: NAME.into(),
        min_size: (560, 420),
        max_size: None,
    };
    let (_sender, events) = host::events::<()>();
    window::run(app, &options, events, None)
}

/// The piles of the top row, by the column each stands in; the third
/// column is empty.
const ROW: [Option<Place>; PILES] = [
    Some(Place::Stock),
    Some(Place::Waste),
    None,
    Some(Place::Foundation(0)),
    Some(Place::Foundation(1)),
    Some(Place::Foundation(2)),
    Some(Place::Foundation(3)),
];

/// The column a pile stands in, and whether it is in the top row.
fn column(place: Place) -> (usize, bool) {
    match place {
        Place::Stock => (0, true),
        Place::Waste => (1, true),
        Place::Foundation(i) => (3 + i.min(3), true),
        Place::Tableau(i) => (i.min(PILES - 1), false),
    }
}

impl Solitaire {
    fn deal_again(&mut self, turn: usize) {
        self.game = Game::new(self.seeds.number(), turn);
        self.drag = None;
        self.flight = None;
        self.chosen = None;
    }

    fn undo(&mut self) -> Outcome {
        self.flight = None;
        self.chosen = None;
        if self.game.undo() {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    fn button(&mut self, button: Button) -> Outcome {
        match button {
            Button::New => self.deal_again(self.game.turn()),
            Button::Undo => return self.undo(),
            Button::Turn => self.deal_again(if self.game.turn() == 3 { 1 } else { 3 }),
        }
        Outcome::Redraw
    }

    /// Send the top card of `from` to its foundation, flying.
    fn send_home(&mut self, from: Place) -> bool {
        let start = self.layout.top(&self.game, from, 1);
        let Some(to) = self.game.play(from, 1, Place::Foundation(0)) else {
            return false;
        };
        let end = self.layout.top(&self.game, to, 1);
        self.flight = Some(Flight {
            place: to,
            from: Point::new(start.x, start.y),
            to: Point::new(end.x, end.y),
            since: Instant::now(),
        });
        true
    }

    /// The pile a hit means, and how many of its cards.
    fn picked(&self, hit: Hit) -> Option<(Place, usize)> {
        let (place, count) = match hit {
            Hit::Waste => (Place::Waste, 1),
            Hit::Foundation(i) => (Place::Foundation(i), 1),
            Hit::Pile { pile, count } => (Place::Tableau(pile), count),
            Hit::Stock | Hit::Button(_) => return None,
        };
        self.game.held(place, count).map(|_| (place, count))
    }

    fn scene(&self) -> Scene {
        let lifted = match (&self.drag, &self.flight, self.pointer) {
            (Some(drag), _, Some(at)) => Some(Lifted {
                place: drag.place,
                count: drag.count,
                at: Point::new(at.x - drag.grip.0, at.y - drag.grip.1),
            }),
            (None, Some(flight), _) => {
                let gone = flight.since.elapsed().as_millis().min(FLIGHT.as_millis());
                let part = i32::try_from(gone * 1000 / FLIGHT.as_millis().max(1)).unwrap_or(1000);
                // Fast at first and slow to land.
                let eased = 1000 - (1000 - part) * (1000 - part) / 1000;
                let between = |a: i32, b: i32| a + (b - a) * eased / 1000;
                Some(Lifted {
                    place: flight.place,
                    count: 1,
                    at: Point::new(
                        between(flight.from.x, flight.to.x),
                        between(flight.from.y, flight.to.y),
                    ),
                })
            }
            _ => None,
        };
        let hover = self
            .pointer
            .and_then(|at| match self.layout.hit(&self.game, at) {
                Some(Hit::Button(b)) => Some(b),
                _ => None,
            });
        Scene {
            lifted,
            hover,
            focus: self.focus,
            chosen: self.chosen,
        }
    }

    /// Move the keyboard's place one pile over.
    fn step(&mut self, key: Key) -> Outcome {
        let Some(at) = self.focus else {
            self.focus = Some(Place::Tableau(0));
            return Outcome::Redraw;
        };
        // On the pile picked up from, up and down take more cards or fewer.
        if let Some((place, count)) = self.chosen
            && place == at
            && matches!(place, Place::Tableau(_))
            && matches!(key, Key::Up | Key::Down)
        {
            let wanted = if key == Key::Up {
                count + 1
            } else {
                count.saturating_sub(1)
            };
            if self.game.held(place, wanted).is_some() {
                self.chosen = Some((place, wanted));
                return Outcome::Redraw;
            }
            return Outcome::Unchanged;
        }
        let (col, top) = column(at);
        let along = |col: usize, by: isize| -> Option<usize> {
            let mut c = col;
            loop {
                c = c.checked_add_signed(by).filter(|c| *c < PILES)?;
                if !top || ROW[c].is_some() {
                    return Some(c);
                }
            }
        };
        let next = match key {
            Key::Left => along(col, -1).map(|c| (c, top)),
            Key::Right => along(col, 1).map(|c| (c, top)),
            Key::Up if !top => Some((if col == 2 { 1 } else { col }, true)),
            Key::Down if top => Some((col, false)),
            _ => None,
        };
        let Some((col, top)) = next else {
            return Outcome::Unchanged;
        };
        self.focus = if top {
            ROW[col]
        } else {
            Some(Place::Tableau(col))
        };
        Outcome::Redraw
    }

    /// Enter: turn the stock, pick up, or put down.
    fn act(&mut self) -> Outcome {
        let Some(at) = self.focus else {
            return self.step(Key::Down);
        };
        match self.chosen.take() {
            Some((from, _)) if from == at => {}
            Some((from, count)) => {
                if self.game.play(from, count, at).is_none() {
                    self.chosen = Some((from, count));
                    return Outcome::Unchanged;
                }
            }
            None if at == Place::Stock => {
                self.game.deal();
            }
            None => {
                if self.game.held(at, 1).is_none() {
                    return Outcome::Unchanged;
                }
                self.chosen = Some((at, 1));
            }
        }
        Outcome::Redraw
    }
}

impl App for Solitaire {
    type Event = ();

    fn title(&self) -> String {
        "Solitaire".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        (1100, 760)
    }

    fn resize(&mut self, size: Size, scale: u32) {
        self.layout = Layout::new(&self.appearance, size, scale);
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        let scene = self.scene();
        view::paint(
            frame,
            &self.layout,
            &self.appearance,
            &mut self.fonts,
            &mut self.faces,
            &self.game,
            &scene,
        );
    }

    fn key(&mut self, key: Key, mods: Mods) -> Outcome {
        match key {
            Key::Left | Key::Right | Key::Up | Key::Down => self.step(key),
            Key::Enter => self.act(),
            Key::Space => {
                self.chosen = None;
                if self.game.deal() {
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Escape if self.chosen.take().is_some() => Outcome::Redraw,
            Key::Backspace => self.undo(),
            Key::Chord('z') if mods.ctrl => self.undo(),
            Key::Chord('n') if mods.ctrl => self.button(Button::New),
            Key::Chord('q' | 'w') if mods.ctrl => Outcome::Close,
            _ => Outcome::Unchanged,
        }
    }

    fn text(&mut self, ch: char) -> Outcome {
        match ch.to_ascii_lowercase() {
            'n' => self.button(Button::New),
            'u' => self.undo(),
            // Home: the card under the keyboard, to its foundation.
            'h' => {
                let focus = self.focus;
                if focus.is_some_and(|at| self.send_home(at)) {
                    self.chosen = None;
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            _ => Outcome::Unchanged,
        }
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let before = self.scene().hover;
        self.pointer = at;
        if self.drag.is_some() || self.scene().hover != before {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    fn press(&mut self, at: Point) -> Outcome {
        self.pointer = Some(at);
        self.focus = None;
        self.chosen = None;
        let Some(hit) = self.layout.hit(&self.game, at) else {
            self.pressed = None;
            return Outcome::Redraw;
        };
        let again =
            matches!(self.pressed, Some((was, when)) if was == hit && when.elapsed() < DOUBLE);
        self.pressed = Some((hit, Instant::now()));
        match hit {
            Hit::Button(button) => return self.button(button),
            Hit::Stock => {
                self.game.deal();
                return Outcome::Redraw;
            }
            _ => {}
        }
        let Some((place, count)) = self.picked(hit) else {
            return Outcome::Redraw;
        };
        if again && count == 1 && self.send_home(place) {
            self.pressed = None;
            return Outcome::Redraw;
        }
        let first = self.layout.top(&self.game, place, count);
        self.flight = None;
        self.drag = Some(Drag {
            place,
            count,
            grip: (at.x - first.x, at.y - first.y),
        });
        Outcome::Redraw
    }

    fn release(&mut self, at: Point) -> Outcome {
        self.pointer = Some(at);
        let Some(drag) = self.drag.take() else {
            return Outcome::Unchanged;
        };
        // Where the first card in the hand is, near its top: what it covers.
        let (w, h) = self.layout.card;
        let over = Point::new(at.x - drag.grip.0 + w / 2, at.y - drag.grip.1 + h / 4);
        if let Some(to) = self.layout.target(over) {
            self.game.play(drag.place, drag.count, to);
        }
        Outcome::Redraw
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Unchanged
    }

    fn cursor(&self, at: Point) -> Cursor {
        match self.layout.hit(&self.game, at) {
            Some(Hit::Button(_) | Hit::Stock) => Cursor::Pointer,
            Some(hit) if self.picked(hit).is_some() => Cursor::Pointer,
            _ => Cursor::Default,
        }
    }

    fn animating(&self) -> bool {
        self.flight.is_some() || (self.drag.is_none() && self.game.runs_out())
    }

    fn tick(&mut self) -> Outcome {
        if self
            .flight
            .as_ref()
            .is_some_and(|f| f.since.elapsed() >= FLIGHT)
        {
            self.flight = None;
        }
        // Nothing hidden and nothing left to turn: the rest plays itself.
        if self.flight.is_none()
            && self.drag.is_none()
            && self.game.runs_out()
            && let Some(from) = self.game.next_home()
        {
            self.send_home(from);
        }
        Outcome::Redraw
    }

    fn event(&mut self, (): ()) -> Outcome {
        Outcome::Unchanged
    }
}

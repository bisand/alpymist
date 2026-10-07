//! The game in a window: the hand that drags, the double click, the
//! keyboard, a card's flight home, the clock, and what is asked over the
//! table.

use alpymist_solitaire::cards::Rng;
use alpymist_solitaire::faces::{BACKS, Faces};
use alpymist_solitaire::game::{Game, PILES, Place};
use alpymist_solitaire::kept::{Kept, Score, clock, initials};
use alpymist_solitaire::view::{self, Button, Hit, Layout, Lifted, Panel, Scene};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host;
use alpymist_widget::window::{self, App, Cursor, Key, Mods};
use alpymist_widget::{Appearance, Outcome};
use denise::Frame;
use denise::geom::{Point, Size};
use std::path::PathBuf;
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

/// What is asked or shown over the table, until it is answered.
enum Over {
    /// Whether to give this game up for one that turns so many cards.
    Ask(usize),
    /// The best games.
    Best,
    /// Whose record a game just won is.
    Record {
        seconds: u32,
        moves: u32,
        typed: String,
    },
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
    /// The back, the way of turning and the best games, as on disk.
    kept: Kept,
    /// Which back the cards wear.
    back: usize,
    /// When the first move of this game was made.
    began: Option<Instant>,
    /// How long it took, once it is won.
    took: Option<u32>,
    /// The second the bar last showed.
    shown: Option<u32>,
    over: Option<Over>,
}

/// The account's state directory.
fn state() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))
        .map(|dir| dir.join("alpymist"))
}

/// What was kept at the last closing. The first release kept the back alone,
/// in a file of its own, which is read where the newer one is not there.
fn recall() -> Kept {
    let Some(dir) = state() else {
        return Kept::default();
    };
    if let Ok(text) = std::fs::read_to_string(dir.join("solitaire")) {
        return Kept::parse(&text);
    }
    let back = std::fs::read_to_string(dir.join("solitaire-back")).unwrap_or_default();
    Kept::parse(&format!("back={}", back.trim()))
}

/// Open the window and play until it closes. `turn` is how many cards the
/// stock turns when the command line says; otherwise it is as last time.
/// `nearly_out` sets out a game five cards from its end and not a deal.
pub fn run(turn: Option<usize>, nearly_out: bool) -> Result<(), String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    for p in &fonts.problems {
        eprintln!("{NAME}: font {p}");
    }
    let mut kept = recall();
    if let Some(turn) = turn {
        kept.turn = turn;
    }
    let back = BACKS
        .iter()
        .position(|(name, _)| *name == kept.back)
        .unwrap_or(0);
    // No two games alike, and nothing to keep secret: the clock will do.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut seeds = Rng::new(u64::try_from(now & u128::from(u64::MAX)).unwrap_or(0));
    let layout = Layout::new(&appearance, Size::new(1100, 760), 1);
    let game = if nearly_out {
        nearly(kept.turn)
    } else {
        Game::new(seeds.number(), kept.turn)
    };
    let app = Solitaire {
        game,
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
        kept,
        back,
        began: None,
        took: None,
        shown: None,
        over: None,
    };
    let options = window::Options {
        app_id: NAME.into(),
        min_size: (760, 480),
        max_size: None,
    };
    let (_sender, events) = host::events::<()>();
    window::run(app, &options, events, None)
}

/// A game with two kings, two queens and a jack left to go home.
fn nearly(turn: usize) -> Game {
    use alpymist_solitaire::cards::{Card, KING, Suit};
    use alpymist_solitaire::game::Pile;
    let card = |suit, rank| Card { suit, rank };
    let mut piles: [Pile; PILES] = Default::default();
    piles[0].up = vec![card(Suit::Spades, KING), card(Suit::Hearts, 12)];
    piles[1].up = vec![card(Suit::Hearts, KING), card(Suit::Spades, 12)];
    let waste = vec![card(Suit::Hearts, 11)];
    Game::set_out(piles, Vec::new(), waste, [KING, KING, 10, 11], turn)
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

/// A line of the best games: its place and whose, and how it went.
fn line(place: usize, who: &str, seconds: u32, moves: u32) -> (String, String) {
    let who = if who.is_empty() { "---" } else { who };
    (
        format!("{}.  {who}", place + 1),
        format!("{}  ·  {moves} moves", clock(seconds)),
    )
}

fn turning(turn: usize) -> String {
    if turn == 3 {
        "Turning three cards".into()
    } else {
        "Turning one card".into()
    }
}

impl Solitaire {
    /// Write what is kept. A choice that cannot be written still holds
    /// until the window closes.
    fn keep(&self) {
        let Some(dir) = state() else {
            return;
        };
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join("solitaire"), self.kept.text()).ok();
    }

    fn deal_again(&mut self, turn: usize) {
        self.game = Game::new(self.seeds.number(), turn);
        self.drag = None;
        self.flight = None;
        self.chosen = None;
        self.began = None;
        self.took = None;
        self.shown = None;
        if self.kept.turn != self.game.turn() {
            self.kept.turn = self.game.turn();
            self.keep();
        }
    }

    /// Deal again, asking first if there is a game under way to lose.
    fn ask_new(&mut self, turn: usize) {
        if self.game.moves() > 0 && !self.game.won() {
            self.over = Some(Over::Ask(turn));
        } else {
            self.deal_again(turn);
        }
    }

    fn undo(&mut self) -> Outcome {
        // A game that is out is over: its time is taken.
        if self.game.won() {
            return Outcome::Unchanged;
        }
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
            Button::New => self.ask_new(self.game.turn()),
            Button::Undo => return self.undo(),
            Button::Turn => self.ask_new(if self.game.turn() == 3 { 1 } else { 3 }),
            Button::Back => self.wear(1),
            Button::Best => self.over = Some(Over::Best),
        }
        Outcome::Redraw
    }

    /// Wear the back `by` after this one, round and round.
    fn wear(&mut self, by: usize) {
        self.back = (self.back + by) % BACKS.len();
        BACKS[self.back].0.clone_into(&mut self.kept.back);
        self.keep();
    }

    /// How long this game has been played, in whole seconds.
    fn seconds(&self) -> Option<u32> {
        self.took.or_else(|| {
            self.began
                .map(|at| u32::try_from(at.elapsed().as_secs()).unwrap_or(u32::MAX))
        })
    }

    /// After anything that may have moved a card: start the clock at the
    /// first move, and stop it when the last card is home.
    fn settle(&mut self) {
        if self.began.is_none() && self.game.moves() > 0 {
            self.began = Some(Instant::now());
        }
        if self.game.won() && self.took.is_none() {
            let seconds = self.seconds().unwrap_or(0);
            let moves = self.game.moves();
            self.took = Some(seconds);
            if self.kept.place(self.game.turn(), seconds, moves).is_some() {
                self.over = Some(Over::Record {
                    seconds,
                    moves,
                    typed: String::new(),
                });
            }
        }
    }

    /// What is said over the table now.
    fn panel(&self) -> Option<Panel> {
        let table = |turn: usize, lines: &mut Vec<(String, String)>| {
            lines.push((turning(turn), String::new()));
            let best = self.kept.best(turn);
            if best.is_empty() {
                lines.push(("    No game won yet".into(), String::new()));
            }
            for (i, s) in best.iter().enumerate() {
                lines.push(line(i, &s.initials, s.seconds, s.moves));
            }
        };
        Some(match self.over.as_ref()? {
            Over::Ask(_) => Panel {
                title: "Deal again?".into(),
                lines: vec![("This game will be lost.".into(), String::new())],
                buttons: vec!["New game".into(), "Keep playing".into()],
            },
            Over::Best => {
                let mut lines = Vec::new();
                table(1, &mut lines);
                table(3, &mut lines);
                Panel {
                    title: "Best games".into(),
                    lines,
                    buttons: vec!["Close".into()],
                }
            }
            Over::Record {
                seconds,
                moves,
                typed,
            } => {
                let turn = self.game.turn();
                let place = self.kept.place(turn, *seconds, *moves).unwrap_or(0);
                let mut lines = vec![(turning(turn), String::new())];
                let best = self.kept.best(turn);
                for (i, s) in best.iter().take(place).enumerate() {
                    lines.push(line(i, &s.initials, s.seconds, s.moves));
                }
                lines.push(line(place, &format!("{typed}_"), *seconds, *moves));
                for (i, s) in best.iter().enumerate().skip(place).take(4 - place.min(4)) {
                    lines.push(line(i + 1, &s.initials, s.seconds, s.moves));
                }
                lines.push(("Type your initials, then Enter".into(), String::new()));
                Panel {
                    title: "A new record".into(),
                    lines,
                    buttons: vec!["Save".into()],
                }
            }
        })
    }

    /// A panel's button was pressed: the first is the one Enter means.
    fn answer(&mut self, button: usize) -> Outcome {
        match self.over.take() {
            Some(Over::Ask(turn)) if button == 0 => self.deal_again(turn),
            Some(Over::Record {
                seconds,
                moves,
                typed,
            }) => {
                let score = Score {
                    seconds,
                    moves,
                    initials: initials(&typed),
                };
                self.kept.record(self.game.turn(), score);
                self.keep();
                self.over = Some(Over::Best);
            }
            _ => {}
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
        let panel = self.panel();
        let hover = match (&panel, self.pointer) {
            (None, Some(at)) => match self.layout.hit(&self.game, at) {
                Some(Hit::Button(b)) => Some(b),
                _ => None,
            },
            _ => None,
        };
        let answer = panel
            .as_ref()
            .zip(self.pointer)
            .and_then(|(panel, at)| self.layout.answer(panel, at));
        Scene {
            lifted,
            hover,
            focus: self.focus,
            chosen: self.chosen,
            back: self.back,
            seconds: self.seconds(),
            answer,
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

    /// A key while something is asked over the table.
    fn key_over(&mut self, key: Key, mods: Mods) -> Outcome {
        match key {
            Key::Enter => self.answer(0),
            // Escape is the answer that loses nothing: keep playing, close,
            // or a record saved without a name.
            Key::Escape => self.answer(1),
            Key::Backspace => {
                if let Some(Over::Record { typed, .. }) = &mut self.over
                    && typed.pop().is_some()
                {
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Chord('q' | 'w') if mods.ctrl => Outcome::Close,
            _ => Outcome::Unchanged,
        }
    }

    fn key_table(&mut self, key: Key, mods: Mods) -> Outcome {
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

    fn text_over(&mut self, ch: char) -> Outcome {
        match &mut self.over {
            Some(Over::Record { typed, .. }) => {
                if ch.is_ascii_alphanumeric() && typed.len() < 3 {
                    typed.push(ch.to_ascii_uppercase());
                    return Outcome::Redraw;
                }
                Outcome::Unchanged
            }
            Some(Over::Ask(_)) => match ch.to_ascii_lowercase() {
                'y' => self.answer(0),
                'n' => self.answer(1),
                _ => Outcome::Unchanged,
            },
            _ => Outcome::Unchanged,
        }
    }

    fn text_table(&mut self, ch: char) -> Outcome {
        // B for the next back, Shift+B for the one before.
        if ch == 'B' {
            self.wear(BACKS.len() - 1);
            return Outcome::Redraw;
        }
        match ch.to_ascii_lowercase() {
            'b' => self.button(Button::Back),
            'n' => self.button(Button::New),
            's' => self.button(Button::Best),
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

    fn press_table(&mut self, at: Point) -> Outcome {
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
        let panel = self.panel();
        self.shown = scene.seconds;
        view::paint(
            frame,
            &self.layout,
            &self.appearance,
            &mut self.fonts,
            &mut self.faces,
            &self.game,
            &scene,
            panel.as_ref(),
        );
    }

    fn key(&mut self, key: Key, mods: Mods) -> Outcome {
        let outcome = if self.over.is_some() {
            self.key_over(key, mods)
        } else {
            self.key_table(key, mods)
        };
        self.settle();
        outcome
    }

    fn text(&mut self, ch: char) -> Outcome {
        let outcome = if self.over.is_some() {
            self.text_over(ch)
        } else {
            self.text_table(ch)
        };
        self.settle();
        outcome
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let before = self.scene();
        self.pointer = at;
        let after = self.scene();
        if self.drag.is_some() || after.hover != before.hover || after.answer != before.answer {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    fn press(&mut self, at: Point) -> Outcome {
        self.pointer = Some(at);
        let outcome = match self.panel() {
            Some(panel) => match self.layout.answer(&panel, at) {
                Some(button) => self.answer(button),
                None => Outcome::Unchanged,
            },
            None => self.press_table(at),
        };
        self.settle();
        outcome
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
        self.settle();
        Outcome::Redraw
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Unchanged
    }

    fn cursor(&self, at: Point) -> Cursor {
        if let Some(panel) = self.panel() {
            return match self.layout.answer(&panel, at) {
                Some(_) => Cursor::Pointer,
                None => Cursor::Default,
            };
        }
        match self.layout.hit(&self.game, at) {
            Some(Hit::Button(_) | Hit::Stock) => Cursor::Pointer,
            Some(hit) if self.picked(hit).is_some() => Cursor::Pointer,
            _ => Cursor::Default,
        }
    }

    fn animating(&self) -> bool {
        self.flight.is_some()
            || (self.drag.is_none() && self.game.runs_out())
            // The clock, while it runs.
            || (self.began.is_some() && self.took.is_none())
    }

    fn tick(&mut self) -> Outcome {
        let flying = self.flight.is_some();
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
            && self.over.is_none()
            && self.game.runs_out()
            && let Some(from) = self.game.next_home()
        {
            self.send_home(from);
        }
        self.settle();
        // A frame for a card in the air, or for the clock's next second:
        // the window is not painted twenty-five times a second to show one.
        if flying || self.flight.is_some() || self.seconds() != self.shown {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    fn event(&mut self, (): ()) -> Outcome {
        Outcome::Unchanged
    }
}

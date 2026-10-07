//! `alpymist-solitaire-console` — Klondike on a screen with no desktop.
//!
//! The same game as `alpymist-solitaire`, shown without a compositor: the
//! picture goes straight to the display through DRM/KMS, and the mouse, a
//! touch screen and the keyboard are read from the kernel's input devices.
//! For a text console, a machine with no desktop installed, a Raspberry Pi.
//!
//! ```text
//! alpymist-solitaire-console           deal a game, turning as last time
//! alpymist-solitaire-console --three   deal one that turns three cards
//! ```
//!
//! It needs what any program drawing to the screen itself needs: to be run
//! from a text console, by someone who may open the display and the input
//! devices. That is root, or an account in the `video` and `input` groups.

#![forbid(unsafe_code)]

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-solitaire-console [--one | --three]

Deals a game of Klondike on this screen, with no desktop.
  --one      turn one card from the stock at a time
  --three    turn three
Without either it turns as many as the last game did.

Run it from a text console, as root or as an account in the video and
input groups. The bar's Quit, Ctrl+Q or Ctrl+C leaves, asking first if a
game is under way. Everything else is as in the window:
drag the cards or double-click one home; the arrows and Enter; Space turns
the stock, U takes a move back, N deals again, B changes the cards' backs,
S shows the best games.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (turn, nearly_out) = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => (None, false),
        ["--one"] => (Some(1), false),
        ["--three"] => (Some(3), false),
        // Not in the usage: a game five cards from out, to try how one ends.
        ["--nearly-out"] => (None, true),
        // Nor this: what a frame costs on this machine, with no screen.
        ["--bench"] => {
            frame::bench();
            return ExitCode::SUCCESS;
        }
        ["-V" | "--version"] => {
            println!("alpymist-solitaire-console {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        ["-h" | "--help"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(turn, nearly_out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-solitaire-console: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(feature = "drm"))]
fn run(_turn: Option<usize>, _nearly_out: bool) -> Result<(), String> {
    Err("built without a screen to draw on; build with --features drm".into())
}

#[cfg(feature = "drm")]
fn run(turn: Option<usize>, nearly_out: bool) -> Result<(), String> {
    screen::run(turn, nearly_out)
}

/// A frame: the table, kept from one to the next, and over it whatever
/// moves.
///
/// A table is fifty-two pictures and takes its time; a pointer crossing it
/// changes a few hundred pixels. So the table is painted into memory of its
/// own, only when it looks different, and a frame is that memory copied
/// where something moved, with the cards in the hand over it. What Denise's
/// own examples do with a tree's damage, done by hand for a game.
mod frame {
    use alpymist_solitaire::kept::Kept;
    use alpymist_solitaire::play::{Change, Look, Play};
    use denise::geom::{Point, Rect, Size};
    use denise::painter::Pen;
    use denise::{PixelFormat, PixelView};
    use denise_render::Canvas;
    use std::time::Instant;

    /// The table as last painted.
    pub struct Table {
        pixels: Vec<u32>,
        size: Size,
        look: Option<Look>,
    }

    impl Table {
        pub fn new(size: Size) -> Self {
            let len = usize::try_from(u64::from(size.width) * u64::from(size.height)).unwrap_or(0);
            Self {
                pixels: vec![0; len],
                size,
                look: None,
            }
        }

        /// Paint what of the table looks different. Returns the one
        /// rectangle all of it is in, the whole table for all of it, and
        /// `None` when nothing was.
        pub fn refresh(&mut self, game: &mut Play) -> Option<Rect> {
            let (look, change) = game.since(self.look.as_ref());
            let whole = Rect::from_size(self.size);
            let parts = match change {
                Change::Nothing => return None,
                Change::Within(parts) => parts,
                Change::Everything => Vec::new(),
            };
            if let Some(mut canvas) = Canvas::from_pixels(
                &mut self.pixels,
                self.size,
                self.size.width,
                PixelFormat::Argb8888,
            ) {
                if parts.is_empty() {
                    game.paint_table_on(&mut canvas, None);
                }
                // Each part by itself: two piles at either end of the table
                // are two columns, not everything between them.
                for part in &parts {
                    game.paint_table_on(&mut canvas, Some(*part));
                }
            }
            self.look = Some(look);
            Some(
                parts
                    .into_iter()
                    .reduce(|all, part| all.union(&part))
                    .unwrap_or(whole),
            )
        }

        /// Put the table's `area` on `canvas`, and the cards in the hand
        /// over it.
        pub fn compose(&self, canvas: &mut Canvas<'_>, area: Rect, game: &mut Play) {
            if let Some(view) = PixelView::new(&self.pixels, self.size, self.size.width) {
                let mut pen = Pen::new(canvas);
                pen.with_clip(area).blit(&view, Point::new(0, 0));
            }
            game.paint_status_on(canvas);
            game.paint_hand_on(canvas);
        }
    }

    /// The smallest rectangle holding all of `parts`.
    pub fn around(parts: [Option<Rect>; 4]) -> Option<Rect> {
        parts
            .into_iter()
            .flatten()
            .reduce(|all, part| all.union(&part))
    }

    fn median(mut times: Vec<f64>) -> f64 {
        times.sort_by(f64::total_cmp);
        times.get(times.len() / 2).copied().unwrap_or(0.0)
    }

    /// Print what a frame costs here: a table painted whole, and a card
    /// carried across it.
    #[allow(clippy::too_many_lines)] // one measurement after another
    pub fn bench() {
        let size = Size::new(1920, 1080);
        let mut game = Play::new(
            alpymist_widget::Appearance::default(),
            Kept::default(),
            None,
            1,
            false,
        )
        .with_quit();
        game.resize(size, 1);
        let mut table = Table::new(size);
        let mut screen = vec![0u32; table.pixels.len()];

        let started = Instant::now();
        table.refresh(&mut game);
        let first = started.elapsed().as_secs_f64() * 1000.0;
        let whole: Vec<f64> = (0..10)
            .map(|_| {
                table.look = None;
                let started = Instant::now();
                table.refresh(&mut game);
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();

        // The last pile's top card, picked up and carried.
        let card = game
            .layout()
            .top(game.game(), alpymist_solitaire::game::Place::Tableau(6), 1);
        game.press(Point::new(card.x + 20, card.y + 20));
        table.refresh(&mut game);
        let mut last = game.hand_bounds();
        let carried: Vec<f64> = (0..200)
            .map(|i| {
                let started = Instant::now();
                game.pointer(Some(Point::new(200 + i * 7, 300 + i * 3)));
                let change = table.refresh(&mut game);
                let now = game.hand_bounds();
                if let (Some(area), Some(mut canvas)) = (
                    around([last, now, None, None]),
                    Canvas::from_pixels(&mut screen, size, size.width, PixelFormat::Argb8888),
                ) {
                    table.compose(&mut canvas, area, &mut game);
                }
                last = now;
                assert!(change.is_none(), "a hand moving repainted the table");
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();

        // Let go, and run the pointer along the bar's buttons and back.
        game.release(Point::new(-50, -50));
        table.refresh(&mut game);
        let spots: Vec<Point> = game
            .layout()
            .buttons
            .iter()
            .map(|(_, r)| Point::new(r.x + r.width / 2, r.y + r.height / 2))
            .collect();
        let lit: Vec<f64> = (0..100)
            .map(|i| {
                let started = Instant::now();
                game.pointer(Some(spots[i % spots.len()]));
                let change = table.refresh(&mut game);
                if let (Some(area), Some(mut canvas)) = (
                    change,
                    Canvas::from_pixels(&mut screen, size, size.width, PixelFormat::Argb8888),
                ) {
                    table.compose(&mut canvas, area, &mut game);
                }
                assert!(
                    change.is_none_or(|area| area.height < 200),
                    "a button lit repainted the table"
                );
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();

        // The stock turned, card after card: the stock and the waste.
        game.pointer(None);
        table.refresh(&mut game);
        let turned: Vec<f64> = (0..20)
            .map(|_| {
                let started = Instant::now();
                game.key(alpymist_solitaire::play::Key::Space);
                let change = table.refresh(&mut game);
                if let (Some(area), Some(mut canvas)) = (
                    change,
                    Canvas::from_pixels(&mut screen, size, size.width, PixelFormat::Argb8888),
                ) {
                    table.compose(&mut canvas, area, &mut game);
                }
                assert!(
                    change.is_some_and(|area| area.height < 800),
                    "turning the stock repainted the table"
                );
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();

        println!("alpymist-solitaire-console --bench, at 1920x1080, in memory");
        println!("  the first table, faces scaled   {first:8.2} ms");
        println!("  a table painted whole, median   {:8.2} ms", median(whole));
        println!(
            "  a card carried, median          {:8.2} ms",
            median(carried)
        );
        println!("  a button lit, median            {:8.2} ms", median(lit));
        println!(
            "  the stock turned, median        {:8.2} ms",
            median(turned)
        );
    }

    #[cfg(test)]
    mod tests {
        use super::around;
        use denise::geom::Rect;

        #[test]
        fn what_moved_is_one_rectangle_around_all_of_it() {
            assert_eq!(around([None, None, None, None]), None);
            let a = Rect::new(10, 10, 5, 5);
            let b = Rect::new(100, 50, 10, 10);
            assert_eq!(around([Some(a), None, None, None]), Some(a));
            assert_eq!(
                around([Some(a), None, Some(b), None]),
                Some(Rect::new(10, 10, 100, 50))
            );
        }
    }
}

/// A face to write with, on a system that is not Alpymist.
#[cfg(feature = "drm")]
mod face {
    /// Where other systems keep a plain sans-serif: Debian and Raspberry Pi
    /// OS first, then Alpine, Arch and Fedora. None of them is carried in
    /// the program; where none is there, text is Denise's built-in bitmap.
    const ELSEWHERE: [&str; 8] = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    ];

    /// The face to use: `wanted` where it `exists`, else the first of the
    /// others that does, else `wanted` all the same, to be reported missing.
    pub fn choose(wanted: &str, exists: impl Fn(&str) -> bool) -> String {
        if exists(wanted) {
            return wanted.to_owned();
        }
        ELSEWHERE
            .into_iter()
            .find(|path| exists(path))
            .unwrap_or(wanted)
            .to_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::{ELSEWHERE, choose};

        #[test]
        fn the_theme_s_face_where_it_is_and_another_system_s_where_it_is_not() {
            let ours = "/usr/share/fonts/fira/FiraSans-Regular.ttf";
            assert_eq!(choose(ours, |_| true), ours);
            assert_eq!(choose(ours, |p| p == ELSEWHERE[2]), ELSEWHERE[2]);
            // The first that is there, in the order they are tried.
            assert_eq!(
                choose(ours, |p| p == ELSEWHERE[4] || p == ELSEWHERE[1]),
                ELSEWHERE[1]
            );
            assert_eq!(choose(ours, |_| false), ours);
        }
    }
}

/// The keys a game takes, from the kernel's.
#[cfg(feature = "drm")]
mod keys {
    use alpymist_solitaire::play::Key;
    use denise::input::{KeyCode, Modifiers};

    /// The game's key for a key pressed with `held`, if it has one.
    pub fn key(code: KeyCode, held: Modifiers) -> Option<Key> {
        let ctrl = held.contains(Modifiers::CTRL);
        Some(match code {
            KeyCode::ArrowLeft => Key::Left,
            KeyCode::ArrowRight => Key::Right,
            KeyCode::ArrowUp => Key::Up,
            KeyCode::ArrowDown => Key::Down,
            KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
            KeyCode::Space => Key::Space,
            KeyCode::Escape => Key::Escape,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Z if ctrl => Key::Undo,
            KeyCode::N if ctrl => Key::New,
            // Ctrl+C as well: what a hand tries first at a console.
            KeyCode::Q | KeyCode::C if ctrl => Key::Quit,
            _ => return None,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::key;
        use alpymist_solitaire::play::Key;
        use denise::input::{KeyCode, Modifiers};

        #[test]
        fn the_arrows_enter_and_the_chords_are_the_game_s() {
            assert_eq!(key(KeyCode::ArrowLeft, Modifiers::NONE), Some(Key::Left));
            assert_eq!(key(KeyCode::NumpadEnter, Modifiers::NONE), Some(Key::Enter));
            assert_eq!(key(KeyCode::Z, Modifiers::CTRL), Some(Key::Undo));
            assert_eq!(key(KeyCode::Q, Modifiers::CTRL), Some(Key::Quit));
            assert_eq!(key(KeyCode::C, Modifiers::CTRL), Some(Key::Quit));
            assert_eq!(key(KeyCode::C, Modifiers::NONE), None);
            // A letter alone is text, which comes by itself.
            assert_eq!(key(KeyCode::Q, Modifiers::NONE), None);
            assert_eq!(key(KeyCode::Z, Modifiers::NONE), None);
        }
    }
}

/// On the machine: KMS for the picture, evdev for the hand.
#[cfg(feature = "drm")]
mod screen {
    use super::frame::{Table, around};
    use super::keys;
    use alpymist_solitaire::kept::Kept;
    use alpymist_solitaire::play::{self, Outcome, Play};
    use alpymist_ui::display::{self, Screen};
    use alpymist_ui::render::{new_cursor, paint_cursor};
    use denise::geom::Rect;
    use denise::input::{ElementState, PointerButton};
    use denise::painter::Pen;
    use denise::{InputEvent, InputSource};
    use denise_evdev::{Console, InputBackend};
    use denise_ui::cursor::Cursor;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// How long to rest when there is nothing to do: short, since what is
    /// waited for is a hand, and a pointer that answers late is what a
    /// choppy one is.
    const REST: Duration = Duration::from_millis(3);

    pub fn run(turn: Option<usize>, nearly_out: bool) -> Result<(), String> {
        // Graphics mode stops the text console drawing over the table, and a
        // muted keyboard stops it from also taking what is typed as a
        // command. Both are given back whatever happens after.
        let mut console = Console::open_if_present();
        if let Some(console) = console.as_mut() {
            console
                .graphics_mode()
                .and_then(|()| console.mute_keyboard())
                .map_err(|e| format!("this console could not be taken: {e}"))?;
        }
        let result = play(turn, nearly_out);
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        result
    }

    /// What a move of the hand does to the game, and to the arrow shown.
    fn hand(event: &InputEvent, game: &mut Play, cursor: &mut Cursor) -> Outcome {
        match *event {
            InputEvent::PointerMoved { position } => {
                cursor.position = position;
                cursor.visible = true;
                game.pointer(Some(position));
                // The pointer itself is part of the picture here.
                Outcome::Redraw
            }
            InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position,
                ..
            } => match state {
                ElementState::Down => game.press(position),
                ElementState::Up => game.release(position),
            },
            // A finger is a pointer with no arrow to show.
            InputEvent::TouchDown { position, .. } => {
                cursor.visible = false;
                game.pointer(Some(position));
                game.press(position)
            }
            InputEvent::TouchMoved { position, .. } => game.pointer(Some(position)),
            InputEvent::TouchUp { position, .. } => {
                let outcome = game.release(position);
                game.pointer(None);
                outcome
            }
            InputEvent::Key {
                code,
                state: ElementState::Down,
                modifiers,
                ..
            } => keys::key(code, modifiers).map_or(Outcome::Unchanged, |key| game.key(key)),
            InputEvent::Text { ch } => game.text(ch),
            _ => Outcome::Unchanged,
        }
    }

    /// The game to play: dealt as the last one was, or as the command line
    /// says, and written in a face this system has.
    fn deal(turn: Option<usize>, nearly_out: bool) -> Play {
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
        let mut appearance = alpymist_widget::appearance();
        appearance.font = super::face::choose(&appearance.font, |path| {
            std::path::Path::new(path).is_file()
        });
        let game = Play::new(appearance, kept, dir, seed, nearly_out).with_quit();
        for p in game.font_problems() {
            eprintln!("font {p}");
        }
        game
    }

    fn play(turn: Option<usize>, nearly_out: bool) -> Result<(), String> {
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [SIGTERM, SIGINT, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&stop))
                .map_err(|e| format!("could not listen for signals: {e}"))?;
        }

        let mut screen = Screen::open(display::config()).map_err(|e| {
            format!(
                "the display could not be opened: {e}\n\
                 Run it from a text console, as root or in the video group."
            )
        })?;
        let size = screen.size();
        // A screen of very many pixels is a small one seen close: draw
        // everything twice the size on it.
        let scale = if size.height >= 1800 { 2 } else { 1 };
        eprintln!("display: {}x{} via DRM/KMS", size.width, size.height);

        let mut game = deal(turn, nearly_out);
        game.resize(size, scale);

        let mut input: Option<InputBackend> = None;
        let mut next_look = Instant::now();
        let mut events: Vec<InputEvent> = Vec::new();
        let mut cursor = new_cursor();
        let mut dirty = true;
        let mut table = Table::new(size);
        // Where the hand and the arrow were in the frame on the screen, and
        // what the bar said in it.
        let mut shown: [Option<Rect>; 2] = [None, None];
        let mut said = String::new();

        loop {
            // A keyboard or a mouse that turns up a moment late is ordinary;
            // keep looking until there is one.
            if input.is_none() && Instant::now() >= next_look {
                input = InputBackend::open_all(size).ok();
                if let Some(backend) = input.as_mut() {
                    let (layout, source) = backend.set_layout_from_system();
                    eprintln!("keyboard: {} (from {source})", layout.name);
                } else {
                    eprintln!(
                        "no keyboard or mouse can be read yet; \
                         as root or in the input group there would be"
                    );
                }
                next_look = Instant::now() + Duration::from_secs(2);
            }

            events.clear();
            if let Some(backend) = input.as_mut() {
                backend.poll(&mut events);
            }
            for event in &events {
                let outcome = hand(event, &mut game, &mut cursor);
                match outcome {
                    Outcome::Close => return Ok(()),
                    Outcome::Redraw => dirty = true,
                    Outcome::Unchanged => {}
                }
            }
            if game.animating() && game.tick() == Outcome::Redraw {
                dirty = true;
            }

            if dirty {
                // The table only when it looks different; otherwise just
                // where the hand and the arrow were and are.
                let repainted = table.refresh(&mut game);
                let now = [game.hand_bounds(), cursor.visible.then(|| cursor.bounds())];
                // The clock's words, where they changed: a second passing
                // is a strip of the bar, not a table.
                let ticked = game.status().filter(|(words, _)| *words != said);
                let moved = around([shown[0], shown[1], now[0], now[1]]);
                let said_at = ticked.as_ref().map(|(_, at)| *at);
                let area = around([moved, said_at, repainted, None]);
                if let Some((words, _)) = ticked {
                    said = words;
                }
                if let Some(area) = area {
                    screen
                        .present_area(area, |canvas| {
                            table.compose(canvas, area, &mut game);
                            let mut pen = Pen::new(canvas);
                            paint_cursor(&mut pen, &cursor, &denise::theme::DARK);
                        })
                        .map_err(|e| format!("the display stopped taking pictures: {e}"))?;
                }
                shown = now;
                dirty = false;
            } else {
                std::thread::sleep(REST);
            }
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::hand;
        use alpymist_solitaire::kept::Kept;
        use alpymist_solitaire::play::{Outcome, Play};
        use alpymist_ui::render::new_cursor;
        use denise::InputEvent;
        use denise::geom::Point;
        use denise::input::{ElementState, KeyCode, Modifiers, PointerButton};

        fn game() -> Play {
            Play::new(
                alpymist_widget::Appearance::default(),
                Kept::default(),
                None,
                7,
                false,
            )
            .with_quit()
        }

        fn key(code: KeyCode, modifiers: Modifiers) -> InputEvent {
            InputEvent::Key {
                code,
                state: ElementState::Down,
                repeat: false,
                modifiers,
            }
        }

        /// What the kernel sends for each way out ends the game: the reason
        /// this test is here is a console nobody could leave.
        #[test]
        fn every_way_out_is_a_way_out() {
            let ways = [
                key(KeyCode::Q, Modifiers::CTRL),
                key(KeyCode::C, Modifiers::CTRL),
            ];
            for way in ways {
                let mut cursor = new_cursor();
                assert_eq!(
                    hand(&way, &mut game(), &mut cursor),
                    Outcome::Close,
                    "{way:?}"
                );
            }
            // Escape and a letter by itself are the game's, not ways out.
            for not in [
                key(KeyCode::Escape, Modifiers::NONE),
                key(KeyCode::Q, Modifiers::NONE),
                key(KeyCode::W, Modifiers::CTRL),
                InputEvent::Text { ch: 'q' },
            ] {
                let mut cursor = new_cursor();
                assert_ne!(
                    hand(&not, &mut game(), &mut cursor),
                    Outcome::Close,
                    "{not:?}"
                );
            }
        }

        #[test]
        fn the_mouse_shows_an_arrow_and_presses_the_quit_button() {
            let mut game = game();
            let mut cursor = new_cursor();
            assert!(!cursor.visible, "no arrow where there is no mouse");
            let (_, quit) = game.layout().buttons.last().copied().unwrap();
            let at = Point::new(quit.x + quit.width / 2, quit.y + quit.height / 2);
            let moved = InputEvent::PointerMoved { position: at };
            assert_eq!(hand(&moved, &mut game, &mut cursor), Outcome::Redraw);
            assert!(cursor.visible);
            assert_eq!(cursor.position, at);
            let press = InputEvent::PointerButton {
                button: PointerButton::Left,
                state: ElementState::Down,
                position: at,
                modifiers: Modifiers::NONE,
            };
            assert_eq!(hand(&press, &mut game, &mut cursor), Outcome::Close);
        }
    }
}

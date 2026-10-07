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
    use super::keys;
    use alpymist_solitaire::kept::Kept;
    use alpymist_solitaire::play::{self, Outcome, Play};
    use alpymist_ui::display::{self, Screen};
    use alpymist_ui::render::{new_cursor, paint_cursor};
    use denise::input::{ElementState, PointerButton};
    use denise::painter::Pen;
    use denise::{InputEvent, InputSource};
    use denise_evdev::{Console, InputBackend};
    use denise_ui::cursor::Cursor;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// The least time between two pictures: a mouse reports far more often
    /// than a screen is worth painting.
    const FRAME: Duration = Duration::from_millis(25);
    /// How long to rest when there is nothing to do.
    const REST: Duration = Duration::from_millis(8);

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
        let mut game =
            Play::new(alpymist_widget::appearance(), kept, dir, seed, nearly_out).with_quit();
        for p in game.font_problems() {
            eprintln!("font {p}");
        }
        game.resize(size, scale);

        let mut input: Option<InputBackend> = None;
        let mut next_look = Instant::now();
        let mut events: Vec<InputEvent> = Vec::new();
        let mut cursor = new_cursor();
        let mut dirty = true;
        let mut painted = Instant::now();

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

            if dirty && painted.elapsed() >= FRAME {
                screen
                    .present_with(|canvas| {
                        game.paint_on(canvas);
                        let mut pen = Pen::new(canvas);
                        paint_cursor(&mut pen, &cursor, &denise::theme::DARK);
                    })
                    .map_err(|e| format!("the display stopped taking pictures: {e}"))?;
                painted = Instant::now();
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

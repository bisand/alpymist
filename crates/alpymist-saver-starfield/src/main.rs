//! `alpymist-saver-starfield` — the view from a ship under way.
//!
//! A screensaver is a program, and this is one. It is never run by hand in
//! normal use: `alpymist-screensaver` runs it when the machine has been still
//! long enough, and Settings runs it to preview it. Run directly it does the
//! same thing, and goes away at the first key.
//!
//! What it draws is stars streaming past a ship that never quite holds a
//! straight line, and, every minute or so, an asteroid field with a hole
//! through it that the ship steers for. The stars move smoothly and the rocks
//! are drawn as coarse square cells — the two are separate settings, because
//! they cost separate things.

#![forbid(unsafe_code)]

mod picture;

use alpymist_screensaver::paint;
use picture::{Look, Starfield};
use std::process::ExitCode;

/// The name of this screensaver's definition file, and of its settings.
const ID: &str = "starfield";

fn main() -> ExitCode {
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("-h" | "--help" | "-V" | "--version")
    ) {
        println!("alpymist-saver-starfield {}", env!("CARGO_PKG_VERSION"));
        println!("The starfield screensaver. Run by alpymist-screensaver.");
        return ExitCode::SUCCESS;
    }

    let look = look();
    match paint::start(move |output| Box::new(Starfield::compose(output, &look))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-saver-starfield: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What this account has set, or the defaults where it has set nothing.
///
/// The ranges come from the definition file, which is also what Settings drew
/// its page from — so what the page allowed and what this accepts cannot drift
/// apart. With no definition installed the built-in defaults stand, and it
/// still draws.
fn look() -> Look {
    let fallback = Look::default();
    let Some(values) = alpymist_screensaver::values_for(ID) else {
        return fallback;
    };
    let number =
        |key: &str, from: i32| i32::try_from(values.number(key, i64::from(from))).unwrap_or(from);
    Look {
        block: u32::try_from(values.number("block", i64::from(fallback.block)))
            .unwrap_or(fallback.block),
        fps: values.number("fps", fallback.fps),
        stars: number("stars", fallback.stars),
        speed: number("speed", fallback.speed),
        rocks: values.switch("rocks", fallback.rocks),
        grain: u32::try_from(values.number("grain", i64::from(fallback.grain)))
            .unwrap_or(fallback.grain),
    }
}

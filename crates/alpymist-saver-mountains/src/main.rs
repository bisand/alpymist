//! `alpymist-saver-mountains` — the mountains, when nobody is there.
//!
//! A screensaver is a program, and this is one. It is never run by hand in
//! normal use: `alpymist-screensaver` runs it when the machine has been still
//! long enough, and Settings runs it to preview it. Run directly it does the
//! same thing, and goes away at the first key.
//!
//! What it draws is the same ranges as the desktop wallpaper, the boot splash
//! and the installer — composed at a fraction of the screen's resolution and
//! blown up in square blocks, which is both the pixelated look and the reason
//! it costs a few per cent of one core.

#![forbid(unsafe_code)]

mod picture;

use alpymist_screensaver::{Values, paint};
use picture::{Look, Mountains};
use std::process::ExitCode;

/// The name of this screensaver's definition file, and of its settings.
const ID: &str = "mountains";

fn main() -> ExitCode {
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("-h" | "--help" | "-V" | "--version")
    ) {
        println!("alpymist-saver-mountains {}", env!("CARGO_PKG_VERSION"));
        println!("The mountains screensaver. Run by alpymist-screensaver.");
        return ExitCode::SUCCESS;
    }

    let look = look();
    match paint::start(move |output| Box::new(Mountains::compose(output, &look))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-saver-mountains: {e}");
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
        mist: number("mist", fallback.mist),
        speed: number("speed", fallback.speed),
    }
}

/// Kept so the binary's build exercises the library's value reading.
#[allow(dead_code)]
fn values() -> Option<Values> {
    alpymist_screensaver::values_for(ID)
}

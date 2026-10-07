//! `alpymist-solitaire` — Klondike in a window.
//!
//! ```text
//! alpymist-solitaire           deal a game, turning as many cards as last time
//! alpymist-solitaire --one     deal one that turns one card at a time
//! alpymist-solitaire --three   deal one that turns three
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-solitaire [--one | --three]

Deals a game of Klondike in a window.
  --one      turn one card from the stock at a time
  --three    turn three
Without either it turns as many as the last game did.

Drag cards with the mouse, or double-click one to send it home. With the
keyboard: the arrows move between piles, Enter picks up and puts down,
Up and Down on a pile picked up take more or fewer cards, Space turns the
stock, U takes a move back, N deals again, B and Shift+B change the
picture on the cards' backs, S shows the best games, Ctrl+Q closes.

The clock starts at the first move. The five shortest games of each kind
are kept, with the back and the way of turning, in
~/.local/state/alpymist/solitaire.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => open(None, false),
        ["--one"] => open(Some(1), false),
        ["--three"] => open(Some(3), false),
        // Not in the usage: a game five cards from out, to try how one ends
        // without playing one through.
        ["--nearly-out"] => open(None, true),
        ["-V" | "--version"] => {
            println!("alpymist-solitaire {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(target_os = "linux")]
fn open(turn: Option<usize>, nearly_out: bool) -> ExitCode {
    match app::run(turn, nearly_out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-solitaire: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open(_turn: Option<usize>, _nearly_out: bool) -> ExitCode {
    eprintln!("alpymist-solitaire: the window needs Wayland");
    ExitCode::FAILURE
}

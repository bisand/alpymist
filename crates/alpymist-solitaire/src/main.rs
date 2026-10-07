//! `alpymist-solitaire` — Klondike in a window.
//!
//! ```text
//! alpymist-solitaire           deal a game
//! alpymist-solitaire --three   deal one that turns three cards at a time
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-solitaire [--three]

Deals a game of Klondike in a window.
  --three    turn three cards from the stock at a time, not one

Drag cards with the mouse, or double-click one to send it home. With the
keyboard: the arrows move between piles, Enter picks up and puts down,
Up and Down on a pile picked up take more or fewer cards, Space turns the
stock, U takes a move back, N deals again, Ctrl+Q closes.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => open(1),
        ["--three"] => open(3),
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
fn open(turn: usize) -> ExitCode {
    match app::run(turn) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-solitaire: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open(_turn: usize) -> ExitCode {
    eprintln!("alpymist-solitaire: the window needs Wayland");
    ExitCode::FAILURE
}

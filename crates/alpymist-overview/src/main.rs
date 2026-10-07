//! `alpymist-overview` — the workspaces of the screen with the focus, side by
//! side, to pick one from.
//!
//! ```text
//! alpymist-overview    open the overview, or move the open one's mark on
//! ```
//!
//! `alpymist displays overview`, which Super+Tab runs, starts this wherever
//! Hyprland's own overview cannot load.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-overview

Shows the nine workspaces of the screen with the focus, each window an
outline with its program's name. Run again while it is open, it moves on to
the next workspace, which is what holding Super and pressing Tab does.
  Tab, Shift+Tab, arrows    move between workspaces
  Enter, or a click         go to the chosen one
  1 to 9                    go to that one
  Esc                       close, and stay where you were";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => open(),
        ["-V" | "--version"] => {
            println!("alpymist-overview {}", env!("CARGO_PKG_VERSION"));
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
fn open() -> ExitCode {
    match app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-overview: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open() -> ExitCode {
    eprintln!("alpymist-overview: the overview needs a Wayland session");
    ExitCode::FAILURE
}

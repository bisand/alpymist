//! `alpymist-settings` — every Alpymist setting in one window.
//!
//! ```text
//! alpymist-settings                     open Settings, or bring it forward
//! alpymist-settings touchpad            open at an area
//! alpymist-settings keyboard.layout     open at a setting, focused
//! ```
//!
//! The menu's entries for each setting run the last form (ADR 0007).

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-settings [AREA | SETTING]

Opens Settings, or brings the open window forward, at an area such as
`touchpad` or a setting such as `touchpad.natural-scroll`.
`alpymist list` shows every setting on the command line.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["-V" | "--version"] => {
            println!("alpymist-settings {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [] => open(None),
        [id] if !id.starts_with('-') => open(Some(id)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(target_os = "linux")]
fn open(at: Option<&str>) -> ExitCode {
    match app::run(at) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-settings: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open(_: Option<&str>) -> ExitCode {
    eprintln!("alpymist-settings: the window needs Wayland; `alpymist list` shows every setting");
    ExitCode::FAILURE
}

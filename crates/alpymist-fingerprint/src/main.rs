//! `alpymist-fingerprint` — enrol a finger, test it, remove them (#58).
//!
//! ```text
//! alpymist-fingerprint        open the window, or bring it forward
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-fingerprint

Opens the fingerprint window: choose a finger, add it, test it, or remove
every fingerprint. Adding or removing one asks for your password.";

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
            println!("alpymist-fingerprint {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [] => open(),
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
            eprintln!("alpymist-fingerprint: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open() -> ExitCode {
    eprintln!("alpymist-fingerprint: the window needs Wayland");
    ExitCode::FAILURE
}

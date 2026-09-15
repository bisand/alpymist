//! `alpymist-about` — which Alpymist this system runs.
//!
//! ```text
//! alpymist-about           open the About box, or bring the open one forward
//! alpymist-about --print   print the same details, for a terminal or a bug report
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use alpymist_about::info::About;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-about [--print]

With no option, opens the About box, or brings the open one forward.
  --print    print the version, channel, system and packages instead";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => open(),
        ["--print"] => {
            print!("{}", About::read(Path::new("/")).text());
            ExitCode::SUCCESS
        }
        ["-V" | "--version"] => {
            println!("alpymist-about {}", env!("CARGO_PKG_VERSION"));
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
    match app::run(About::read(Path::new("/"))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-about: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn open() -> ExitCode {
    eprintln!("alpymist-about: the window needs Wayland; use --print");
    ExitCode::FAILURE
}

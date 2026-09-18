//! `alpymist-screensaver` — the mountains, when nobody is there.
//!
//! ```text
//! alpymist-screensaver         show the mountains (run again to take them away)
//! alpymist-screensaver stop    take them away, if they are up
//! alpymist-screensaver idle    watch for idleness, and start over if watching
//! ```
//!
//! Nothing here decides *when* to appear. `idle` keeps a `swayidle` running
//! with the account's settings, and it is swayidle that runs the first of these
//! when the seat has been still long enough. See [`alpymist_screensaver::idle`].

#![forbid(unsafe_code)]

use alpymist_screensaver::config::Config;
use alpymist_screensaver::idle;
use std::process::ExitCode;

/// The name the running screensaver listens under, so a second run reaches it.
const NAME: &str = "alpymist-screensaver";

const USAGE: &str = "\
usage: alpymist-screensaver [stop | idle]

With no command, covers the screen with the Alpymist mountains until a key is
pressed or the pointer moves.
  stop   take the screensaver away, if one is up
  idle   watch for idleness and show it in its own time; running this again
         reads the settings afresh and starts the watch over";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("alpymist-screensaver {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("stop") => {
            stop();
            ExitCode::SUCCESS
        }
        Some("idle") => report(idle::watch()),
        Some(other) => {
            eprintln!("alpymist-screensaver: unknown command {other}\n\n{USAGE}");
            ExitCode::from(2)
        }
        None => report(show()),
    }
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-screensaver: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Take away a screensaver that is up. Nothing to do is not a failure: the
/// resume command runs whether or not the timeout before it ever fired.
fn stop() {
    if let Some(path) = alpymist_widget::instance::socket_path(NAME) {
        // Connecting is what closes it; there is nothing to say afterwards.
        std::os::unix::net::UnixStream::connect(path).ok();
    }
}

/// Cover the screen until somebody comes back.
#[cfg(target_os = "linux")]
fn show() -> Result<(), String> {
    use alpymist_screensaver::saver::{Saver, backdrop};
    use alpymist_widget::host;

    let config = Config::load().unwrap_or_else(|e| {
        eprintln!("alpymist-screensaver: {e}");
        Config::default()
    });
    alpymist_widget::instance::toggle(NAME, |listener| {
        let mut options = host::Options::new(NAME);
        options.placement = host::Placement::FullScreen;
        // Under the first frame, and under the edges of a screen the blocks do
        // not quite divide.
        options.backdrop = backdrop();
        let (_sender, events) = host::events::<()>();
        host::run(Saver::new(config.block), &options, events, listener)
    })
}

/// There is no layer shell off Linux; the logic and the tests still build.
#[cfg(not(target_os = "linux"))]
fn show() -> Result<(), String> {
    let _ = Config::load();
    Err("the screensaver needs a Wayland compositor".to_owned())
}

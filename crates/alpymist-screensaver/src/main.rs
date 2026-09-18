//! `alpymist-screensaver` — which screensaver runs, and when.
//!
//! ```text
//! alpymist-screensaver         run the chosen screensaver (or a random one)
//! alpymist-screensaver list    every screensaver installed
//! alpymist-screensaver stop    take away whichever one is up
//! alpymist-screensaver idle    watch for idleness, and start over if watching
//! ```
//!
//! This draws nothing itself. A screensaver is a program with a definition file
//! beside it (see `alpymist_screensaver::definition`); this finds them, decides
//! which to run, and runs it. `idle` keeps a `swayidle` running that starts this
//! when the seat has been still long enough.

#![forbid(unsafe_code)]

use alpymist_screensaver::config::Config;
use alpymist_screensaver::definition::{Definition, discover};
use alpymist_screensaver::paint;
use alpymist_screensaver::picture::Show;
use alpymist_screensaver::{Values, idle};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: alpymist-screensaver [list | stop | idle]

With no command, runs the screensaver chosen in Settings — or one at random,
which is the default — until a key is pressed or the pointer moves.
  list   every screensaver installed, and what each is called
  stop   take away whichever screensaver is up, if any
  idle   watch for idleness and run one in its own time; running this again
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
        Some("list") => {
            list();
            ExitCode::SUCCESS
        }
        Some("stop") => {
            paint::stop();
            ExitCode::SUCCESS
        }
        Some("idle") => report(idle::watch()),
        Some(other) => {
            eprintln!("alpymist-screensaver: unknown command {other}\n\n{USAGE}");
            ExitCode::from(2)
        }
        None => report(run()),
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

/// Every screensaver installed, and what it is called.
fn list() {
    let installed = discover();
    if installed.is_empty() {
        println!("no screensavers are installed");
        return;
    }
    for def in &installed {
        println!("{:<16} {}", def.id, def.name);
    }
}

/// Run the chosen screensaver.
///
/// The screensaver replaces this process rather than being started beside it,
/// so what swayidle started and what `alpymist-screensaver stop` takes away are
/// one process, and nothing is left behind holding the socket.
fn run() -> Result<(), String> {
    let config = Config::load().unwrap_or_else(|e| {
        eprintln!("alpymist-screensaver: {e}");
        Config::default()
    });
    let installed = discover();
    let Some(def) = config.show.resolve(&installed) else {
        return Err(missing(&config.show, &installed));
    };
    // The values are not read here: the screensaver reads its own, so one added
    // later needs nothing from this program but its name.
    let program = def.exec.clone();
    let error = Command::new(&program).exec_replacing();
    Err(format!("{program}: {error}"))
}

/// Why there is nothing to run, in terms of what to do about it.
fn missing(show: &Show, installed: &[Definition]) -> String {
    match show {
        _ if installed.is_empty() => {
            "no screensavers are installed; there is nothing to show".to_owned()
        }
        Show::Random => "no screensavers are installed; there is nothing to show".to_owned(),
        Show::One(id) => format!(
            "no screensaver called `{id}` is installed; `alpymist-screensaver list` \
             shows those that are"
        ),
    }
}

/// Replace this process with the command, and return why if that failed.
trait Replace {
    fn exec_replacing(&mut self) -> std::io::Error;
}

impl Replace for Command {
    fn exec_replacing(&mut self) -> std::io::Error {
        use std::os::unix::process::CommandExt as _;
        // `exec` only ever returns an error: on success this process is gone.
        self.exec()
    }
}

/// Kept so the library's value reading is exercised by the binary's own build.
#[allow(dead_code)]
fn values_of(def: &Definition) -> Values {
    Values::read(def)
}

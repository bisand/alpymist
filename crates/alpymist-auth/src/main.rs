//! `alpymist-auth` — Alpymist's password prompt.
//!
//! ```text
//! alpymist-auth run -- COMMAND [ARGS…]   run COMMAND through pkexec, asking here
//! alpymist-auth prompt                   the dialog, started by an agent
//! ```
//!
//! `run` is for a menu entry or a script: it registers an agent for itself
//! and runs `pkexec COMMAND`, so whatever polkit asks is asked in the dialog.
//! Programs that authenticate often, such as the store, register an agent for
//! themselves instead, so polkit's "keep" lets a second install within a few
//! minutes through without asking again.
//!
//! `prompt` reads polkitd's description of the request as JSON on standard
//! input, asks, and exits 0 only when polkit's helper said the password was
//! right.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod dialog;

use std::process::ExitCode;

const USAGE: &str = "\
usage: alpymist-auth run -- COMMAND [ARGS...]

Runs COMMAND as root through pkexec, asking for the password in Alpymist's
dialog rather than on a terminal. polkit decides who may, and checks the
password; this only asks for it.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => {
            let command = match args.get(1).map(String::as_str) {
                Some("--") => &args[2..],
                _ => &args[1..],
            };
            run(command)
        }
        Some("prompt") => prompt(),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(command: &[String]) -> ExitCode {
    let Some((program, rest)) = command.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // ALPYMIST_AUTH names another prompt, for trying a build; it is this
    // process's own environment, so nothing it could not already change.
    let me = match std::env::var_os("ALPYMIST_AUTH")
        .filter(|p| !p.is_empty())
        .map_or_else(std::env::current_exe, |p| Ok(p.into()))
    {
        Ok(me) => me,
        Err(e) => {
            eprintln!("alpymist-auth: {e}");
            return ExitCode::FAILURE;
        }
    };
    let registration = match alpymist_auth::agent::register(me) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("alpymist-auth: {e}");
            return ExitCode::FAILURE;
        }
    };
    let status = std::process::Command::new("pkexec")
        .arg(program)
        .args(rest)
        .status();
    drop(registration);
    match status {
        Ok(s) => ExitCode::from(u8::try_from(s.code().unwrap_or(1)).unwrap_or(1)),
        Err(e) => {
            eprintln!("alpymist-auth: pkexec: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn prompt() -> ExitCode {
    use std::io::Read;
    let mut input = Vec::new();
    if std::io::stdin()
        .take(1 << 16)
        .read_to_end(&mut input)
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    let request: alpymist_auth::request::Request = match serde_json::from_slice(&input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("alpymist-auth: not a request: {e}");
            return ExitCode::from(2);
        }
    };
    match dialog::ask(request) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("alpymist-auth: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn prompt() -> ExitCode {
    eprintln!("alpymist-auth: the prompt needs Wayland");
    ExitCode::FAILURE
}

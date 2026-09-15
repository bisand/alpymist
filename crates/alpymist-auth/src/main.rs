//! `alpymist-auth` — Alpymist's password prompt.
//!
//! ```text
//! alpymist-auth run -- COMMAND [ARGS…]   run COMMAND through pkexec, asking here
//! alpymist-auth prompt                   the dialog, started by an agent
//! alpymist-auth attention                check the prompt on screen is Alpymist's
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
        Some("attention") => attention(),
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

/// Ctrl+Alt+Delete: say whether the prompt on screen is Alpymist's.
///
/// Hyprland is asked which process drew each overlay, and says the answer in
/// its own notification, drawn by the compositor. A genuine prompt is told
/// too, and shows it was checked.
fn attention() -> ExitCode {
    use alpymist_auth::attention::{self, Verdict};
    let hyprctl = |args: &[&str]| {
        std::process::Command::new("/usr/bin/hyprctl")
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
    };
    let layers = match hyprctl(&["-j", "layers"]) {
        Ok(out) if out.status.success() => serde_json::from_slice(&out.stdout).unwrap_or_default(),
        _ => {
            eprintln!("alpymist-auth: could not ask Hyprland for its layers");
            return ExitCode::FAILURE;
        }
    };
    let verdict = attention::judge(&attention::overlays(&layers), attention::exe);
    // Permissions are what keep other programs from reading the screen and
    // typing; without them a genuine prompt is still genuine, but not the
    // whole story.
    let protected = hyprctl(&["-j", "getoption", "ecosystem:enforce_permissions"])
        .ok()
        .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok())
        .and_then(|v| v.get("int").and_then(serde_json::Value::as_i64))
        .is_some_and(|v| v != 0);
    let (icon, colour) = match (&verdict, protected) {
        (Verdict::Genuine(_), true) => ("5", "rgb(9ad9a2)"),
        (Verdict::Impostor, _) | (Verdict::Genuine(_), false) => ("3", "rgb(e8b06a)"),
        (Verdict::Nothing, _) => ("1", "rgb(7fb8d9)"),
    };
    let mut sentence = verdict.sentence().to_owned();
    if !protected {
        sentence.push_str(
            ". Warning: Hyprland's permissions are off, so other programs can read the screen",
        );
    }
    let _ = hyprctl(&["notify", icon, "8000", colour, &sentence]);
    if let Verdict::Genuine(pids) = &verdict {
        for pid in pids {
            if let Some(path) = attention::socket(*pid)
                && let Ok(mut stream) = std::os::unix::net::UnixStream::connect(path)
            {
                let _ = std::io::Write::write_all(&mut stream, b"verified\n");
            }
        }
    }
    if matches!(verdict, Verdict::Impostor) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
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

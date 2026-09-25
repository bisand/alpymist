//! Lock this session.
//!
//! ```text
//! alpymist-lock          lock, and stay until the password is given
//! alpymist-lock -f       lock, and return once the screen is covered
//! ```
//!
//! `-f` is what a lock before suspending needs, and it is spelled as
//! `swaylock`'s for the same reason: swayidle runs one command and waits for
//! it, so the command has to return when the screen is covered and not before,
//! or the machine sleeps with the desktop still showing. It is done by running
//! this program again rather than by forking, which keeps the whole thing free
//! of unsafe code: the second run does the locking and says so down a pipe,
//! and the first returns when it hears it.

#![forbid(unsafe_code)]

const USAGE: &str = "\
alpymist-lock — lock this session behind the Alpymist login screen

    alpymist-lock          lock, and stay until the password is given
    alpymist-lock -f       lock, and return once the screen is covered
    alpymist-lock --help   this

The password is the account's own, checked by PAM as the alpymist-lock
service. The compositor holds the lock: it stays even if this program is
killed, and only the password takes it away.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => lock::now(),
        ["-f" | "--daemonize"] => lock::in_the_background(),
        ["-h" | "--help"] => {
            print!("{USAGE}");
            return;
        }
        ["-V" | "--version"] => {
            println!("alpymist-lock {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        other => Err(format!(
            "alpymist-lock takes no arguments but -f: {}",
            other.join(" ")
        )),
    };
    if let Err(why) = result {
        eprintln!("alpymist-lock: {why}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod lock {
    use alpymist_greeter::app::{App, Authenticator, Purpose};
    use alpymist_lock::host::{self, Ending};
    use alpymist_lock::{pam, who};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::Arc;

    /// What the second run says down the pipe once the screen is covered.
    const READY: &str = "locked";

    /// The size the screen is first laid out for: the smallest one supported,
    /// because the real one is not known until the compositor says how big its
    /// outputs are, and laying out for a screen that turns out to be bigger is
    /// a composition thrown away.
    const UNTIL_CONFIGURED: (u32, u32) = (640, 480);

    /// What must be true before the compositor is asked to cover the screen.
    ///
    /// All of it is checked before anything is locked. A lock whose password
    /// cannot be checked is a session that has to be rescued from a text
    /// console, and this is the last moment at which saying so is cheap.
    fn preflight() -> Result<who::User, String> {
        if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|d| d.is_empty()) {
            return Err("this locks a Wayland session, and there is none here".into());
        }
        if !pam::configured() {
            return Err(format!(
                "there is no {}, so no password could be checked: not locking",
                pam::CONFIG
            ));
        }
        who::current().ok_or_else(|| {
            "cannot tell which account this session belongs to: not locking".to_owned()
        })
    }

    /// Lock, and stay until the password is given.
    pub fn now() -> Result<(), String> {
        let user = preflight()?;
        let authenticate: Authenticator = Arc::new(pam::check);
        let mut app = App::new(
            vec![user],
            authenticate,
            UNTIL_CONFIGURED.0,
            UNTIL_CONFIGURED.1,
        );
        app.purpose = Purpose::Unlock;
        // Before the lock is asked for, so the first frame is the picture
        // rather than the mountains and then the picture.
        app.load_picture(std::path::Path::new(alpymist_greeter::app::PICTURE));
        app.hostname = std::fs::read_to_string("/etc/hostname")
            .map(|name| name.trim().to_owned())
            .unwrap_or_default();

        let covered = Box::new(|| {
            // Whoever is waiting to hear it, hears it once.
            let mut out = std::io::stdout();
            writeln!(out, "{READY}").ok();
            out.flush().ok();
        });
        match host::run(app, covered)? {
            Ending::Unlocked => Ok(()),
            Ending::Refused => {
                eprintln!("alpymist-lock: the session was already locked");
                Ok(())
            }
        }
    }

    /// Lock, and return once the screen is covered.
    pub fn in_the_background() -> Result<(), String> {
        // Checked here as well as in the second run: a message on the way to
        // suspending is worth more than one from a process nobody is watching.
        preflight()?;
        let me = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
        let mut child = Command::new(me)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start the lock: {e}"))?;
        let said = child.stdout.take().map(|pipe| {
            let mut line = String::new();
            BufReader::new(pipe).read_line(&mut line).ok();
            line
        });
        if said.is_some_and(|line| line.trim() == READY) {
            // The screen is covered. The second run is on its own from here;
            // this one returns so that whatever is waiting for it can go on.
            return Ok(());
        }
        // It gave up before covering anything, and has said why on the error
        // it shares with this one. Its answer is this one's.
        let status = child
            .wait()
            .map_err(|e| format!("could not wait for the lock: {e}"))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

#[cfg(not(target_os = "linux"))]
mod lock {
    /// Everything this does — the session lock and PAM — is Linux's.
    fn nowhere() -> Result<(), String> {
        Err("a Wayland session lock needs Linux".into())
    }

    pub fn now() -> Result<(), String> {
        nowhere()
    }

    pub fn in_the_background() -> Result<(), String> {
        nowhere()
    }
}

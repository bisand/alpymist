//! A session's secrets: the keyring PAM unlocked at login, and an SSH agent
//! that lives and dies with the session (ADR 0013).
//!
//! PAM starts gnome-keyring with the login password and unlocks it, before
//! there is a session bus to put it on: `dbus-run-session` makes that bus
//! afterwards, around `alpymist session`. So here, on that bus, the running
//! daemon is asked to take its place there, and programs find it at
//! `org.freedesktop.secrets` — Flatpak apps too, through the portal.
//!
//! The SSH agent is OpenSSH's own, holding keys in memory only, and started
//! as the compositor's parent: given a command, `ssh-agent` runs it and exits
//! when it does, so the keys go with the session. ssh adds a key the first
//! time it is used (`AddKeysToAgent`, from the package's `ssh_config.d`), asking
//! for its passphrase through gcr's dialog, and the lock screen tells the
//! agent to forget them all.

use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The keyring daemon.
const KEYRING: &str = "/usr/bin/gnome-keyring-daemon";
/// OpenSSH's agent.
const AGENT: &str = "/usr/bin/ssh-agent";
/// gcr's passphrase dialog, which ssh asks when there is no terminal to.
const ASKPASS: &str = "/usr/libexec/gcr4-ssh-askpass";
/// The agent's socket, in the session's runtime directory: only this account
/// can reach it, and it is gone at the next boot.
pub const AGENT_SOCKET: &str = "ssh-agent.socket";
/// How long the keyring is given to answer. It is one exchange on a socket;
/// a login that never arrives is worse than a keyring that is not there.
const WAIT: Duration = Duration::from_secs(5);

/// What the session is started with for its secrets.
pub fn environment(socket: Option<&Path>) -> Vec<(String, String)> {
    // Chromium and every Electron app choose where they keep passwords and
    // tokens by which desktop they think they are on. Hyprland is not one they
    // know, and on a desktop they do not know they keep them "basic": in a file,
    // under a key built into the program. DESKTOP_SESSION is what they read
    // once XDG_CURRENT_DESKTOP has told them nothing, and "gnome" there makes
    // them use the keyring, Flatpak's included, with nothing set app by app.
    // Almost nothing else reads it, and XDG_CURRENT_DESKTOP stays Hyprland.
    let mut vars = vec![("DESKTOP_SESSION".to_owned(), "gnome".to_owned())];
    if Path::new(KEYRING).exists() {
        match keyring() {
            Ok(found) => vars.extend(found),
            Err(e) => eprintln!("alpymist session: the keyring: {e}"),
        }
    }
    if let Some(socket) = socket
        && Path::new(AGENT).exists()
    {
        vars.push((
            "SSH_AUTH_SOCK".to_owned(),
            socket.to_string_lossy().into_owned(),
        ));
        if Path::new(ASKPASS).exists() {
            vars.push(("SSH_ASKPASS".to_owned(), ASKPASS.to_owned()));
            // The dialog, even from a terminal: one way of being asked, and
            // one a program cannot draw over.
            vars.push(("SSH_ASKPASS_REQUIRE".to_owned(), "prefer".to_owned()));
        }
    }
    vars
}

/// The keyring PAM started, put on this session's bus: what it says to set.
fn keyring() -> Result<Vec<(String, String)>, String> {
    let mut child = Command::new(KEYRING)
        .args(["--start", "--components=secrets"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(status)) => return Err(format!("it said {status}")),
            Ok(None) if started.elapsed() < WAIT => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it did not answer".into());
            }
        }
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    Ok(said(&out))
}

/// `KEY=value` lines, less any agent of the keyring's: this session's agent
/// is OpenSSH's.
fn said(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| !k.is_empty() && *k != "SSH_AUTH_SOCK")
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// The command line that runs `program` with `args`, under the agent when
/// there is one: it runs the program and exits with it.
pub fn under_agent(program: &str, args: &[String], socket: Option<&Path>) -> Vec<String> {
    let mut line = Vec::new();
    if let Some(socket) = socket
        && Path::new(AGENT).exists()
    {
        // A socket left by a session that did not end cleanly would stop the
        // agent binding; nothing else could be listening on it by now.
        let _ = std::fs::remove_file(socket);
        line.extend([
            AGENT.to_owned(),
            "-a".to_owned(),
            socket.to_string_lossy().into_owned(),
        ]);
    }
    line.push(program.to_owned());
    line.extend(args.iter().cloned());
    line
}

#[cfg(test)]
mod tests {
    use super::said;

    #[test]
    fn the_keyring_is_heard_but_not_its_agent() {
        assert_eq!(
            said(
                "GNOME_KEYRING_CONTROL=/run/user/1000/keyring\nSSH_AUTH_SOCK=/run/user/1000/keyring/ssh\n\n"
            ),
            [(
                "GNOME_KEYRING_CONTROL".to_owned(),
                "/run/user/1000/keyring".to_owned()
            )]
        );
    }
}

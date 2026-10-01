//! A provider's keys, in the keyring.
//!
//! Through `secret-tool`, libsecret's own, which speaks to gnome-keyring
//! (ADR 0013): unlocked at login, locked when nobody is logged in, and never
//! a file in the home directory. A key is found by three attributes — this
//! service, the provider's id, and the credential's key — and neither stored
//! nor looked up through an argument: `secret-tool store` reads it from its
//! input, and prints it only to the pipe it is read from.
//!
//! **A locked keyring is never waited on.** Asked for anything while locked,
//! the keyring puts up a dialog to unlock it and `secret-tool` waits for the
//! answer. The bar asks every few minutes with nobody there to answer, so
//! that would be a dialog out of nowhere, again and again, or a bar that
//! stopped. So the keyring is asked first where it is ([`state`]), once, and
//! when it is locked or not answering nothing more is asked of it. Every
//! call has a time limit besides.

use std::io::{Read as _, Write as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// What is said when the keyring is locked.
pub const LOCKED: &str = "The keyring is locked. It opens when you log in with your password.";

/// What is said when it does not answer at all: after a login that had no
/// password, nothing has started it.
pub const SILENT: &str =
    "The keyring is not answering. It opens when you log in with your password.";

/// How long a keyring call may take before it is given up on.
const PATIENCE: Duration = Duration::from_secs(8);

/// Where the keyring is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyring {
    /// Open: keys can be read and kept.
    Open,
    /// Locked: asking it anything would put up a dialog.
    Locked,
    /// Not answering: no keyring is running in this session.
    Silent,
    /// It answered something else — no default keyring yet, say. Worth
    /// trying, within the time limit.
    Unknown,
}

impl Keyring {
    /// Why nothing can be asked of it, when nothing can.
    #[must_use]
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Locked => Some(LOCKED),
            Self::Silent => Some(SILENT),
            Self::Open | Self::Unknown => None,
        }
    }
}

/// Ask the keyring where it is, without making it ask anyone anything.
#[must_use]
pub fn state() -> Keyring {
    let child = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--reply-timeout=3000",
            "--dest=org.freedesktop.secrets",
            "/org/freedesktop/secrets/aliases/default",
            "org.freedesktop.DBus.Properties.Get",
            "string:org.freedesktop.Secret.Collection",
            "string:Locked",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return Keyring::Unknown;
    };
    let errors = child.stderr.take();
    let Some((_, mut said)) = finish(child, PATIENCE) else {
        return Keyring::Silent;
    };
    if let Some(mut errors) = errors {
        let _ = errors.read_to_string(&mut said);
    }
    said_state(&said)
}

/// What `dbus-send` printed for the `Locked` property, or failed with.
fn said_state(reply: &str) -> Keyring {
    let mut words = reply.split_whitespace().skip_while(|w| *w != "boolean");
    match (words.next(), words.next()) {
        (Some("boolean"), Some("true")) => Keyring::Locked,
        (Some("boolean"), Some("false")) => Keyring::Open,
        // Nobody there, or nobody answering in time: the same to us.
        _ if [
            "NoReply",
            "ServiceUnknown",
            "NameHasNoOwner",
            "TimedOut",
            "Timeout",
            "Spawn.",
        ]
        .iter()
        .any(|e| reply.contains(e)) =>
        {
            Keyring::Silent
        }
        _ => Keyring::Unknown,
    }
}

/// Wait for `child` no longer than `patience`: whether it succeeded and what
/// it printed, or `None` when it had to be stopped.
fn finish(mut child: Child, patience: Duration) -> Option<(bool, String)> {
    let deadline = Instant::now() + patience;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut said = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut said);
    }
    Some((status.success(), said))
}

/// What the keyring's entries are filed under.
pub const SERVICE: &str = "alpymist-ai-usage";

fn attributes(provider: &str, key: &str) -> [String; 6] {
    [
        "service".into(),
        SERVICE.into(),
        "provider".into(),
        provider.into(),
        "credential".into(),
        key.into(),
    ]
}

/// The secret kept for `provider`'s `key`: `Ok(None)` when there is none.
/// The caller has asked [`state`] already, and does not call this for a
/// keyring that is locked or silent.
///
/// # Errors
/// The keyring did not answer in time. That is not the same as having no
/// key, and is not said as if it were.
pub fn lookup(provider: &str, key: &str) -> Result<Option<String>, String> {
    let child = Command::new("secret-tool")
        .arg("lookup")
        .args(attributes(provider, key))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("secret-tool: {e}"))?;
    let (ok, secret) = finish(child, PATIENCE).ok_or(SILENT)?;
    let secret = secret.trim_end_matches(['\n', '\r']);
    Ok((ok && !secret.is_empty()).then(|| secret.to_owned()))
}

/// Keep `secret` for `provider`'s `key`, under `label` as the keyring's own
/// tools show it.
///
/// # Errors
/// The keyring is locked, `secret-tool` is missing, or the keyring refused
/// or did not answer.
pub fn store(provider: &str, key: &str, label: &str, secret: &str) -> Result<(), String> {
    if let Some(why) = state().refusal() {
        return Err(why.into());
    }
    let mut child = Command::new("secret-tool")
        .args(["store", "--label", label])
        .args(attributes(provider, key))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("secret-tool: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(secret.as_bytes())
            .map_err(|e| format!("secret-tool: {e}"))?;
    }
    match finish(child, PATIENCE) {
        Some((true, _)) => Ok(()),
        Some((false, _)) => Err("the keyring did not take the key".into()),
        None => Err(SILENT.into()),
    }
}

/// Forget what is kept for `provider`'s `key`.
///
/// # Errors
/// The keyring is locked or did not answer, so the key is still there.
pub fn clear(provider: &str, key: &str) -> Result<(), String> {
    if let Some(why) = state().refusal() {
        return Err(why.into());
    }
    let child = Command::new("secret-tool")
        .arg("clear")
        .args(attributes(provider, key))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("secret-tool: {e}"))?;
    match finish(child, PATIENCE) {
        Some(_) => Ok(()),
        None => Err(SILENT.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Keyring, finish, said_state};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn the_keyrings_answer_says_where_it_is() {
        let open = "method return time=1790851227.613 sender=:1.4 -> destination=:1.56 serial=39 reply_serial=2\n   variant       boolean false\n";
        assert_eq!(said_state(open), Keyring::Open);
        assert_eq!(
            said_state("   variant       boolean true\n"),
            Keyring::Locked
        );
        // After a login with no password, as seen on the dev VM.
        assert_eq!(
            said_state("Error org.freedesktop.DBus.Error.NoReply: Did not receive a reply."),
            Keyring::Silent
        );
        assert_eq!(
            said_state(
                "Error org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable"
            ),
            Keyring::Silent
        );
        // No default keyring yet: not a reason to give up before trying.
        assert_eq!(
            said_state(
                "Error org.freedesktop.DBus.Error.UnknownMethod: Object does not exist at path"
            ),
            Keyring::Unknown
        );
        assert_eq!(Keyring::Locked.refusal(), Some(super::LOCKED));
        assert_eq!(Keyring::Silent.refusal(), Some(super::SILENT));
        assert_eq!(Keyring::Open.refusal(), None);
        assert_eq!(Keyring::Unknown.refusal(), None);
    }

    #[test]
    fn a_call_that_does_not_answer_is_given_up_on() {
        let sh = |script: &str| {
            Command::new("sh")
                .args(["-c", script])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap()
        };
        let began = Instant::now();
        assert_eq!(finish(sh("sleep 30"), Duration::from_millis(200)), None);
        assert!(began.elapsed() < Duration::from_secs(5), "not waited out");
        assert_eq!(
            finish(sh("echo sesame"), Duration::from_secs(5)),
            Some((true, "sesame\n".into()))
        );
        assert_eq!(
            finish(sh("exit 1"), Duration::from_secs(5)),
            Some((false, String::new()))
        );
    }
}

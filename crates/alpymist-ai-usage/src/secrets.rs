//! A provider's keys, in the keyring.
//!
//! Through `secret-tool`, libsecret's own, which speaks to gnome-keyring
//! (ADR 0013): unlocked at login, locked when nobody is logged in, and never
//! a file in the home directory. A key is found by three attributes — this
//! service, the provider's id, and the credential's key — and neither stored
//! nor looked up through an argument: `secret-tool store` reads it from its
//! input, and prints it only to the pipe it is read from.

use std::io::Write as _;
use std::process::{Command, Stdio};

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

/// The secret kept for `provider`'s `key`, if there is one and the keyring
/// is unlocked.
#[must_use]
pub fn lookup(provider: &str, key: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .arg("lookup")
        .args(attributes(provider, key))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let secret = String::from_utf8(output.stdout).ok()?;
    let secret = secret.trim_end_matches(['\n', '\r']);
    (output.status.success() && !secret.is_empty()).then(|| secret.to_owned())
}

/// Keep `secret` for `provider`'s `key`, under `label` as the keyring's own
/// tools show it.
///
/// # Errors
/// `secret-tool` is missing, or the keyring refused.
pub fn store(provider: &str, key: &str, label: &str, secret: &str) -> Result<(), String> {
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
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err("the keyring did not take the key: is it unlocked?".into()),
        Err(e) => Err(format!("secret-tool: {e}")),
    }
}

/// Forget what is kept for `provider`'s `key`.
pub fn clear(provider: &str, key: &str) {
    let _ = Command::new("secret-tool")
        .arg("clear")
        .args(attributes(provider, key))
        .stdin(Stdio::null())
        .status();
}

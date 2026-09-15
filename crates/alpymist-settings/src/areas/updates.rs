//! The release channel: which Alpymist repository apk follows.
//!
//! A channel is the one Alpymist line in `/etc/apk/repositories` and, for dev,
//! the key its index is signed with (ADR 0006). Setting it switches both;
//! upgrading to it is apk's, as `alpymist channel` does after switching.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::Channel;
use std::io::ErrorKind;

/// apk's repositories.
pub const REPOSITORIES: &str = "etc/apk/repositories";
/// Where apk looks for keys.
pub const APK_KEYS: &str = "etc/apk/keys";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: "updates.channel",
        title: "Release channel",
        description: "Stable is released packages; dev is every change to main, and may break.",
        keywords: &["stable", "dev", "beta", "testing", "repository", "upgrade"],
        kind: Kind::Choice(vec![
            Choice::new("stable", "Stable"),
            Choice::new("dev", "Dev"),
        ]),
        default: Value::Text("stable".into()),
        scope: Scope::System,
        applies: Applies::NextUpdate,
    }]
}

/// The channel the repositories file follows.
pub fn get(env: &Env) -> Result<Value, String> {
    let path = env.system(REPOSITORIES);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    Ok(Value::Text(
        Channel::of_repositories(&text)
            .unwrap_or(Channel::Stable)
            .name()
            .into(),
    ))
}

/// Follow a channel, as root: trust its key before its index can be read,
/// and follow stable before distrusting dev's.
pub fn set(env: &Env, value: Option<&Value>) -> Result<(), String> {
    let to: Channel = value.and_then(Value::as_text).unwrap_or("stable").parse()?;
    if let Some(key) = to.opt_in_key() {
        let shipped = env
            .root
            .join(Channel::SHIPPED_KEYS.trim_start_matches('/'))
            .join(key);
        let bytes = std::fs::read(&shipped).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                format!(
                    "{} is missing; upgrade alpymist-keys first",
                    shipped.display()
                )
            } else {
                crate::io_error(&shipped, &e)
            }
        })?;
        let installed = env.system(APK_KEYS).join(key);
        crate::generated::replace(&installed, &String::from_utf8_lossy(&bytes))?;
    }
    let path = env.system(REPOSITORIES);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    let rewritten = to.rewrite_repositories(&text);
    if rewritten != text {
        crate::generated::replace(&path, &rewritten)?;
    }
    for other in Channel::ALL.into_iter().filter(|&c| c != to) {
        if let Some(key) = other.opt_in_key() {
            let installed = env.system(APK_KEYS).join(key);
            match std::fs::remove_file(&installed) {
                Err(e) if e.kind() != ErrorKind::NotFound => {
                    return Err(crate::io_error(&installed, &e));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

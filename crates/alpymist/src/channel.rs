//! `alpymist channel`: which Alpymist repository this system follows.
//!
//! Switching writes the repositories file and trusts or distrusts dev's key;
//! then apk upgrades as it always does. Leaving dev passes `--available`,
//! because dev's packages are versioned above stable's and apk would otherwise
//! keep them (ADR 0006).

use alpymist_core::Channel;
use std::error::Error;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPOSITORIES: &str = "/etc/apk/repositories";
const APK_KEYS: &str = "/etc/apk/keys";
const APK: &str = "/sbin/apk";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Say which channel the system follows.
pub fn show() -> Result<()> {
    let text = read(Path::new(REPOSITORIES))?;
    match Channel::of_repositories(&text) {
        Some(c) => println!("{c}"),
        None => println!("none: {REPOSITORIES} has no Alpymist repository"),
    }
    Ok(())
}

/// Follow `to`, then upgrade unless told not to.
pub fn switch(to: Channel, upgrade: bool) -> Result<()> {
    let path = Path::new(REPOSITORIES);
    let text = read(path)?;
    let from = Channel::of_repositories(&text);

    // Trust before following, and follow stable before distrusting, so apk
    // never reads an index signed with a key it does not have.
    if let Some(key) = to.opt_in_key() {
        let shipped = Path::new(Channel::SHIPPED_KEYS).join(key);
        if !shipped.is_file() {
            return Err(format!(
                "{} is missing; upgrade alpymist-keys first",
                shipped.display()
            )
            .into());
        }
        write(&Path::new(APK_KEYS).join(key), &std::fs::read(&shipped)?)?;
    }
    let rewritten = to.rewrite_repositories(&text);
    if rewritten != text {
        write(path, rewritten.as_bytes())?;
    }
    for channel in Channel::ALL.into_iter().filter(|&c| c != to) {
        if let Some(key) = channel.opt_in_key() {
            match std::fs::remove_file(Path::new(APK_KEYS).join(key)) {
                Err(e) if e.kind() != ErrorKind::NotFound => return Err(denied(APK_KEYS, &e)),
                _ => {}
            }
        }
    }
    println!("following {to}: {}", to.repository());

    if !upgrade {
        let available = if to == Channel::Stable {
            " --available"
        } else {
            ""
        };
        println!("not upgraded; run: apk upgrade --update-cache{available}");
        return Ok(());
    }
    let mut apk = Command::new(APK);
    apk.args(["--update-cache", "upgrade"]);
    if to == Channel::Stable && from != Some(Channel::Stable) {
        apk.arg("--available");
    }
    let status = apk.status().map_err(|e| format!("{APK}: {e}"))?;
    if !status.success() {
        return Err(format!("apk upgrade failed ({status})").into());
    }
    Ok(())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()).into())
}

/// Replace a file in one step, so apk never sees half of it.
fn write(path: &Path, contents: &[u8]) -> Result<()> {
    let mut tmp = PathBuf::from(path);
    tmp.as_mut_os_string().push(".alpymist");
    std::fs::write(&tmp, contents)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| denied(&path.display().to_string(), &e))
}

fn denied(what: &str, e: &std::io::Error) -> Box<dyn Error> {
    if e.kind() == ErrorKind::PermissionDenied {
        format!("{what}: permission denied; run it as root, with doas").into()
    } else {
        format!("{what}: {e}").into()
    }
}

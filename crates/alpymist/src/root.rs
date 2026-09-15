//! Running this command again as root, through pkexec.
//!
//! A system setting can only be written by root. Rather than asking for doas,
//! `alpymist` runs itself again as `pkexec /usr/bin/alpymist ARGS`; polkit's
//! `org.alpymist.settings` actions ask for an administrator's password, in a
//! terminal or in Alpymist's own dialog, and pkexec gives the command a clean
//! environment.
//!
//! Run that way, it only does what needs root: `PKEXEC_UID` says so, and
//! [`refuse_account_settings`] turns away anything else.

use std::error::Error;
use std::process::Command;

/// Where this command is installed, which polkit's policy names.
const INSTALLED: &str = "/usr/bin/alpymist";

/// Run `alpymist ARGS` as root and wait for it.
///
/// # Errors
/// pkexec could not run, the password was refused, or the command failed.
pub fn again(args: &[&str]) -> Result<(), Box<dyn Error>> {
    let status = Command::new("pkexec")
        .arg(INSTALLED)
        .args(args)
        .status()
        .map_err(|e| format!("this needs root, and pkexec could not run: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        // pkexec's own: the password dialog was dismissed, or polkit said no.
        Some(126) => Err("cancelled".into()),
        Some(127) => Err("not allowed: this needs an administrator's password".into()),
        _ => Err(format!("alpymist as root failed ({status})").into()),
    }
}

/// Whether this process was started by pkexec on someone's behalf.
pub fn through_pkexec() -> bool {
    std::env::var_os("PKEXEC_UID").is_some()
}

/// Root through pkexec writes system settings only: an account setting as
/// root would land in root's home, which is never what was meant.
///
/// # Errors
/// `setting` is an account setting and this is pkexec's root.
pub fn refuse_account_settings(setting: &alpymist_settings::Setting) -> Result<(), Box<dyn Error>> {
    if through_pkexec() && setting.scope != alpymist_settings::Scope::System {
        return Err(format!(
            "{} is an account setting; run alpymist without pkexec",
            setting.id
        )
        .into());
    }
    Ok(())
}

//! `alpymist guest`: graphics for Alpymist in a virtual machine.
//!
//! The switch is the `updates.guest-graphics` setting's; this adds the
//! upgrade after it. Turning it off passes `--available`, because the guest
//! repository's Mesa is versioned above Alpine's and apk would otherwise keep
//! it (ADR 0018).

use crate::root;
use alpymist_core::guest;
use alpymist_settings::{Env, Settings, Value};
use std::error::Error;
use std::process::Command;

const APK: &str = "/sbin/apk";
const ID: &str = "updates.guest-graphics";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Say whether the system follows the guest repository, and whether this
/// machine is one it is for.
pub fn show(settings: &Settings, env: &Env) -> Result<()> {
    let on = settings.get(env, ID)? == Value::Bool(true);
    println!("{}", if on { "on" } else { "off" });
    if !on && alpymist_hwprobe::probe().is_ok_and(|caps| guest::offered(&caps)) {
        eprintln!("this is a virtual machine with virtio-gpu: `alpymist guest on` is for it");
    }
    Ok(())
}

/// Follow the guest repository or stop, then upgrade unless told not to. Not
/// root: run again through pkexec.
pub fn switch(settings: &Settings, env: &Env, on: bool, upgrade: bool) -> Result<()> {
    let word = if on { "on" } else { "off" };
    if !env.is_root {
        let mut args = vec!["guest", word];
        if !upgrade {
            args.push("--no-upgrade");
        }
        return root::again(&args);
    }
    settings.set(env, ID, word, false)?;
    if on {
        println!("following {}", guest::REPOSITORY);
    } else {
        println!("no longer following {}", guest::REPOSITORY);
    }

    let available = if on { "" } else { " --available" };
    if !upgrade {
        println!("not upgraded; run: apk upgrade --update-cache{available}");
        return Ok(());
    }
    let mut apk = Command::new(APK);
    apk.args(["--update-cache", "upgrade"]);
    if !on {
        apk.arg("--available");
    }
    let status = apk.status().map_err(|e| format!("{APK}: {e}"))?;
    if !status.success() {
        return Err(format!("apk upgrade failed ({status})").into());
    }
    println!("log out and in again for the desktop to use it");
    Ok(())
}

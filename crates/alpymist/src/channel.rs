//! `alpymist channel`: which Alpymist repository this system follows.
//!
//! The switch is the `updates.channel` setting's; this adds the upgrade after
//! it. Leaving dev passes `--available`, because dev's packages are versioned
//! above stable's and apk would otherwise keep them (ADR 0006).

use crate::root;
use alpymist_core::Channel;
use alpymist_settings::{Env, Settings, Value};
use std::error::Error;
use std::process::Command;

const APK: &str = "/sbin/apk";
const ID: &str = "updates.channel";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Say which channel the system follows.
pub fn show(settings: &Settings, env: &Env) -> Result<()> {
    println!("{}", settings.get(env, ID)?);
    Ok(())
}

/// Follow `to`, then upgrade unless told not to. Not root: run again through
/// pkexec.
pub fn switch(settings: &Settings, env: &Env, to: Channel, upgrade: bool) -> Result<()> {
    if !env.is_root {
        let mut args = vec!["channel", to.name()];
        if !upgrade {
            args.push("--no-upgrade");
        }
        return root::again(&args);
    }
    let from = settings.get(env, ID).ok();
    settings.set(env, ID, to.name(), false)?;
    println!("following {to}: {}", to.repository());

    let leaving_dev = to == Channel::Stable && from != Some(Value::Text("stable".into()));
    if !upgrade {
        let available = if leaving_dev { " --available" } else { "" };
        println!("not upgraded; run: apk upgrade --update-cache{available}");
        return Ok(());
    }
    let mut apk = Command::new(APK);
    apk.args(["--update-cache", "upgrade"]);
    if leaving_dev {
        apk.arg("--available");
    }
    let status = apk.status().map_err(|e| format!("{APK}: {e}"))?;
    if !status.success() {
        return Err(format!("apk upgrade failed ({status})").into());
    }
    Ok(())
}

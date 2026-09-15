//! Wi-Fi on or off, through iwd, as the Wi-Fi popup switches it.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_wifi::iwd::Iwd;
use alpymist_wifi::model::Radio;

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: "wifi.enabled",
        title: "Wi-Fi",
        description: "Turn the wireless adapter on or off.",
        keywords: &["wireless", "wlan", "airplane", "radio"],
        kind: Kind::Switch,
        default: Value::Bool(true),
        scope: Scope::Account,
        applies: Applies::Now,
    }]
}

/// Whether the adapter is on.
pub fn get(env: &Env) -> Result<Value, String> {
    if env.root != std::path::Path::new("/") {
        return Err("Wi-Fi is only read on the running system".into());
    }
    match Iwd::connect()?.state().radio {
        Radio::On => Ok(Value::Bool(true)),
        Radio::Off => Ok(Value::Bool(false)),
        Radio::NoAdapter => Err("there is no Wi-Fi adapter".into()),
        Radio::NoDaemon => Err("iwd is not running".into()),
    }
}

/// Turn it on or off.
pub fn set(env: &Env, value: Option<&Value>) -> Result<(), String> {
    if env.root != std::path::Path::new("/") {
        return Err("Wi-Fi is only changed on the running system".into());
    }
    let on = value.and_then(Value::as_bool).unwrap_or(true);
    Iwd::connect()?.set_powered(on)
}

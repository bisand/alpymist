//! Bluetooth: bluetoothd, off until someone turns it on (ADR 0011).
//!
//! bluez ships with the desktop, but nothing that answers the radio runs
//! until this is on: then bluetoothd starts now and at every boot, powering
//! the adapter as bluez does by default, neither discoverable nor always
//! pairable. Off stops it and takes it out of the default runlevel, and the
//! adapter goes dark with it.
//!
//! Pairing and connecting are bluetuith's, in a terminal: the page's
//! "Devices…" button, the menu and a click on the bar's icon all open it.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};

/// The switch's id.
pub const ID: &str = "bluetooth.enabled";
/// bluetoothd's link in the default runlevel.
pub const RUNLEVEL: &str = "etc/runlevels/default/bluetooth";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: ID,
        title: "Bluetooth",
        description: "Turn Bluetooth on, now and at every start, to use headphones, \
                      keyboards and other devices. Other devices cannot see this \
                      computer unless you make it discoverable while pairing.",
        keywords: &[
            "bluez",
            "bluetoothd",
            "wireless",
            "headphones",
            "headset",
            "radio",
        ],
        kind: Kind::Switch,
        default: Value::Bool(false),
        scope: Scope::System,
        applies: Applies::Now,
    }]
}

/// Whether bluetoothd starts at boot.
pub fn get(env: &Env) -> Value {
    Value::Bool(env.system(RUNLEVEL).symlink_metadata().is_ok())
}

/// Start bluetoothd now and at every boot, or stop it and take it out, as
/// root.
pub fn set(env: &Env, value: Option<&Value>) -> Result<(), String> {
    if value.and_then(Value::as_bool).unwrap_or(false) {
        // A radio blocked in software cannot be powered; one blocked by a
        // switch on the machine stays blocked, which this cannot change.
        let _ = env.run(&["rfkill", "unblock", "bluetooth"]);
        env.run(&["rc-update", "add", "bluetooth", "default"])?;
        return env.run(&["rc-service", "bluetooth", "start"]).map(drop);
    }
    env.run(&["rc-service", "--ifstarted", "bluetooth", "stop"])?;
    // rc-update fails for a service that is not in the runlevel.
    if env.system(RUNLEVEL).symlink_metadata().is_ok() {
        env.run(&["rc-update", "del", "bluetooth", "default"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ID, RUNLEVEL};
    use crate::env::Env;
    use crate::{Error, Settings, Value};
    use std::sync::Mutex;

    #[test]
    fn bluetooth_is_bluetoothd_at_boot_and_off_until_asked() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-bluetooth-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let settings = Settings::new();
        assert_eq!(settings.find(ID).unwrap().default, Value::Bool(false));
        assert_eq!(
            settings.set(&Env::test(&d, false, &RAN), ID, "on", false),
            Err(Error::NeedsRoot(ID.into()))
        );

        let env = Env::test(&d, true, &RAN);
        assert_eq!(settings.get(&env, ID), Ok(Value::Bool(false)));
        settings.set(&env, ID, "on", false).unwrap();
        let link = env.system(RUNLEVEL);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "").unwrap();
        assert_eq!(settings.get(&env, ID), Ok(Value::Bool(true)));
        settings.set(&env, ID, "off", false).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "rfkill unblock bluetooth",
                "rc-update add bluetooth default",
                "rc-service bluetooth start",
                "rc-service --ifstarted bluetooth stop",
                "rc-update del bluetooth default",
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

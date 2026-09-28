//! The SSH server: OpenSSH's `sshd`, started now and at every boot, or not at
//! all.
//!
//! Off until someone turns it on (ADR 0011), and `openssh-server` is not even
//! installed until then: turning it on installs it. On is sshd in the default
//! runlevel, which is what the value is read from, since that is what decides
//! whether it is there after the next boot.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};

/// The server itself.
pub const SSHD: &str = "usr/sbin/sshd";
/// Its init script.
pub const SERVICE: &str = "etc/init.d/sshd";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: "ssh.server",
        title: "SSH server",
        description: "Let the accounts on this computer log in to it from another one, \
                      over the network. Installed the first time it is turned on.",
        keywords: &["sshd", "openssh", "remote", "login", "terminal"],
        kind: Kind::Switch,
        default: Value::Bool(false),
        scope: Scope::System,
        applies: Applies::Now,
    }]
}

/// Whether sshd starts at boot.
pub fn get(env: &Env) -> Value {
    Value::Bool(crate::service::at_boot(env, "sshd"))
}

/// Turn it on, installing it first if it has to be, or off. Returns where it
/// can be reached, when that is known.
pub fn set(env: &Env, value: Option<&Value>) -> Result<Vec<String>, String> {
    let on = value.and_then(Value::as_bool).unwrap_or(false);
    if !on {
        crate::service::stop(env, "sshd")?;
        return Ok(Vec::new());
    }
    if !env.system(SSHD).exists() || !env.system(SERVICE).exists() {
        env.run(&["apk", "add", "openssh-server"])?;
    }
    crate::service::start(env, "sshd")?;
    Ok(address(env)
        .map(|a| format!("Reach it at {a}, port 22."))
        .into_iter()
        .collect())
}

/// The address the network would see this computer at: the source of its
/// route to the outside world, if it has one.
fn address(env: &Env) -> Option<String> {
    let route = env.run(&["ip", "-4", "route", "get", "1.1.1.1"]).ok()?;
    source(&route)
}

/// The word after `src` in `ip route get`'s answer.
fn source(route: &str) -> Option<String> {
    let mut words = route.split_whitespace();
    words.find(|&w| w == "src")?;
    words.next().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{get, set, source};
    use crate::env::Env;
    use crate::model::Value;
    use std::sync::Mutex;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("alpymist-ssh-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn the_address_is_the_routes_source() {
        assert_eq!(
            source("1.1.1.1 via 192.168.1.1 dev wlan0  src 192.168.1.23  uid 1000\n"),
            Some("192.168.1.23".into())
        );
        assert_eq!(source(""), None);
    }

    #[test]
    fn on_installs_it_when_missing_and_starts_it_at_boot() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("on");
        let env = Env::test(&d, true, &RAN);
        assert_eq!(get(&env), Value::Bool(false));
        set(&env, Some(&Value::Bool(true))).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "apk add openssh-server",
                "rc-update add sshd default",
                "rc-service sshd start",
                "ip -4 route get 1.1.1.1",
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn off_stops_it_and_takes_it_out_of_the_runlevel_only_when_there() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("off");
        let env = Env::test(&d, true, &RAN);
        set(&env, None).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            ["rc-service --ifstarted sshd stop"]
        );

        RAN.lock().unwrap().clear();
        let link = crate::service::link(&env, "sshd");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "").unwrap();
        assert_eq!(get(&env), Value::Bool(true));
        set(&env, Some(&Value::Bool(false))).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "rc-service --ifstarted sshd stop",
                "rc-update del sshd default"
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

//! The release channel: which Alpymist repository apk follows.
//!
//! A channel is the one Alpymist line in `/etc/apk/repositories` and, for dev,
//! the key its index is signed with (ADR 0006). Setting it switches both;
//! upgrading to it is apk's, as `alpymist channel` does after switching.
//!
//! Guest graphics is a line and a key of the same kind, beside the channel's
//! and independent of it: a Mesa with the driver for a virtual machine's 3D
//! (ADR 0018). `alpymist guest` upgrades after switching.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::{Channel, guest};
use std::io::ErrorKind;

/// apk's repositories.
pub const REPOSITORIES: &str = "etc/apk/repositories";
/// Where apk looks for keys.
pub const APK_KEYS: &str = "etc/apk/keys";

/// The guest graphics setting.
pub const GUEST: &str = "updates.guest-graphics";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![
        channel(),
        Setting {
            id: GUEST,
            title: "Graphics for a virtual machine",
            description: "In a virtual machine, draw with the host's graphics card: \
                          a second build of Mesa, from a repository of its own.",
            keywords: &[
                "vm", "virtual", "qemu", "utm", "virgl", "mesa", "guest", "3d",
            ],
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::NextUpdate,
        },
    ]
}

fn channel() -> Setting {
    Setting {
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
    }
}

/// The channel the repositories file follows, or whether it follows the
/// guest repository.
pub fn get(env: &Env, s: &Setting) -> Result<Value, String> {
    let path = env.system(REPOSITORIES);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    if s.id == GUEST {
        return Ok(Value::Bool(guest::follows(&text)));
    }
    Ok(Value::Text(
        Channel::of_repositories(&text)
            .unwrap_or(Channel::Stable)
            .name()
            .into(),
    ))
}

/// Follow a channel, as root: trust its key before its index can be read,
/// and follow stable before distrusting dev's.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<(), String> {
    if s.id == GUEST {
        return set_guest(env, value.and_then(Value::as_bool).unwrap_or(false));
    }
    let to: Channel = value.and_then(Value::as_text).unwrap_or("stable").parse()?;
    if let Some(key) = to.opt_in_key() {
        trust(env, key)?;
    }
    let path = env.system(REPOSITORIES);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    let rewritten = to.rewrite_repositories(&text);
    if rewritten != text {
        crate::generated::replace(&path, &rewritten)?;
    }
    for other in Channel::ALL.into_iter().filter(|&c| c != to) {
        if let Some(key) = other.opt_in_key() {
            distrust(env, key)?;
        }
    }
    Ok(())
}

/// Follow the guest repository or stop, as root, in the order the channel
/// keeps: its key trusted before its index can be read, and its line gone
/// before the key is.
fn set_guest(env: &Env, on: bool) -> Result<(), String> {
    if on {
        trust(env, guest::KEY)?;
    }
    let path = env.system(REPOSITORIES);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    let rewritten = guest::rewrite_repositories(&text, on);
    if rewritten != text {
        crate::generated::replace(&path, &rewritten)?;
    }
    if !on {
        distrust(env, guest::KEY)?;
    }
    Ok(())
}

/// Copy a key `alpymist-keys` ships to where apk looks.
fn trust(env: &Env, key: &str) -> Result<(), String> {
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
    crate::generated::replace(&installed, &String::from_utf8_lossy(&bytes))
}

/// Take a key away from where apk looks. Not there is done.
fn distrust(env: &Env, key: &str) -> Result<(), String> {
    let installed = env.system(APK_KEYS).join(key);
    match std::fs::remove_file(&installed) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(crate::io_error(&installed, &e)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{APK_KEYS, GUEST, REPOSITORIES};
    use crate::env::Env;
    use crate::{Settings, Value};
    use alpymist_core::{Channel, guest};
    use std::sync::Mutex;

    const INSTALLED: &str = "https://dl-cdn.alpinelinux.org/alpine/v3.24/main\n\
                             https://pkgs.alpymist.org/v3.24/alpymist\n";

    /// A system as installed, under the root `Env::test` gives a directory:
    /// its repositories, and the keys alpymist-keys ships outside apk's
    /// directory.
    fn system(name: &str, ran: &'static Mutex<Vec<String>>) -> (Env, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("alpymist-updates-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, true, ran);
        let d = env.root.clone();
        let shipped = d.join(Channel::SHIPPED_KEYS.trim_start_matches('/'));
        std::fs::create_dir_all(&shipped).unwrap();
        std::fs::write(shipped.join(guest::KEY), "guest key\n").unwrap();
        std::fs::create_dir_all(d.join("etc/apk")).unwrap();
        std::fs::write(d.join(REPOSITORIES), INSTALLED).unwrap();
        (env, d)
    }

    #[test]
    fn guest_graphics_is_off_until_asked_for() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let (env, d) = system("default", &RAN);
        assert_eq!(Settings::new().get(&env, GUEST), Ok(Value::Bool(false)));
        assert!(!d.join(APK_KEYS).join(guest::KEY).exists());
    }

    #[test]
    fn turning_guest_graphics_on_trusts_its_key_and_off_distrusts_it() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let (env, d) = system("switch", &RAN);
        let settings = Settings::new();
        let key = d.join(APK_KEYS).join(guest::KEY);
        let repositories = || std::fs::read_to_string(d.join(REPOSITORIES)).unwrap();

        settings.set(&env, GUEST, "on", false).unwrap();
        assert_eq!(settings.get(&env, GUEST), Ok(Value::Bool(true)));
        assert_eq!(
            repositories(),
            format!("{INSTALLED}{}\n", guest::REPOSITORY)
        );
        assert_eq!(std::fs::read_to_string(&key).unwrap(), "guest key\n");
        // The channel is its own line and stays where it was.
        assert_eq!(
            settings.get(&env, "updates.channel"),
            Ok(Value::Text("stable".into()))
        );

        settings.set(&env, GUEST, "off", false).unwrap();
        assert_eq!(settings.get(&env, GUEST), Ok(Value::Bool(false)));
        assert_eq!(repositories(), INSTALLED);
        assert!(
            !key.exists(),
            "apk would still trust it for every repository"
        );
    }

    #[test]
    fn without_the_shipped_key_nothing_is_followed() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let (env, d) = system("nokey", &RAN);
        std::fs::remove_file(
            d.join(Channel::SHIPPED_KEYS.trim_start_matches('/'))
                .join(guest::KEY),
        )
        .unwrap();
        let e = Settings::new().set(&env, GUEST, "on", false).unwrap_err();
        assert!(e.to_string().contains("alpymist-keys"), "{e}");
        assert_eq!(
            std::fs::read_to_string(d.join(REPOSITORIES)).unwrap(),
            INSTALLED
        );
    }
}

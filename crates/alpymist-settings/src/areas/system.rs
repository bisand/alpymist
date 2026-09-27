//! The computer itself: what it is called.
//!
//! The host name is `/etc/hostname`, which the `hostname` service reads
//! at boot, set on the running system at once with `hostname -F`. The
//! installer also wrote it into `/etc/network/interfaces`, as the name the
//! wired connection gives the network's DHCP server; a line there that names
//! the old host name is changed with it, and nothing else in that file is.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, TextRule, Value};

/// The host name's id.
pub const HOSTNAME_ID: &str = "system.hostname";
/// The host name.
pub const HOSTNAME: &str = "etc/hostname";
/// ifupdown's interfaces, where DHCP is told the host name.
pub const INTERFACES: &str = "etc/network/interfaces";
/// What the `hostname` service calls a computer whose `/etc/hostname` is missing.
const UNNAMED: &str = "localhost";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: HOSTNAME_ID,
        title: "Computer name",
        description: "What this computer is called on the network, in the terminal's prompt \
                      and on the login screen.",
        keywords: &["hostname", "host name", "network name", "device name"],
        // The kernel keeps at most 64 characters of it.
        kind: Kind::Text {
            max: 64,
            rule: TextRule::Hostname,
        },
        // What the installer suggests.
        default: Value::Text("alpymist".into()),
        scope: Scope::System,
        applies: Applies::Now,
    }]
}

/// The name in `/etc/hostname`.
pub fn get(env: &Env) -> Result<Value, String> {
    let path = env.system(HOSTNAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Value::Text(
            text.lines().next().unwrap_or_default().trim().to_owned(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Text(UNNAMED.into())),
        Err(e) => Err(crate::io_error(&path, &e)),
    }
}

/// Name the computer, as root: the file, the interfaces' DHCP lines that
/// named it, then the running system.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let name = value
        .unwrap_or(&setting.default)
        .as_text()
        .unwrap_or_default()
        .to_owned();
    let old = get(env)?;
    let old = old.as_text().unwrap_or_default();

    let interfaces = env.system(INTERFACES);
    if let Ok(text) = std::fs::read_to_string(&interfaces)
        && let Some(renamed) = rename(&text, old, &name)
    {
        crate::generated::replace(&interfaces, &renamed)?;
    }
    let path = env.system(HOSTNAME);
    crate::generated::replace(&path, &format!("{name}\n"))?;
    let file = path.to_string_lossy();
    env.run(&["hostname", "-F", &file]).map(drop)
}

/// `interfaces` with each `hostname OLD` option naming `new` instead, keeping
/// its indentation; `None` when there is none.
fn rename(interfaces: &str, old: &str, new: &str) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = interfaces
        .lines()
        .map(|line| {
            let body = line.trim_start();
            let mut words = body.split_whitespace();
            if words.next() == Some("hostname")
                && words.next() == Some(old)
                && words.next().is_none()
            {
                changed = true;
                format!("{}hostname {new}", &line[..line.len() - body.len()])
            } else {
                line.to_owned()
            }
        })
        .collect();
    changed.then(|| {
        let mut out = lines.join("\n");
        if interfaces.ends_with('\n') {
            out.push('\n');
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::{HOSTNAME, HOSTNAME_ID, INTERFACES, rename};
    use crate::env::Env;
    use crate::{Error, Settings, Value};
    use std::sync::Mutex;

    /// What the installer writes for a wired connection.
    const INSTALLED: &str = "auto lo\niface lo inet loopback\n\n\
                             auto eth0\niface eth0 inet dhcp\n\thostname alpymist\n";

    #[test]
    fn only_the_dhcp_line_naming_the_old_name_changes() {
        assert_eq!(
            rename(INSTALLED, "alpymist", "x1").as_deref(),
            Some(
                "auto lo\niface lo inet loopback\n\n\
                 auto eth0\niface eth0 inet dhcp\n\thostname x1\n"
            )
        );
        assert_eq!(rename(INSTALLED, "someone-else", "x1"), None);
        assert_eq!(
            rename(
                "iface eth0 inet dhcp\n  hostname alpymist # mine\n",
                "alpymist",
                "x1"
            ),
            None
        );
    }

    #[test]
    fn a_new_name_goes_to_the_file_the_network_and_the_running_system() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-hostname-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let settings = Settings::new();
        assert_eq!(
            settings.get(&Env::test(&d, false, &RAN), HOSTNAME_ID),
            Ok(Value::Text("localhost".into()))
        );
        assert_eq!(
            settings.set(&Env::test(&d, false, &RAN), HOSTNAME_ID, "x1", false),
            Err(Error::NeedsRoot(HOSTNAME_ID.into()))
        );

        let env = Env::test(&d, true, &RAN);
        std::fs::create_dir_all(env.system("etc/network")).unwrap();
        std::fs::write(env.system(HOSTNAME), "alpymist\n").unwrap();
        std::fs::write(env.system(INTERFACES), INSTALLED).unwrap();
        assert!(settings.set(&env, HOSTNAME_ID, "my laptop", false).is_err());
        settings.set(&env, HOSTNAME_ID, "x1", false).unwrap();
        assert_eq!(
            settings.get(&env, HOSTNAME_ID),
            Ok(Value::Text("x1".into()))
        );
        assert!(
            std::fs::read_to_string(env.system(INTERFACES))
                .unwrap()
                .contains("\thostname x1\n")
        );
        let ran = RAN.lock().unwrap();
        assert_eq!(ran.len(), 1);
        assert!(ran[0].starts_with("hostname -F ") && ran[0].ends_with("etc/hostname"));
        std::fs::remove_dir_all(&d).ok();
    }
}

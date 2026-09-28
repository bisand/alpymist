//! The computer itself: what it is called, and who looks after it.
//!
//! The host name is `/etc/hostname`, which the `hostname` service reads
//! at boot, set on the running system at once with `hostname -F`. The
//! installer also wrote it into `/etc/network/interfaces`, as the name the
//! wired connection gives the network's DHCP server; a line there that names
//! the old host name is changed with it, and nothing else in that file is.
//!
//! A password is never a setting's value: that travels as an argument, which
//! anyone can read in the process list. Changing it opens a terminal running
//! `passwd`, which asks for the old one and the new one itself.
//!
//! An administrator is an account in `wheel`, which is who polkit and doas
//! ask. The switch is for the account asking, which as root is the one pkexec
//! or doas names. The last administrator is never taken out: root's password
//! is locked, so nobody could look after the computer again.

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
/// Changing the password's id.
pub const PASSWORD_ID: &str = "system.password";
/// Being an administrator's id.
pub const ADMINISTRATOR_ID: &str = "system.administrator";
/// The groups, `wheel` among them.
pub const GROUP: &str = "etc/group";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![
        Setting {
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
        },
        Setting {
            id: PASSWORD_ID,
            title: "Password",
            description: "Change the password this account logs in and unlocks with, \
                          in a terminal that asks for the old one first.",
            keywords: &["passwd", "login", "unlock", "change password"],
            kind: Kind::Action { label: "Change…" },
            default: Value::Text(String::new()),
            scope: Scope::Account,
            applies: Applies::Now,
        },
        Setting {
            id: ADMINISTRATOR_ID,
            title: "Administrator",
            description: "Let this account change settings for everyone and install software. \
                          The last administrator cannot be turned off.",
            keywords: &["admin", "wheel", "doas", "sudo", "root", "privileges"],
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::NextLogin,
        },
    ]
}

/// Its value.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    match setting.id {
        // Nothing to read: it is a thing to do, not a thing to be.
        PASSWORD_ID => Ok(Value::Text(String::new())),
        ADMINISTRATOR_ID => {
            let user = user(env)?;
            Ok(Value::Bool(wheel(env)?.iter().any(|m| m == user)))
        }
        _ => hostname(env),
    }
}

/// Set one, or reset it with `None`.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    match setting.id {
        PASSWORD_ID => change_password(),
        ADMINISTRATOR_ID => administrator(env, value.and_then(Value::as_bool).unwrap_or(false)),
        _ => name(env, setting, value),
    }
}

/// The account a setting about an account is for.
fn user(env: &Env) -> Result<&str, String> {
    env.user
        .as_deref()
        .ok_or_else(|| "which account? Run this as the account, not as root".to_owned())
}

/// The members of `wheel`.
fn wheel(env: &Env) -> Result<Vec<String>, String> {
    let path = env.system(GROUP);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    Ok(members(&text, "wheel"))
}

/// The members of `group` in the text of `/etc/group`.
fn members(groups: &str, group: &str) -> Vec<String> {
    groups
        .lines()
        .find_map(|l| {
            let mut fields = l.split(':');
            (fields.next() == Some(group)).then(|| fields.nth(2).unwrap_or_default())
        })
        .map(|list| {
            list.split(',')
                .filter(|m| !m.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Put the account in `wheel` or take it out, as root; never the last one
/// who can use it, root not counting, since its password is locked.
fn administrator(env: &Env, on: bool) -> Result<(), String> {
    let user = user(env)?;
    let members = wheel(env)?;
    let member = members.iter().any(|m| m == user);
    if on && !member {
        return env.run(&["addgroup", user, "wheel"]).map(drop);
    }
    if !on && member {
        if !members.iter().any(|m| m != user && m != "root") {
            return Err(format!(
                "{user} is the only administrator. Make another account one first: \
                 without one, nobody could change this computer's settings or software."
            ));
        }
        return env.run(&["delgroup", user, "wheel"]).map(drop);
    }
    Ok(())
}

/// Open a terminal for `passwd`, and leave it: it asks, and it says whether
/// it worked, before the window goes. The terminal is the one chosen in
/// Settings › Default applications.
fn change_password() -> Result<(), String> {
    let command = [
        "sh".to_owned(),
        "-c".to_owned(),
        "passwd; printf '\\nPress Enter to close. '; read -r _".to_owned(),
    ];
    let argv = alpymist_core::defaults::terminal_argv(
        &alpymist_core::defaults::Places::current(),
        &command,
    );
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .spawn()
        .map(drop)
        .map_err(|e| format!("{}: {e}", argv[0]))
}

/// The name in `/etc/hostname`.
fn hostname(env: &Env) -> Result<Value, String> {
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
fn name(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let name = value
        .unwrap_or(&setting.default)
        .as_text()
        .unwrap_or_default()
        .to_owned();
    let old = hostname(env)?;
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
    use super::{ADMINISTRATOR_ID, GROUP, HOSTNAME, HOSTNAME_ID, INTERFACES, members, rename};
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
    fn a_groups_members_are_read_from_its_line() {
        let groups = "root:x:0:root\nwheel:x:10:root,someone,other\nvideo:x:27:\n";
        assert_eq!(members(groups, "wheel"), ["root", "someone", "other"]);
        assert!(members(groups, "video").is_empty());
        assert!(members(groups, "missing").is_empty());
    }

    #[test]
    fn the_last_administrator_stays_one() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-admin-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, true, &RAN);
        let settings = Settings::new();
        std::fs::create_dir_all(env.system("etc")).unwrap();
        let group = env.system(GROUP);

        // The account and root: root does not count, so it is the last.
        std::fs::write(&group, "wheel:x:10:root,someone\n").unwrap();
        assert_eq!(settings.get(&env, ADMINISTRATOR_ID), Ok(Value::Bool(true)));
        let refused = settings.set(&env, ADMINISTRATOR_ID, "off", false);
        assert!(
            matches!(refused, Err(Error::Failed(ref m)) if m.contains("only administrator")),
            "{refused:?}"
        );
        assert!(settings.reset(&env, ADMINISTRATOR_ID, false).is_err());

        // With another, it can go.
        std::fs::write(&group, "wheel:x:10:root,someone,other\n").unwrap();
        settings.set(&env, ADMINISTRATOR_ID, "off", false).unwrap();
        // Not one: made one; already one: nothing to do.
        std::fs::write(&group, "wheel:x:10:root,other\n").unwrap();
        assert_eq!(settings.get(&env, ADMINISTRATOR_ID), Ok(Value::Bool(false)));
        settings.set(&env, ADMINISTRATOR_ID, "on", false).unwrap();
        std::fs::write(&group, "wheel:x:10:root,other,someone\n").unwrap();
        settings.set(&env, ADMINISTRATOR_ID, "on", false).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            ["delgroup someone wheel", "addgroup someone wheel"]
        );
        std::fs::remove_dir_all(&d).ok();
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

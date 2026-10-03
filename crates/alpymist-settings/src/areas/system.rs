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
//! The hardware report is `alpymist report --issue` in a terminal, started by
//! its button and by nothing else: it shows the report and asks before it
//! opens anything, and what it opens posts nothing by itself (ADR 0020).
//!
//! An administrator is an account in `wheel`, which is who polkit and doas
//! ask. The switch is for the account asking, which as root is the one pkexec
//! or doas names. The last administrator is never taken out: root's password
//! is locked, so nobody could look after the computer again.
//!
//! A fingerprint is off everywhere until it is turned on here (ADR 0016), and
//! each place it can be used has a switch of its own. Each is a PAM change
//! and nothing else: the lock screen's own fingerprint service,
//! [`LOCK_FINGERPRINT`], whose being there is the switch; one `pam_fprintd`
//! line in polkit's, so administrator prompts take an enrolled finger; and,
//! for logging in, [`LOGIN`]'s password stack with a finger asked for after a
//! password that was not right. The password still works everywhere. A login
//! by finger leaves the keyring locked, which its switch says: the keyring is
//! opened with the password, and a finger has none to give.

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
/// The hardware report's id.
pub const REPORT_ID: &str = "system.report";
/// Being an administrator's id.
pub const ADMINISTRATOR_ID: &str = "system.administrator";
/// The groups, `wheel` among them.
pub const GROUP: &str = "etc/group";
/// Unlocking the screen with a fingerprint's id.
pub const FINGERPRINT_LOCK_ID: &str = "system.fingerprint-lock";
/// Answering administrator prompts with a fingerprint's id.
pub const FINGERPRINT_PROMPTS_ID: &str = "system.fingerprint-prompts";
/// Logging in with a fingerprint's id.
pub const FINGERPRINT_LOGIN_ID: &str = "system.fingerprint-login";
/// The login screen's PAM service, as greetd's configuration names it.
pub const LOGIN: &str = "etc/pam.d/alpymist-greetd";
/// The lock screen's fingerprint service, as `alpymist-lock` names it.
pub const LOCK_FINGERPRINT: &str = "etc/pam.d/alpymist-lock-fingerprint";
/// polkit's PAM service, which administrator prompts are checked with.
pub const POLKIT: &str = "etc/pam.d/polkit-1";
/// Where linux-pam finds a distribution's own when `/etc` has none.
const POLKIT_VENDOR: &str = "usr/lib/pam.d/polkit-1";
/// `pam_fprintd`, from fprintd-pam, which alpymist-fingerprint brings.
pub const PAM_FPRINTD: &str = "usr/lib/security/pam_fprintd.so";

/// The lock screen's service: a finger, checked by the fingerprint daemon,
/// and nothing that could take a password.
const LOCK_SERVICE: &str = "\
# Written by Settings › System › Unlock the screen with a fingerprint, and removed when
# it is turned off. The lock screen asks this beside the password: an
# enrolled finger on the reader, checked by the fingerprint daemon.
auth\t\trequired\tpam_fprintd.so
";
/// What goes before the line in polkit's service, so it can be found again.
const POLKIT_NOTE: &str = "# Settings › System › Answer administrator prompts with a fingerprint: an enrolled finger, or the password";
/// The line itself. `sufficient`: a finger lets the prompt through, and
/// anything else — no reader, no finger enrolled, the reader held by the lock
/// screen, a finger not recognised — goes on to the password. The dash keeps
/// polkit working should fprintd-pam be removed while this is on.
const POLKIT_LINE: &str = "-auth\t\tsufficient\tpam_fprintd.so";
/// What polkit's note said before each place had a switch of its own; taken
/// out with the line, so a system switched on then is left clean.
const POLKIT_NOTE_BEFORE: &str =
    "# Settings › System › Unlock with a fingerprint: an enrolled finger, or the password";

/// The line of the login service that a finger takes the place of: Alpine's
/// own password stack.
const LOGIN_PASSWORD: &str = "auth\t\tinclude\t\tbase-auth";
/// What a login checks with a finger allowed: [`LOGIN_PASSWORD`]'s modules,
/// spelt out, with a finger between the password and the refusal.
///
/// The password is asked first, and a right one goes past the reader to the
/// keyring, which it opens. One that is not right — nothing typed, most of
/// all — goes on to the reader, once and for ten seconds, and a finger the
/// daemon knows goes past the refusal and past the keyring too: the keyring
/// is never offered the empty password a finger came with. The login screen sends an empty
/// password to ask for the reader, and gives up on a typed one that was
/// wrong without waiting for a finger. An account with no password logs in
/// on Enter, as it did. `pam_permit` is there because a line that jumps says
/// nothing of its own, and a stack where nothing said yes is a no. The dash
/// keeps logging in by password working should fprintd-pam be removed while
/// this is on.
///
/// This is `base-auth` spelt out, and must go on saying what it says: a test
/// holds it to Alpine's file as it was when this was written.
const LOGIN_FINGER: &str = "\
# Settings › System › Log in with a fingerprint: the password, or an enrolled finger
auth\t\t[success=2 default=ignore]\tpam_unix.so nullok
-auth\t\t[success=2 default=ignore]\tpam_fprintd.so max-tries=1 timeout=10
auth\t\trequisite\tpam_deny.so
-auth\t\toptional\tpam_gnome_keyring.so
auth\t\trequired\tpam_permit.so
auth\t\trequired\tpam_nologin.so
auth\t\trequired\tpam_env.so
# Settings › System › Log in with a fingerprint: to here";

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
        Setting {
            id: FINGERPRINT_LOCK_ID,
            title: "Unlock the screen with a fingerprint",
            description: "Let an enrolled finger unlock the locked screen, beside the \
                          password. Fingers are added in Fingerprints.",
            keywords: FINGER_WORDS,
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::Now,
        },
        Setting {
            id: FINGERPRINT_PROMPTS_ID,
            title: "Answer administrator prompts with a fingerprint",
            description: "Let an enrolled finger answer the window that asks for an \
                          administrator's password, beside the password. Not doas in a \
                          terminal, which takes only the password.",
            keywords: FINGER_WORDS,
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::Now,
        },
        Setting {
            id: FINGERPRINT_LOGIN_ID,
            title: "Log in with a fingerprint",
            description: "Let an enrolled finger log in: press Enter with no password typed, \
                          then touch the reader. The keyring stays locked after such a login, \
                          so saved passwords and keys ask for the password when first used.",
            keywords: FINGER_WORDS,
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::Now,
        },
        Setting {
            id: REPORT_ID,
            title: "Hardware report",
            description: "Show what this computer is made of and which drivers have it, in a \
                          terminal, and offer to open it as a GitHub issue to say what does not \
                          work. It holds no serial number, address or name, and nothing is sent \
                          unless you submit the issue.",
            keywords: &[
                "report", "hardware", "driver", "firmware", "bug", "issue", "probe", "support",
            ],
            kind: Kind::Action { label: "Show…" },
            default: Value::Text(String::new()),
            scope: Scope::Account,
            applies: Applies::Now,
        },
    ]
}

/// What finds any of the fingerprint switches.
const FINGER_WORDS: &[&str] = &[
    "fingerprint",
    "finger",
    "reader",
    "biometric",
    "fprintd",
    "touch",
];

/// Its value.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    match setting.id {
        // Nothing to read: it is a thing to do, not a thing to be.
        PASSWORD_ID | REPORT_ID => Ok(Value::Text(String::new())),
        FINGERPRINT_LOCK_ID => Ok(Value::Bool(env.system(LOCK_FINGERPRINT).exists())),
        FINGERPRINT_PROMPTS_ID => Ok(Value::Bool(has_line(env, POLKIT, POLKIT_LINE))),
        FINGERPRINT_LOGIN_ID => Ok(Value::Bool(has_line(
            env,
            LOGIN,
            LOGIN_FINGER.lines().next().unwrap_or_default(),
        ))),
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
        PASSWORD_ID => in_terminal("passwd"),
        // It prints the report, asks, and says what it did.
        REPORT_ID => in_terminal("alpymist report --issue"),
        FINGERPRINT_LOCK_ID | FINGERPRINT_PROMPTS_ID | FINGERPRINT_LOGIN_ID => {
            let on = value.and_then(Value::as_bool).unwrap_or(false);
            if on && !env.system(PAM_FPRINTD).exists() {
                return Err(
                    "Fingerprint support is not installed: add the alpymist-fingerprint \
                     package, then enrol a finger in Fingerprints."
                        .into(),
                );
            }
            match setting.id {
                FINGERPRINT_LOCK_ID => lock(env, on),
                FINGERPRINT_PROMPTS_ID => polkit(env, on),
                _ => login(env, on),
            }
        }
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

/// Whether the PAM service `service` has `line` in it.
fn has_line(env: &Env, service: &str, line: &str) -> bool {
    std::fs::read_to_string(env.system(service)).is_ok_and(|text| text.lines().any(|l| l == line))
}

/// Let a finger unlock the screen, or stop it, as root: the lock screen's
/// fingerprint service is made, or removed.
fn lock(env: &Env, on: bool) -> Result<(), String> {
    let lock = env.system(LOCK_FINGERPRINT);
    if on {
        return crate::generated::replace(&lock, LOCK_SERVICE);
    }
    match std::fs::remove_file(&lock) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(crate::io_error(&lock, &e)),
        _ => Ok(()),
    }
}

/// Let a finger log in, or stop it, as root: the login service's password
/// line becomes [`LOGIN_FINGER`], or that becomes the line again. Nothing
/// else in the service is touched, and one already as asked is not written.
fn login(env: &Env, on: bool) -> Result<(), String> {
    let path = env.system(LOGIN);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !on => return Ok(()),
        Err(e) => return Err(crate::io_error(&path, &e)),
    };
    let changed = if on {
        login_with_finger(&text)
    } else {
        login_without_finger(&text)
    };
    let first = LOGIN_FINGER.lines().next().unwrap_or_default();
    match changed {
        Some(text) => crate::generated::replace(&path, &text),
        None if on && !text.lines().any(|l| l == first) => Err(format!(
            "{} does not check the password with \"auth include base-auth\", so there \
             is nowhere known to put a fingerprint; it was left as it is",
            path.display()
        )),
        None => Ok(()),
    }
}

/// The login service with a finger allowed: its `auth include base-auth`
/// line, however it is spaced, replaced by [`LOGIN_FINGER`]. `None` when
/// there is no such line.
fn login_with_finger(service: &str) -> Option<String> {
    let password: Vec<&str> = LOGIN_PASSWORD.split_whitespace().collect();
    let mut found = false;
    let lines: Vec<&str> = service
        .lines()
        .map(|line| {
            if !found && line.split_whitespace().eq(password.iter().copied()) {
                found = true;
                LOGIN_FINGER
            } else {
                line
            }
        })
        .collect();
    found.then(|| lines.join("\n") + "\n")
}

/// The login service with the password alone again: everything from
/// [`LOGIN_FINGER`]'s first line to its last replaced by the one line.
/// `None` when it has no such block.
fn login_without_finger(service: &str) -> Option<String> {
    let mut block = LOGIN_FINGER.lines();
    let (first, last) = (block.next()?, block.next_back()?);
    let lines: Vec<&str> = service.lines().collect();
    let from = lines.iter().position(|l| *l == first)?;
    let to = from + lines[from..].iter().position(|l| *l == last)?;
    let mut out: Vec<&str> = lines[..from].to_vec();
    out.push(LOGIN_PASSWORD);
    out.extend(&lines[to + 1..]);
    Some(out.join("\n") + "\n")
}

/// Put the `pam_fprintd` line in polkit's service, or take it out. Only that
/// line and its note are touched, and a service already as asked is not
/// written at all.
fn polkit(env: &Env, on: bool) -> Result<(), String> {
    let path = env.system(POLKIT);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // The distribution's own, which /etc overrides from here on.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && on => {
            let vendor = env.system(POLKIT_VENDOR);
            std::fs::read_to_string(&vendor).map_err(|e| crate::io_error(&vendor, &e))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(crate::io_error(&path, &e)),
    };
    let changed = if on {
        with_finger(&text)
    } else {
        without_finger(&text)
    };
    match changed {
        Some(text) => crate::generated::replace(&path, &text),
        None if on && !text.lines().any(|l| l == POLKIT_LINE) => Err(format!(
            "{} has no password line to put a fingerprint before; add \"{POLKIT_LINE}\" \
             above it by hand",
            path.display()
        )),
        None => Ok(()),
    }
}

/// polkit's service with the fingerprint first among the ways in: before the
/// first `auth` line that checks something — `pam_unix`, or a stack it
/// includes — and after those that only set things up. `None` when it is
/// there already, or there is nowhere to put it.
fn with_finger(service: &str) -> Option<String> {
    if service.lines().any(|l| l == POLKIT_LINE) {
        return None;
    }
    let lines: Vec<&str> = service.lines().collect();
    let at = lines.iter().position(|line| {
        let mut words = line.split_whitespace();
        let (Some(kind), Some(control), Some(module)) = (words.next(), words.next(), words.next())
        else {
            return false;
        };
        kind.trim_start_matches('-') == "auth"
            && (matches!(control, "include" | "substack") || module.starts_with("pam_unix"))
    })?;
    let mut out: Vec<&str> = lines[..at].to_vec();
    out.extend([POLKIT_NOTE, POLKIT_LINE]);
    out.extend(&lines[at..]);
    Some(out.join("\n") + "\n")
}

/// polkit's service without the line and its note; `None` when it has
/// neither.
fn without_finger(service: &str) -> Option<String> {
    let kept: Vec<&str> = service
        .lines()
        .filter(|l| ![POLKIT_LINE, POLKIT_NOTE, POLKIT_NOTE_BEFORE].contains(l))
        .collect();
    (kept.len() != service.lines().count()).then(|| kept.join("\n") + "\n")
}

/// Open a terminal for `program`, and leave it: it asks, and it says whether
/// it worked, before the window goes. The terminal is the one chosen in
/// Settings › Default applications.
fn in_terminal(program: &str) -> Result<(), String> {
    let command = [
        "sh".to_owned(),
        "-c".to_owned(),
        format!("{program}; printf '\\nPress Enter to close. '; read -r _"),
    ];
    let argv =
        alpymist_core::defaults::task_argv(&alpymist_core::defaults::Places::current(), &command);
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
    use super::{
        ADMINISTRATOR_ID, FINGERPRINT_LOCK_ID, FINGERPRINT_LOGIN_ID, FINGERPRINT_PROMPTS_ID, GROUP,
        HOSTNAME, HOSTNAME_ID, INTERFACES, LOCK_FINGERPRINT, LOGIN, PAM_FPRINTD, POLKIT, members,
        rename,
    };
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

    /// polkit's service as Alpine ships it.
    const POLKIT_1: &str = "auth            requisite       pam_nologin.so\n\
                            auth            required        pam_env.so\n\
                            auth            required        pam_unix.so\n\
                            account         required        pam_unix.so\n\
                            session         required        pam_unix.so\n";

    #[test]
    fn a_finger_goes_before_the_password_and_comes_out_again_leaving_the_rest() {
        let with = super::with_finger(POLKIT_1).unwrap();
        let lines: Vec<&str> = with.lines().collect();
        assert!(lines[1].contains("pam_env"), "setting up comes first");
        assert_eq!(lines[3], super::POLKIT_LINE);
        assert!(lines[4].contains("pam_unix"), "then the password");
        assert_eq!(super::with_finger(&with), None, "once is enough");
        assert_eq!(super::without_finger(&with).as_deref(), Some(POLKIT_1));
        assert_eq!(super::without_finger(POLKIT_1), None);
        // A stack that includes another is put before the include.
        let included = super::with_finger("auth include base-auth\n").unwrap();
        assert!(included.ends_with(&format!("{}\nauth include base-auth\n", super::POLKIT_LINE)));
        assert_eq!(super::with_finger("account required pam_unix.so\n"), None);
    }

    /// The login service as the package ships it.
    const GREETD: &str = include_str!("../../../../desktop/alpymist-greetd.pam");
    /// Alpine's `base-auth`, from linux-pam 1.7, which the login service
    /// includes and a login by finger spells out.
    const BASE_AUTH: &str = "auth required pam_unix.so nullok\n\
                             auth required pam_nologin.so\n\
                             auth required pam_env.so\n\
                             \n\
                             -auth optional pam_gnome_keyring.so\n\
                             -auth optional pam_kwallet5.so\n";

    #[test]
    fn each_place_a_finger_is_taken_has_a_switch_of_its_own_and_all_are_off() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-finger-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, true, &RAN);
        let settings = Settings::new();
        std::fs::create_dir_all(env.system("etc/pam.d")).unwrap();
        std::fs::write(env.system(POLKIT), POLKIT_1).unwrap();
        std::fs::write(env.system(LOGIN), GREETD).unwrap();
        let all = [
            FINGERPRINT_LOCK_ID,
            FINGERPRINT_PROMPTS_ID,
            FINGERPRINT_LOGIN_ID,
        ];
        let read = |file: &str| std::fs::read_to_string(env.system(file)).unwrap();
        let state = || all.map(|id| settings.get(&env, id).unwrap() == Value::Bool(true));
        assert_eq!(state(), [false; 3]);
        for id in all {
            assert_eq!(settings.find(id).unwrap().default, Value::Bool(false));
            // Without fprintd-pam, nothing is changed.
            let refused = settings.set(&env, id, "on", false);
            assert!(
                matches!(refused, Err(Error::Failed(ref m)) if m.contains("alpymist-fingerprint")),
                "{refused:?}"
            );
        }
        assert_eq!(read(POLKIT), POLKIT_1);
        assert_eq!(read(LOGIN), GREETD);
        assert!(!env.system(LOCK_FINGERPRINT).exists());

        std::fs::create_dir_all(env.system("usr/lib/security")).unwrap();
        std::fs::write(env.system(PAM_FPRINTD), "").unwrap();

        // Each alone: the others stay as they were.
        settings
            .set(&env, FINGERPRINT_LOCK_ID, "on", false)
            .unwrap();
        assert_eq!(state(), [true, false, false]);
        let lock = read(LOCK_FINGERPRINT);
        assert!(lock.contains("pam_fprintd.so") && !lock.contains("pam_unix"));
        assert_eq!(read(POLKIT), POLKIT_1);
        assert_eq!(read(LOGIN), GREETD);

        settings
            .set(&env, FINGERPRINT_PROMPTS_ID, "on", false)
            .unwrap();
        assert_eq!(state(), [true, true, false]);
        assert!(read(POLKIT).contains(super::POLKIT_LINE));
        assert_eq!(read(LOGIN), GREETD);

        settings.reset(&env, FINGERPRINT_LOCK_ID, false).unwrap();
        assert_eq!(state(), [false, true, false]);
        assert!(!env.system(LOCK_FINGERPRINT).exists());

        settings
            .set(&env, FINGERPRINT_LOGIN_ID, "on", false)
            .unwrap();
        assert_eq!(state(), [false, true, true]);
        settings
            .set(&env, FINGERPRINT_LOGIN_ID, "on", false)
            .unwrap();
        assert_eq!(
            read(LOGIN).matches("pam_fprintd").count(),
            1,
            "once is enough"
        );

        settings.reset(&env, FINGERPRINT_PROMPTS_ID, false).unwrap();
        settings.reset(&env, FINGERPRINT_LOGIN_ID, false).unwrap();
        assert_eq!(state(), [false; 3]);
        assert_eq!(read(POLKIT), POLKIT_1);
        assert_eq!(read(LOGIN), GREETD, "exactly as the package ships it");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A system switched on when one switch did the lock and the prompts
    /// shows both on, and its prompts switch takes out the old note too.
    #[test]
    fn a_system_switched_on_before_the_switches_were_split_is_read_and_cleaned() {
        let before = format!(
            "auth requisite pam_nologin.so\n{}\n{}\nauth required pam_unix.so\n",
            super::POLKIT_NOTE_BEFORE,
            super::POLKIT_LINE
        );
        assert_eq!(
            super::without_finger(&before).as_deref(),
            Some("auth requisite pam_nologin.so\nauth required pam_unix.so\n")
        );
        assert_eq!(super::with_finger(&before), None, "already on");
    }

    #[test]
    fn a_login_by_finger_checks_what_a_login_by_password_checks() {
        let with = super::login_with_finger(GREETD).unwrap();
        assert!(!with.contains("base-auth\n"), "{with}");
        // The rest of the service is as it was.
        for line in GREETD.lines().filter(|l| !l.contains("base-auth")) {
            assert!(with.lines().any(|l| l == line), "lost: {line}");
        }
        assert_eq!(super::login_without_finger(&with).as_deref(), Some(GREETD));
        assert_eq!(super::login_without_finger(GREETD), None);
        assert_eq!(
            super::login_with_finger(&with),
            None,
            "nowhere to put it twice"
        );
        assert_eq!(
            super::login_with_finger("auth required pam_unix.so\n"),
            None,
            "a service somebody rewrote is left alone"
        );

        // The modules a finger's stack runs, in order, less the three that
        // are the finger: the reader, the refusal, and the yes that a stack
        // of jumps needs.
        let module = |line: &str| {
            let line = line.trim_start_matches('-');
            let rest = line.strip_prefix("auth")?.trim_start();
            // The control: one word, or one bracket.
            let rest = match rest.strip_prefix('[') {
                Some(bracket) => bracket.split_once(']')?.1,
                None => rest.split_once(char::is_whitespace)?.1,
            };
            Some(rest.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        let finger: Vec<String> = super::LOGIN_FINGER
            .lines()
            .filter_map(module)
            .filter(|m| {
                !["pam_fprintd.so", "pam_deny.so", "pam_permit.so"]
                    .iter()
                    .any(|own| m.starts_with(own))
            })
            .collect();
        // kwallet is KDE's, and nothing here installs it.
        let base: Vec<String> = BASE_AUTH
            .lines()
            .filter_map(module)
            .filter(|m| m != "pam_kwallet5.so")
            .collect();
        let sorted = |mut modules: Vec<String>| {
            modules.sort();
            modules
        };
        assert_eq!(sorted(finger), sorted(base));

        // The password before the finger, and a right one jumps the reader
        // and the refusal both.
        let lines: Vec<&str> = super::LOGIN_FINGER.lines().collect();
        assert!(lines[1].contains("[success=2 default=ignore]") && lines[1].contains("pam_unix"));
        assert!(lines[2].starts_with("-auth") && lines[2].contains("[success=2 default=ignore]"));
        assert!(lines[2].contains("pam_fprintd.so max-tries=1 timeout=10"));
        assert!(lines[3].contains("requisite") && lines[3].contains("pam_deny.so"));
        // A finger goes past the keyring, which only a password opens, and
        // lands on the line that says yes.
        assert!(lines[4].contains("pam_gnome_keyring.so"));
        assert!(lines[5].contains("required") && lines[5].contains("pam_permit.so"));
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

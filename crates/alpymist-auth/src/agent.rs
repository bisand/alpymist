//! The polkit authentication agent a program registers for itself.
//!
//! polkit's agents usually speak for a login session, which needs elogind to
//! exist. They can also speak for one process, as `pkttyagent --process`
//! does, and that is what this is: [`register`] tells polkitd that this
//! process's authentications come here. A program then runs its privileged
//! helper with `pkexec`, whose parent it is; polkitd calls
//! `BeginAuthentication`, and the agent starts `alpymist-auth prompt` with
//! polkitd's description of the request on its standard input.
//!
//! The agent never sees a password: the prompt is a process of its own, and
//! hands the password straight to polkit's helper. Nor can it speak for
//! anything but its own process: polkitd only accepts an agent for a process
//! of the same user, and routes to it only that process's authentications.

// polkit's interface fixes the signatures, and the interface macro passes
// lints on an impl's attributes by.
#![allow(clippy::used_underscore_binding)]

use crate::request::{Identity, Request, identity};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use zbus::zvariant::{OwnedValue, Value};

/// Where the agent is served on the bus.
pub const PATH: &str = "/org/alpymist/AuthenticationAgent";

/// Prompts running, by cookie, so polkitd can cancel one.
type Running = Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>;

/// The agent: what it starts to ask, and what it has started.
struct Agent {
    prompt: PathBuf,
    running: Running,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// Not authorised: cancelled, or the password was not accepted.
    Cancelled(String),
    /// The prompt could not be shown.
    Failed(String),
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        _icon_name: String,
        details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> Result<(), AgentError> {
        let mut details: BTreeMap<String, String> = details.into_iter().collect();
        // polkit no longer passes pkexec's command to agents; the pkexec
        // waiting for this answer still has it, and it is root's, so what it
        // says it will run is what it will run.
        if !details.contains_key("command_line")
            && let Some(command) = details
                .get("polkit.caller-pid")
                .and_then(|pid| pid.parse().ok())
                .and_then(pkexec_command)
        {
            details.insert("command_line".into(), command);
        }
        let request = Request {
            action: action_id,
            message,
            cookie: cookie.clone(),
            details,
            identities: expand(&identities),
        };
        if request.identities.is_empty() {
            return Err(AgentError::Failed("no account may authorise this".into()));
        }
        let input = serde_json::to_vec(&request).map_err(|e| AgentError::Failed(e.to_string()))?;
        let mut child = Command::new(&self.prompt)
            .arg("prompt")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| AgentError::Failed(format!("{}: {e}", self.prompt.display())))?;
        if let Some(mut stdin) = child.stdin.take() {
            // Dropped at the end of this block: the prompt reads to the end.
            stdin
                .write_all(&input)
                .map_err(|e| AgentError::Failed(e.to_string()))?;
        }
        let child = Arc::new(Mutex::new(child));
        lock(&self.running).insert(cookie.clone(), Arc::clone(&child));
        let status = blocking::unblock(move || wait(&child)).await;
        lock(&self.running).remove(&cookie);
        match status {
            Some(true) => Ok(()),
            _ => Err(AgentError::Cancelled("not authorised".into())),
        }
    }

    fn cancel_authentication(&self, cookie: &str) {
        if let Some(child) = lock(&self.running).get(cookie) {
            let _ = lock(child).kill();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Wait for a prompt without holding its lock, so it can be cancelled.
/// `Some(true)` when it authorised.
fn wait(child: &Mutex<Child>) -> Option<bool> {
    loop {
        match lock(child).try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) => {}
            Err(_) => return None,
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The command a waiting `pkexec` will run, if process `pid` is one: owned
/// by root, so nobody but root can have changed what it says, and named
/// pkexec.
fn pkexec_command(pid: u32) -> Option<String> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    parse_pkexec(&status, &cmdline)
}

fn parse_pkexec(status: &str, cmdline: &[u8]) -> Option<String> {
    // Uid: real, effective, saved, filesystem. setuid pkexec runs as root.
    let effective = status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?;
    if effective != "0" {
        return None;
    }
    let args: Vec<String> = cmdline
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let (first, rest) = args.split_first()?;
    if first.rsplit('/').next() != Some("pkexec") {
        return None;
    }
    // pkexec's own options come before the program.
    let mut words = rest.iter().peekable();
    while let Some(word) = words.peek() {
        match word.as_str() {
            "--user" | "-u" => {
                words.next();
                words.next();
            }
            w if w.starts_with("--") => {
                words.next();
            }
            _ => break,
        }
    }
    let command: Vec<&str> = words.map(String::as_str).collect();
    (!command.is_empty()).then(|| command.join(" "))
}

/// polkit's identities as accounts: users as they are, groups as their
/// members, root last, since a password for root is rarely what anyone
/// means to type.
fn expand(identities: &[(String, HashMap<String, OwnedValue>)]) -> Vec<Identity> {
    let groups = std::fs::read_to_string("/etc/group").unwrap_or_default();
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let mut out: Vec<Identity> = Vec::new();
    let mut add = |who: Option<Identity>| {
        if let Some(who) = who
            && !out.iter().any(|i| i.uid == who.uid)
        {
            out.push(who);
        }
    };
    for (kind, fields) in identities {
        let number = |key: &str| fields.get(key).and_then(|v| u32::try_from(v).ok());
        match kind.as_str() {
            "unix-user" => add(number("uid").and_then(identity)),
            "unix-group" => {
                if let Some(gid) = number("gid") {
                    for name in members(&groups, &passwd, gid) {
                        add(uid_of(&passwd, &name).and_then(identity));
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|i| i.uid == 0);
    out
}

/// The members of group `gid`: those listed in `/etc/group`, and those whose
/// primary group it is.
fn members(groups: &str, passwd: &str, gid: u32) -> Vec<String> {
    let mut names: Vec<String> = groups
        .lines()
        .filter(|l| l.split(':').nth(2).and_then(|g| g.parse::<u32>().ok()) == Some(gid))
        .flat_map(|l| {
            l.split(':')
                .nth(3)
                .unwrap_or("")
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|n| !n.is_empty())
        .collect();
    for line in passwd.lines() {
        let f: Vec<&str> = line.split(':').collect();
        if f.get(3).and_then(|g| g.parse::<u32>().ok()) == Some(gid)
            && let Some(name) = f.first()
            && !names.iter().any(|n| n == name)
        {
            names.push((*name).to_owned());
        }
    }
    names
}

fn uid_of(passwd: &str, name: &str) -> Option<u32> {
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.first() == Some(&name)).then(|| f.get(2)?.parse().ok())?
    })
}

/// When this process started, in clock ticks since boot, as polkit names a
/// process: a pid alone could be reused by another.
fn start_time() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The command name is in parentheses and may hold spaces; the fields
    // after it are counted from the last closing one.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// The agent, for as long as it is kept: dropping it closes the connection,
/// and polkitd forgets the agent.
pub struct Registration {
    _connection: zbus::blocking::Connection,
}

/// Register an agent for this process that asks with `prompt`: the
/// `alpymist-auth` binary.
///
/// # Errors
/// No system bus, no polkitd, or polkitd refused.
pub fn register(prompt: PathBuf) -> Result<Registration, String> {
    let agent = Agent {
        prompt,
        running: Arc::default(),
    };
    let connection = zbus::blocking::connection::Builder::system()
        .and_then(|b| b.serve_at(PATH, agent))
        .and_then(zbus::blocking::connection::Builder::build)
        .map_err(|e| format!("the system bus: {e}"))?;
    let pid = std::process::id();
    let uid = crate::request::own_uid().ok_or("could not read this process's user")?;
    let start = start_time().ok_or("could not read when this process started")?;
    let mut subject: HashMap<&str, Value<'_>> = HashMap::new();
    subject.insert("pid", Value::from(pid));
    subject.insert("start-time", Value::from(start));
    subject.insert("uid", Value::from(i32::try_from(uid).unwrap_or(-1)));
    let locale = std::env::var("LANG").unwrap_or_else(|_| "C".into());
    connection
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "RegisterAuthenticationAgent",
            &(("unix-process", subject), locale.as_str(), PATH),
        )
        .map_err(|e| format!("polkit: {e}"))?;
    Ok(Registration {
        _connection: connection,
    })
}

#[cfg(test)]
mod tests {
    use super::{members, parse_pkexec, uid_of};

    #[test]
    fn only_a_root_pkexec_is_believed() {
        let root = "Name:\tpkexec\nUid:\t1000\t0\t0\t0\n";
        let user = "Name:\tpkexec\nUid:\t1000\t1000\t1000\t1000\n";
        let argv =
            b"pkexec\0--disable-internal-agent\0/usr/libexec/alpymist-store-helper\0add\0sl\0";
        assert_eq!(
            parse_pkexec(root, argv).as_deref(),
            Some("/usr/libexec/alpymist-store-helper add sl")
        );
        assert_eq!(
            parse_pkexec(user, argv),
            None,
            "a process of the user's could say anything"
        );
        assert_eq!(parse_pkexec(root, b"sh\0-c\0evil\0"), None);
        assert_eq!(
            parse_pkexec(root, b"/usr/bin/pkexec\0--user\0kari\0/bin/true\0").as_deref(),
            Some("/bin/true")
        );
    }

    #[test]
    fn a_group_is_its_listed_members_and_those_it_is_primary_for() {
        let groups = "root:x:0:root\nwheel:x:10:root,andre\n";
        let passwd = "root:x:0:0::/root:/bin/sh\nandre:x:1000:1000::/home/andre:/bin/sh\nkari:x:1001:10::/home/kari:/bin/sh\n";
        assert_eq!(members(groups, passwd, 10), ["root", "andre", "kari"]);
        assert_eq!(uid_of(passwd, "kari"), Some(1001));
        assert_eq!(uid_of(passwd, "nobody"), None);
    }
}

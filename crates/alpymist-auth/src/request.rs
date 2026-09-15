//! What is being authorised, as polkitd describes it.
//!
//! The agent receives this from polkitd, not from the program asking, and
//! passes it to the prompt on standard input. So what the dialog says is
//! being done — the command `pkexec` was given, and who may authorise it —
//! is what polkit will allow, not what the program would like it to show.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One authentication, for the prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// The polkit action: `org.alpymist.store.install`.
    pub action: String,
    /// polkit's message for it: "Install Alpine packages".
    pub message: String,
    /// The authentication's cookie, for the helper.
    pub cookie: String,
    /// polkit's details: `program`, `command_line`, `user` for `pkexec`.
    pub details: BTreeMap<String, String>,
    /// The accounts whose password would do, the one to ask for first.
    pub identities: Vec<Identity>,
}

/// An account that may authorise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Its user id.
    pub uid: u32,
    /// Its login name.
    pub name: String,
    /// Its full name, where the account has one.
    pub full_name: String,
}

impl Identity {
    /// The account as shown: "André Biseth (andre)", or just the login.
    #[must_use]
    pub fn display(&self) -> String {
        if self.full_name.is_empty() || self.full_name == self.name {
            self.name.clone()
        } else {
            format!("{} ({})", self.full_name, self.name)
        }
    }
}

/// The account with `uid`, from `/etc/passwd`.
#[must_use]
pub fn identity(uid: u32) -> Option<Identity> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    identity_in(&passwd, uid)
}

fn identity_in(passwd: &str, uid: u32) -> Option<Identity> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        let (name, id, gecos) = (
            fields.first()?,
            fields.get(2)?,
            fields.get(4).unwrap_or(&""),
        );
        (id.parse::<u32>().ok()? == uid).then(|| Identity {
            uid,
            name: (*name).to_owned(),
            full_name: gecos.split(',').next().unwrap_or("").to_owned(),
        })
    })
}

/// This process's real user id, from `/proc`.
#[must_use]
pub fn own_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|ids| ids.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

impl Request {
    /// What the dialog says is to be done, in words where the command is one
    /// of Alpymist's helpers, and as the command otherwise.
    #[must_use]
    pub fn what(&self) -> String {
        let command = self.details.get("command_line").map_or("", String::as_str);
        let mut words = command.split_whitespace();
        let program = words.next().unwrap_or("");
        let rest: Vec<&str> = words.collect();
        if program == "/usr/libexec/alpymist-store-helper"
            && let Some((verb, names)) = rest.split_first()
        {
            let names = names.join(", ");
            return match (*verb, names.is_empty()) {
                ("add", false) => format!("Install {names}"),
                ("del", false) => format!("Remove {names}"),
                ("upgrade", true) => "Update every system package".into(),
                ("upgrade", false) => format!("Update {names}"),
                ("update", true) => "Refresh the package lists".into(),
                _ => command.to_owned(),
            };
        }
        command.to_owned()
    }

    /// The identity to ask for first: this account, when it may authorise.
    #[must_use]
    pub fn first_identity(&self, own: Option<u32>) -> usize {
        own.and_then(|uid| self.identities.iter().position(|i| i.uid == uid))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Identity, Request, identity_in};

    #[test]
    fn accounts_come_from_passwd() {
        let passwd = "root:x:0:0:root:/root:/bin/sh\nandre:x:1000:1000:André Biseth,,,:/home/andre:/bin/zsh\n";
        let andre = identity_in(passwd, 1000).unwrap();
        assert_eq!(andre.display(), "André Biseth (andre)");
        assert_eq!(identity_in(passwd, 0).unwrap().display(), "root");
        assert!(identity_in(passwd, 5).is_none());
    }

    #[test]
    fn the_store_helper_is_described_in_words() {
        let mut r = Request::default();
        let mut say = |line: &str| {
            r.details.insert("command_line".into(), line.into());
            r.what()
        };
        assert_eq!(
            say("/usr/libexec/alpymist-store-helper add gimp"),
            "Install gimp"
        );
        assert_eq!(
            say("/usr/libexec/alpymist-store-helper del sl zsh"),
            "Remove sl, zsh"
        );
        assert_eq!(
            say("/usr/libexec/alpymist-store-helper upgrade"),
            "Update every system package"
        );
        assert_eq!(say("/sbin/reboot"), "/sbin/reboot");
    }

    #[test]
    fn this_account_is_asked_for_first() {
        let who = |uid| Identity {
            uid,
            ..Identity::default()
        };
        let r = Request {
            identities: vec![who(1001), who(1000)],
            ..Request::default()
        };
        assert_eq!(r.first_identity(Some(1000)), 1);
        assert_eq!(r.first_identity(Some(1002)), 0);
    }
}

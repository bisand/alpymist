//! Whose session this is.
//!
//! The login screen offers everybody who can log in and lets you choose. A
//! lock screen has exactly one account — the one whose session is behind it —
//! and getting it wrong means asking for a password that cannot open anything.
//!
//! It is found by user id rather than by `$USER`: the environment is whatever
//! started the session and can say anything, and `/proc/self/status` cannot.

pub use alpymist_greeter::users::User;

/// Where the kernel says who this process is.
const STATUS: &str = "/proc/self/status";

/// This process's real user id.
#[must_use]
pub fn own_uid() -> Option<u32> {
    uid_in(&std::fs::read_to_string(STATUS).ok()?)
}

/// The real user id in a `/proc/<pid>/status`.
///
/// The line is `Uid:\treal\teffective\tsaved\tfilesystem`, and it is the real
/// one that says whose session this is.
#[must_use]
pub fn uid_in(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// The account this session belongs to.
///
/// `uid` is what [`own_uid`] found; `named` is `$USER` or `$LOGNAME`, used
/// only when there is no uid to go on.
#[must_use]
pub fn account(passwd: &str, uid: Option<u32>, named: Option<&str>) -> Option<User> {
    passwd.lines().filter_map(entry).find(|user| match uid {
        Some(uid) => user.uid == uid,
        None => named.is_some_and(|name| name == user.name),
    })
}

/// One line of `/etc/passwd` as an account.
///
/// No filtering by shell or by id, unlike the login screen's list: this
/// account is already logged in, whatever `/etc/passwd` thinks of it.
fn entry(line: &str) -> Option<User> {
    let fields: Vec<&str> = line.split(':').collect();
    let [name, _, uid, _, gecos, _, _] = fields.as_slice() else {
        return None;
    };
    if name.is_empty() {
        return None;
    }
    let display = gecos.split(',').next().unwrap_or_default().trim();
    Some(User {
        name: (*name).to_owned(),
        display: if display.is_empty() {
            (*name).to_owned()
        } else {
            display.to_owned()
        },
        uid: uid.parse().ok()?,
    })
}

/// The account this session belongs to, from this machine.
#[must_use]
pub fn current() -> Option<User> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    let named = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok();
    account(&passwd, own_uid(), named.as_deref())
}

#[cfg(test)]
mod tests {
    use super::{account, uid_in};

    const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/sh
daemon:x:2:2:daemon:/sbin:/sbin/nologin
andre:x:1000:1000:André Biseth,,,:/home/andre:/bin/zsh
kid:x:1001:1001::/home/kid:/bin/sh
";

    #[test]
    fn the_real_uid_is_the_first_of_the_four() {
        let status = "Name:\talpymist-lock\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\n";
        assert_eq!(uid_in(status), Some(1000));
        assert_eq!(uid_in("Name:\tsh\n"), None);
    }

    #[test]
    fn the_account_is_the_one_with_that_id_whatever_the_environment_says() {
        let user = account(PASSWD, Some(1000), Some("kid")).expect("andre");
        assert_eq!(user.name, "andre");
        assert_eq!(
            user.display, "André Biseth",
            "the first field of the comment"
        );
    }

    #[test]
    fn an_account_without_a_full_name_is_shown_by_its_login() {
        let user = account(PASSWD, Some(1001), None).expect("kid");
        assert_eq!(user.display, "kid");
    }

    #[test]
    fn root_locks_its_own_session_too() {
        let user = account(PASSWD, Some(0), None).expect("root");
        assert_eq!(user.name, "root");
    }

    #[test]
    fn without_a_uid_the_name_is_all_there_is() {
        assert_eq!(
            account(PASSWD, None, Some("kid")).map(|u| u.uid),
            Some(1001)
        );
        assert!(account(PASSWD, None, None).is_none());
        assert!(account(PASSWD, Some(4242), Some("andre")).is_none());
    }
}

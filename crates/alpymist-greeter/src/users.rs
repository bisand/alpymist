//! Who can log in here.
//!
//! Read from `/etc/passwd` rather than asked for: the people who use one of
//! these laptops are a list of one or two, and choosing from it is quicker and
//! less error-prone than typing a username on a keyboard that may not be
//! laid out the way its keycaps say.

/// An account offered on the login screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// The login name.
    pub name: String,
    /// What to call them: the first field of the comment, or the login name.
    pub display: String,
    /// Numeric id, for ordering.
    pub uid: u32,
}

/// The range adduser gives people, as opposed to services.
const FIRST_UID: u32 = 1000;
const LAST_UID: u32 = 59_999;

/// Shells that mean "this account does not log in".
fn refuses_login(shell: &str) -> bool {
    shell.is_empty() || shell.ends_with("/nologin") || shell.ends_with("/false")
}

/// The people in a passwd file, oldest account first.
#[must_use]
pub fn parse(passwd: &str) -> Vec<User> {
    let mut users: Vec<User> = passwd
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            let [name, _, uid, _, gecos, _, shell] = fields.as_slice() else {
                return None;
            };
            let uid: u32 = uid.parse().ok()?;
            if !(FIRST_UID..=LAST_UID).contains(&uid) || refuses_login(shell) || name.is_empty() {
                return None;
            }
            let full = gecos.split(',').next().unwrap_or("").trim();
            Some(User {
                name: (*name).to_string(),
                display: if full.is_empty() { *name } else { full }.to_string(),
                uid,
            })
        })
        .collect();
    users.sort_by_key(|u| u.uid);
    users
}

/// The people on this machine.
#[must_use]
pub fn discover() -> Vec<User> {
    std::fs::read_to_string("/etc/passwd")
        .map(|text| parse(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::parse;

    const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/sh
greetd:x:101:102:greetd:/var/lib/greetd:/sbin/nologin
# a comment
andre:x:1000:1000:André Biseth,,,:/home/andre:/bin/zsh
kid:x:1002:1002::/home/kid:/bin/zsh
old:x:1001:1001:Old account:/home/old:/sbin/nologin
nobody:x:65534:65534:nobody:/:/sbin/nologin
broken line
";

    #[test]
    fn only_people_who_can_log_in_are_offered() {
        let names: Vec<String> = parse(PASSWD).into_iter().map(|u| u.name).collect();
        assert_eq!(names, ["andre", "kid"]);
    }

    #[test]
    fn the_full_name_is_shown_where_there_is_one() {
        let users = parse(PASSWD);
        assert_eq!(users[0].display, "André Biseth");
        assert_eq!(users[1].display, "kid", "no comment, so the login name");
    }

    #[test]
    fn accounts_are_in_the_order_they_were_made() {
        let users = parse("b:x:1005:1:B:/:/bin/sh\na:x:1000:1:A:/:/bin/sh\n");
        assert_eq!(users[0].name, "a");
    }
}

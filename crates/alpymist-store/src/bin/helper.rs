//! `alpymist-store-helper` — the part of the store that needs root.
//!
//! Run through pkexec by `alpymist-store`:
//!
//! ```text
//! alpymist-store-helper add NAME…
//! alpymist-store-helper del NAME…
//! alpymist-store-helper upgrade [NAME…]
//! alpymist-store-helper update
//! ```
//!
//! polkit's policy (org.alpymist.store.policy) lets an administrator run it
//! with their password, so it is the whole of what that password allows here,
//! and it is kept small enough to read in one sitting. It trusts nothing
//! about its caller:
//!
//! - Four verbs, and package names that can be nothing else: lowercase
//!   letters, digits and `._+-`, starting with a letter or digit. No option
//!   (`--allow-untrusted`, `--repository`, `--root`) can be passed, no file
//!   or URL, no version, no repository tag.
//! - apk is run by its full path, from `/`, with an empty environment, so
//!   nothing of the caller's reaches it.
//! - Packages that make up the system cannot be removed. The shipped
//!   store.toml protects the same names, but a user can edit their own copy,
//!   so the list that counts is compiled in here.
//! - Every run is logged to syslog, so what was installed or removed, and
//!   by whom, can be found again.

#![forbid(unsafe_code)]

use std::process::{Command, ExitCode};

/// apk.
const APK: &str = "/sbin/apk";

/// The most names one run takes.
const MOST: usize = 32;

/// What cannot be removed, whatever the store's configuration says.
const PROTECTED: &[&str] = &[
    "alpine-base",
    "alpymist-*",
    "apk-tools",
    "busybox",
    "doas",
    "linux-*",
    "musl",
    "openrc",
    "polkit",
    "polkit-common",
];

/// A verb, with its checked names.
#[derive(Debug, PartialEq, Eq)]
enum Verb<'a> {
    Add(Vec<&'a str>),
    Del(Vec<&'a str>),
    Upgrade(Vec<&'a str>),
    Update,
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_lowercase() | bytes[0].is_ascii_digit()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'+' | b'-'))
        // apk takes a name ending in .apk for a file to install.
        && !std::path::Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("apk"))
}

fn protected(name: &str) -> bool {
    PROTECTED.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

fn parse(args: &[String]) -> Result<Verb<'_>, String> {
    let (verb, names) = args
        .split_first()
        .ok_or("no verb: add, del, upgrade or update")?;
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    if names.len() > MOST {
        return Err(format!("at most {MOST} packages at a time"));
    }
    if let Some(bad) = names.iter().find(|n| !valid_name(n)) {
        return Err(format!("`{bad}` is not a package name"));
    }
    match verb.as_str() {
        "add" if !names.is_empty() => Ok(Verb::Add(names)),
        "del" if !names.is_empty() => {
            if let Some(p) = names.iter().find(|n| protected(n)) {
                return Err(format!("{p} is part of the system and cannot be removed"));
            }
            Ok(Verb::Del(names))
        }
        "upgrade" => Ok(Verb::Upgrade(names)),
        "update" if names.is_empty() => Ok(Verb::Update),
        "add" | "del" => Err(format!("{verb} needs at least one package")),
        "update" => Err("update takes no packages".into()),
        other => Err(format!(
            "`{other}` is not a verb: add, del, upgrade or update"
        )),
    }
}

fn argv<'a>(verb: &Verb<'a>) -> Vec<&'a str> {
    let mut argv = vec!["--no-interactive"];
    match verb {
        Verb::Add(names) => {
            argv.push("add");
            argv.extend(names);
        }
        Verb::Del(names) => {
            argv.push("del");
            argv.extend(names);
        }
        Verb::Upgrade(names) => {
            // Fresh indexes first, as `apk upgrade -U` does.
            argv.extend(["--update-cache", "upgrade"]);
            argv.extend(names);
        }
        Verb::Update => argv.push("update"),
    }
    argv
}

fn log(message: &str) {
    // pkexec says who asked by user id; doas, for running it by hand, by name.
    let who = std::env::var("PKEXEC_UID")
        .map(|uid| format!("uid {uid}"))
        .or_else(|_| std::env::var("DOAS_USER"))
        .unwrap_or_else(|_| "unknown".into());
    let _ = Command::new("/usr/bin/logger")
        .args(["-t", "alpymist-store-helper", "--"])
        .arg(format!("{who}: {message}"))
        .env_clear()
        .status();
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verb = match parse(&args) {
        Ok(verb) => verb,
        Err(e) => {
            eprintln!("alpymist-store-helper: {e}");
            return ExitCode::from(2);
        }
    };
    let apk_args = argv(&verb);
    log(&format!("apk {}", apk_args.join(" ")));
    let status = Command::new(APK)
        .args(&apk_args)
        .current_dir("/")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            log(&format!("apk failed: {s}"));
            ExitCode::from(u8::try_from(s.code().unwrap_or(1)).unwrap_or(1))
        }
        Err(e) => {
            eprintln!("alpymist-store-helper: {APK}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Verb, argv, parse};

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn names_are_names_and_nothing_else() {
        for bad in [
            "--allow-untrusted",
            "-X",
            "/tmp/evil.apk",
            "evil.apk",
            "foo=1.0",
            "foo>1",
            "foo@testing",
            "https://example.org/x",
            "Foo",
            "",
            "../x",
        ] {
            assert!(
                parse(&args(&["add", bad])).is_err(),
                "{bad} must be refused"
            );
        }
        assert_eq!(
            parse(&args(&[
                "add",
                "gimp",
                "py3-numpy",
                "libstdc++",
                "font-fira-ttf"
            ])),
            Ok(Verb::Add(vec![
                "gimp",
                "py3-numpy",
                "libstdc++",
                "font-fira-ttf"
            ]))
        );
    }

    #[test]
    fn the_system_cannot_be_removed() {
        assert!(parse(&args(&["del", "musl"])).is_err());
        assert!(parse(&args(&["del", "alpymist-desktop-full"])).is_err());
        assert!(parse(&args(&["del", "linux-lts"])).is_err());
        assert!(parse(&args(&["del", "gimp", "busybox"])).is_err());
        assert!(parse(&args(&["del", "gimp"])).is_ok());
        // Installing them is harmless.
        assert!(parse(&args(&["add", "linux-lts"])).is_ok());
    }

    #[test]
    fn verbs_take_what_they_take() {
        assert!(parse(&args(&[])).is_err());
        assert!(parse(&args(&["add"])).is_err());
        assert!(parse(&args(&["update", "gimp"])).is_err());
        assert!(parse(&args(&["fix", "gimp"])).is_err());
        assert_eq!(parse(&args(&["upgrade"])), Ok(Verb::Upgrade(vec![])));
        assert_eq!(argv(&Verb::Update), ["--no-interactive", "update"]);
        assert_eq!(
            argv(&Verb::Upgrade(vec!["zsh"])),
            ["--no-interactive", "--update-cache", "upgrade", "zsh"]
        );
    }
}

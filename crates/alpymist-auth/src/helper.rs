//! The conversation with `polkit-agent-helper-1`.
//!
//! polkit's helper is setuid root: given a user name as its argument and the
//! authentication's cookie on its first line of input, it runs PAM for that
//! user and relays PAM's conversation a line at a time —
//!
//! ```text
//! PAM_PROMPT_ECHO_OFF Password:     answer with a line, not shown as typed
//! PAM_PROMPT_ECHO_ON Username:      answer with a line, shown as typed
//! PAM_ERROR_MSG …                   something went wrong; say so
//! PAM_TEXT_INFO …                   something to say
//! SUCCESS | FAILURE                 the end
//! ```
//!
//! — and on success tells polkitd itself. One helper is one attempt: after a
//! failure a new one is started for the next.

use crate::secret::Secret;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Where Alpine and most others install the helper.
pub const PATHS: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
];

/// The helper on this system.
#[must_use]
pub fn find() -> Option<PathBuf> {
    PATHS.iter().map(PathBuf::from).find(|p| p.exists())
}

/// A line from the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// PAM wants an answer; `echo` when it may be shown as typed.
    Prompt {
        /// What PAM asks, as it says it: "Password:".
        text: String,
        /// Whether the answer may be shown.
        echo: bool,
    },
    /// Something went wrong.
    Error(String),
    /// Something to say.
    Info(String),
    /// The attempt is over.
    Done(bool),
}

/// Parse one line of the helper's output.
#[must_use]
pub fn parse(line: &str) -> Step {
    let line = line.trim_end_matches(['\n', '\r']);
    let rest = |prefix: &str| line.strip_prefix(prefix).map(|r| r.trim().to_owned());
    if let Some(text) = rest("PAM_PROMPT_ECHO_OFF") {
        Step::Prompt { text, echo: false }
    } else if let Some(text) = rest("PAM_PROMPT_ECHO_ON") {
        Step::Prompt { text, echo: true }
    } else if let Some(text) = rest("PAM_ERROR_MSG") {
        Step::Error(text)
    } else if let Some(text) = rest("PAM_TEXT_INFO") {
        Step::Info(text)
    } else if line == "SUCCESS" {
        Step::Done(true)
    } else {
        // FAILURE, or anything the helper was not meant to say.
        Step::Done(false)
    }
}

/// One attempt at authenticating.
pub struct Attempt {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    over: bool,
}

impl Attempt {
    /// Start the helper at `path` for `user`, with the authentication's
    /// `cookie`.
    ///
    /// # Errors
    /// The helper could not be started.
    pub fn start(path: &Path, user: &str, cookie: &str) -> std::io::Result<Self> {
        let mut child = Command::new(path)
            .arg(user)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(std::io::Error::other("the helper has no pipes"));
        };
        // The cookie goes on standard input, where no other process can see
        // it, rather than in the arguments, where any can.
        input.write_all(cookie.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            over: false,
        })
    }

    /// The next thing the helper says. The end of its output is a failure.
    ///
    /// # Errors
    /// Reading from the helper failed.
    pub fn step(&mut self) -> std::io::Result<Step> {
        if self.over {
            return Ok(Step::Done(false));
        }
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            self.over = true;
            return Ok(Step::Done(false));
        }
        let step = parse(&line);
        if matches!(step, Step::Done(_)) {
            self.over = true;
        }
        Ok(step)
    }

    /// Answer a prompt, wiping the answer once written.
    ///
    /// # Errors
    /// Writing to the helper failed.
    pub fn answer(&mut self, secret: &mut Secret) -> std::io::Result<()> {
        let result = self
            .input
            .write_all(secret.expose())
            .and_then(|()| self.input.write_all(b"\n"))
            .and_then(|()| self.input.flush());
        secret.clear();
        result
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.over {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::{Attempt, Step, parse};
    use crate::secret::Secret;
    use std::path::PathBuf;

    #[test]
    fn lines_are_read_as_polkit_writes_them() {
        assert_eq!(
            parse("PAM_PROMPT_ECHO_OFF Password: \n"),
            Step::Prompt {
                text: "Password:".into(),
                echo: false
            }
        );
        assert_eq!(
            parse("PAM_ERROR_MSG Authentication failure"),
            Step::Error("Authentication failure".into())
        );
        assert_eq!(parse("SUCCESS"), Step::Done(true));
        assert_eq!(parse("FAILURE"), Step::Done(false));
        assert_eq!(parse("garbage"), Step::Done(false));
    }

    /// A stand-in helper: checks the cookie and the password it is given.
    fn fake_helper() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("alpymist-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("helper");
        std::fs::write(
            &path,
            "#!/bin/sh\n\
             read cookie\n\
             [ \"$1\" = alice ] && [ \"$cookie\" = c00kie ] || { echo FAILURE; exit 1; }\n\
             echo 'PAM_PROMPT_ECHO_OFF Password: '\n\
             read password\n\
             if [ \"$password\" = 'sesame' ]; then echo SUCCESS; else echo 'PAM_ERROR_MSG Wrong'; echo FAILURE; fi\n",
        )
        .unwrap();
        let _ = std::process::Command::new("chmod")
            .arg("+x")
            .arg(&path)
            .status();
        path
    }

    fn attempt(password: &str) -> Vec<Step> {
        let helper = fake_helper();
        let mut a = Attempt::start(&helper, "alice", "c00kie").unwrap();
        let mut steps = Vec::new();
        loop {
            let step = a.step().unwrap();
            if let Step::Prompt { .. } = step {
                let mut secret = Secret::new();
                password.chars().for_each(|c| {
                    secret.push(c);
                });
                a.answer(&mut secret).unwrap();
                assert!(secret.is_empty(), "wiped once written");
            }
            let done = matches!(step, Step::Done(_));
            steps.push(step);
            if done {
                return steps;
            }
        }
    }

    #[test]
    fn a_right_password_succeeds_and_a_wrong_one_says_why() {
        assert_eq!(attempt("sesame").last(), Some(&Step::Done(true)));
        let wrong = attempt("open");
        assert!(wrong.contains(&Step::Error("Wrong".into())));
        assert_eq!(wrong.last(), Some(&Step::Done(false)));
    }
}

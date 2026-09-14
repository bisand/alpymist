//! One login attempt, from username to a started session.
//!
//! greetd relays PAM's conversation rather than asking for a password itself,
//! so the greeter answers whatever is asked: the password for a hidden prompt,
//! an acknowledgement for a notice. A second hidden prompt means PAM wants more
//! than a password — a one-time code, say — which this screen has no field for,
//! so the attempt is abandoned and says why rather than sending the password
//! again as the answer to a different question.
//!
//! Every failure cancels the session before returning. greetd refuses to begin
//! a new one while an old one is half set up, so forgetting that turns one
//! wrong password into a screen that can never log anyone in.

use crate::ipc::{self, AuthMessageType, ErrorType, Request, Response};
use std::io::{self, Read, Write};

/// Anything that can carry greetd's messages: its socket, or a script in tests.
pub trait Transport {
    /// Send a request and wait for its reply.
    ///
    /// # Errors
    /// Whatever the connection fails with.
    fn call(&mut self, request: &Request) -> io::Result<Response>;
}

/// A connection to greetd over any byte stream.
pub struct Stream<S>(pub S);

impl<S: Read + Write> Transport for Stream<S> {
    fn call(&mut self, request: &Request) -> io::Result<Response> {
        ipc::send(&mut self.0, request)?;
        ipc::receive(&mut self.0)
    }
}

/// How an attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// greetd accepted the session and starts it once the greeter exits.
    Started,
    /// The password was wrong, or the account may not log in.
    Rejected(String),
    /// Something other than the credentials went wrong.
    Failed(String),
}

/// Log `username` in with `password` and start `cmd`.
///
/// Notices PAM sends along the way — "your password expires in 3 days" — are
/// returned in `notices`, for the screen to show.
pub fn attempt(
    greetd: &mut dyn Transport,
    username: &str,
    password: &str,
    cmd: &[String],
    notices: &mut Vec<String>,
) -> Outcome {
    match converse(greetd, username, password, cmd, notices) {
        Ok(Outcome::Started) => Outcome::Started,
        Ok(other) => {
            let _ = greetd.call(&Request::CancelSession);
            other
        }
        Err(e) => {
            let _ = greetd.call(&Request::CancelSession);
            Outcome::Failed(format!("Could not reach the login service: {e}"))
        }
    }
}

fn converse(
    greetd: &mut dyn Transport,
    username: &str,
    password: &str,
    cmd: &[String],
    notices: &mut Vec<String>,
) -> io::Result<Outcome> {
    let mut reply = greetd.call(&Request::CreateSession {
        username: username.to_string(),
    })?;
    let mut answered = false;

    loop {
        reply = match reply {
            Response::Success => break,
            Response::Error {
                error_type: ErrorType::AuthError,
                ..
            } => return Ok(Outcome::Rejected("That password is not right.".into())),
            Response::Error { description, .. } => return Ok(Outcome::Failed(description)),
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret | AuthMessageType::Visible,
                auth_message,
            } => {
                if answered {
                    return Ok(Outcome::Failed(format!(
                        "This account asks for more than a password ({}), which this screen cannot answer.",
                        auth_message.trim().trim_end_matches(':')
                    )));
                }
                answered = true;
                greetd.call(&Request::PostAuthMessageResponse {
                    response: Some(password.to_string()),
                })?
            }
            Response::AuthMessage { auth_message, .. } => {
                let text = auth_message.trim();
                if !text.is_empty() {
                    notices.push(text.to_string());
                }
                greetd.call(&Request::PostAuthMessageResponse { response: None })?
            }
        };
    }

    match greetd.call(&Request::StartSession {
        cmd: cmd.to_vec(),
        env: Vec::new(),
    })? {
        Response::Success => Ok(Outcome::Started),
        Response::Error { description, .. } => Ok(Outcome::Failed(format!(
            "The desktop could not be started: {description}"
        ))),
        Response::AuthMessage { .. } => Ok(Outcome::Failed(
            "The login service asked a question after the session was accepted.".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Transport, attempt};
    use crate::ipc::{AuthMessageType, ErrorType, Request, Response};
    use std::collections::VecDeque;
    use std::io;

    /// greetd, played from a script: records what it was asked and replies in
    /// order.
    struct Script {
        replies: VecDeque<Response>,
        asked: Vec<Request>,
    }

    impl Script {
        fn new(replies: impl IntoIterator<Item = Response>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
                asked: Vec::new(),
            }
        }

        fn cancelled(&self) -> bool {
            self.asked.last() == Some(&Request::CancelSession)
        }
    }

    impl Transport for Script {
        fn call(&mut self, request: &Request) -> io::Result<Response> {
            self.asked.push(request.clone());
            if *request == Request::CancelSession {
                return Ok(Response::Success);
            }
            self.replies
                .pop_front()
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "greetd went away"))
        }
    }

    fn secret(prompt: &str) -> Response {
        Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: prompt.into(),
        }
    }

    fn cmd() -> Vec<String> {
        vec!["dbus-run-session".into(), "labwc".into()]
    }

    fn run(script: &mut Script) -> (Outcome, Vec<String>) {
        let mut notices = Vec::new();
        let outcome = attempt(script, "andre", "hunter2", &cmd(), &mut notices);
        (outcome, notices)
    }

    #[test]
    fn a_right_password_starts_the_session_with_the_command() {
        let mut g = Script::new([secret("Password: "), Response::Success, Response::Success]);
        assert_eq!(run(&mut g).0, Outcome::Started);
        assert_eq!(
            g.asked,
            [
                Request::CreateSession {
                    username: "andre".into()
                },
                Request::PostAuthMessageResponse {
                    response: Some("hunter2".into())
                },
                Request::StartSession {
                    cmd: cmd(),
                    env: vec![]
                },
            ]
        );
    }

    /// Without the cancel, greetd refuses the next attempt and nobody can log
    /// in until it is restarted.
    #[test]
    fn a_wrong_password_is_rejected_and_the_session_cancelled() {
        let mut g = Script::new([
            secret("Password: "),
            Response::Error {
                error_type: ErrorType::AuthError,
                description: "pam_authenticate: AUTH_ERR".into(),
            },
        ]);
        assert!(matches!(run(&mut g).0, Outcome::Rejected(_)));
        assert!(g.cancelled());
    }

    #[test]
    fn notices_are_passed_on_and_acknowledged() {
        let mut g = Script::new([
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Info,
                auth_message: "Your password expires in 3 days\n".into(),
            },
            secret("Password: "),
            Response::Success,
            Response::Success,
        ]);
        let (outcome, notices) = run(&mut g);
        assert_eq!(outcome, Outcome::Started);
        assert_eq!(notices, ["Your password expires in 3 days"]);
        assert!(
            g.asked
                .contains(&Request::PostAuthMessageResponse { response: None })
        );
    }

    /// Sending the password as the answer to a one-time-code prompt would put
    /// it somewhere it was never meant to go.
    #[test]
    fn a_second_question_is_never_answered_with_the_password() {
        let mut g = Script::new([secret("Password: "), secret("Verification code: ")]);
        let (outcome, _) = run(&mut g);
        assert!(matches!(outcome, Outcome::Failed(ref why) if why.contains("Verification code")));
        let answers = g
            .asked
            .iter()
            .filter(|r| matches!(r, Request::PostAuthMessageResponse { .. }))
            .count();
        assert_eq!(answers, 1);
        assert!(g.cancelled());
    }

    #[test]
    fn a_session_that_will_not_start_says_why_and_cancels() {
        let mut g = Script::new([
            secret("Password: "),
            Response::Success,
            Response::Error {
                error_type: ErrorType::Error,
                description: "no such file".into(),
            },
        ]);
        assert!(matches!(run(&mut g).0, Outcome::Failed(ref why) if why.contains("no such file")));
        assert!(g.cancelled());
    }

    #[test]
    fn losing_greetd_mid_conversation_is_a_failure_not_a_panic() {
        let mut g = Script::new([secret("Password: ")]);
        assert!(matches!(run(&mut g).0, Outcome::Failed(_)));
    }
}

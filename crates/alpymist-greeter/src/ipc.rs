//! greetd's IPC protocol.
//!
//! greetd hands its greeter a Unix socket in `GREETD_SOCK`. Every message, in
//! both directions, is a 32-bit length in native byte order followed by that
//! many bytes of JSON. That is the whole protocol; it is small enough that
//! writing it here costs less than trusting a crate to keep up with it, and
//! the framing is the part worth testing.
//!
//! The password travels in [`Request::PostAuthMessageResponse`], so
//! [`Request`]'s `Debug` is written by hand: a stray `{:?}` in a log line must
//! not be able to print one.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// The largest reply accepted. greetd's messages are a few hundred bytes; a
/// length far beyond that means the stream is out of step, and allocating
/// whatever it claims would be how that turns into running out of memory.
const MAX_MESSAGE: u32 = 64 * 1024;

/// What the greeter asks greetd to do.
#[derive(Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Begin logging in as `username`.
    CreateSession {
        /// The account.
        username: String,
    },
    /// Answer the question greetd last asked. `None` acknowledges a message
    /// that asked nothing.
    PostAuthMessageResponse {
        /// The answer — usually the password.
        response: Option<String>,
    },
    /// Start the session once authentication has succeeded.
    StartSession {
        /// The program and its arguments.
        cmd: Vec<String>,
        /// Extra `KEY=value` environment for the session.
        env: Vec<String>,
    },
    /// Abandon the session being set up.
    CancelSession,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateSession { username } => f
                .debug_struct("CreateSession")
                .field("username", username)
                .finish(),
            Self::PostAuthMessageResponse { response } => f
                .debug_struct("PostAuthMessageResponse")
                .field("response", &response.as_ref().map(|_| "<hidden>"))
                .finish(),
            Self::StartSession { cmd, env } => f
                .debug_struct("StartSession")
                .field("cmd", cmd)
                .field("env", env)
                .finish(),
            Self::CancelSession => f.write_str("CancelSession"),
        }
    }
}

/// Which kind of question or notice greetd is passing on from PAM.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMessageType {
    /// A question whose answer may be shown, such as a username.
    Visible,
    /// A question whose answer must be hidden: a password.
    Secret,
    /// Something to tell the user.
    Info,
    /// Something that went wrong, to tell the user.
    Error,
}

/// Why greetd refused.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// The credentials were wrong.
    AuthError,
    /// Anything else.
    Error,
}

/// What greetd answers.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The request did what it asked.
    Success,
    /// The request failed.
    Error {
        /// Whether it was the credentials.
        error_type: ErrorType,
        /// greetd's own words.
        description: String,
    },
    /// PAM has something to ask or say.
    AuthMessage {
        /// What kind.
        auth_message_type: AuthMessageType,
        /// The prompt or notice, as PAM wrote it.
        auth_message: String,
    },
}

/// Write one framed request.
///
/// # Errors
/// Whatever writing to `out` fails with.
pub fn send(out: &mut impl Write, request: &Request) -> io::Result<()> {
    let body = serde_json::to_vec(request).map_err(io::Error::other)?;
    let len = u32::try_from(body.len()).map_err(io::Error::other)?;
    out.write_all(&len.to_ne_bytes())?;
    out.write_all(&body)?;
    out.flush()
}

/// Read one framed response.
///
/// # Errors
/// A closed or short stream, an oversized length, or JSON that is not a
/// response.
pub fn receive(input: &mut impl Read) -> io::Result<Response> {
    let mut len = [0u8; 4];
    input.read_exact(&mut len)?;
    let len = u32::from_ne_bytes(len);
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("greetd sent a {len}-byte message; the stream is out of step"),
        ));
    }
    let mut body = vec![0u8; len as usize];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::{AuthMessageType, ErrorType, Request, Response, receive, send};

    fn frame(json: &str) -> Vec<u8> {
        let mut out = u32::try_from(json.len()).unwrap().to_ne_bytes().to_vec();
        out.extend_from_slice(json.as_bytes());
        out
    }

    #[test]
    fn requests_are_framed_with_their_length() {
        let mut out = Vec::new();
        send(
            &mut out,
            &Request::CreateSession {
                username: "andre".into(),
            },
        )
        .unwrap();
        let (len, body) = out.split_at(4);
        assert_eq!(
            u32::from_ne_bytes(len.try_into().unwrap()) as usize,
            body.len()
        );
        assert_eq!(body, br#"{"type":"create_session","username":"andre"}"#);
    }

    /// greetd's names, exactly: a request it cannot parse is dropped with no
    /// reply, and the greeter would wait forever.
    #[test]
    fn every_request_uses_greetds_names() {
        let cases = [
            (
                Request::PostAuthMessageResponse {
                    response: Some("pw".into()),
                },
                r#"{"type":"post_auth_message_response","response":"pw"}"#,
            ),
            (
                Request::PostAuthMessageResponse { response: None },
                r#"{"type":"post_auth_message_response","response":null}"#,
            ),
            (
                Request::StartSession {
                    cmd: vec!["labwc".into()],
                    env: vec![],
                },
                r#"{"type":"start_session","cmd":["labwc"],"env":[]}"#,
            ),
            (Request::CancelSession, r#"{"type":"cancel_session"}"#),
        ];
        for (request, json) in cases {
            let mut out = Vec::new();
            send(&mut out, &request).unwrap();
            assert_eq!(&out[4..], json.as_bytes());
        }
    }

    #[test]
    fn every_response_greetd_sends_is_understood() {
        let mut stream = Vec::new();
        stream.extend(frame(r#"{"type":"success"}"#));
        stream.extend(frame(
            r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#,
        ));
        stream.extend(frame(
            r#"{"type":"error","error_type":"auth_error","description":"pam_authenticate: AUTH_ERR"}"#,
        ));
        let mut input = stream.as_slice();
        assert_eq!(receive(&mut input).unwrap(), Response::Success);
        assert_eq!(
            receive(&mut input).unwrap(),
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                auth_message: "Password: ".into(),
            }
        );
        assert!(matches!(
            receive(&mut input).unwrap(),
            Response::Error {
                error_type: ErrorType::AuthError,
                ..
            }
        ));
    }

    #[test]
    fn a_stream_that_ends_early_is_an_error_not_a_hang() {
        let whole = frame(r#"{"type":"success"}"#);
        let mut short = &whole[..whole.len() - 3];
        assert!(receive(&mut short).is_err());
        let mut empty: &[u8] = &[];
        assert!(receive(&mut empty).is_err());
    }

    #[test]
    fn an_absurd_length_is_refused_before_allocating_it() {
        let mut input: &[u8] = &u32::MAX.to_ne_bytes();
        assert!(receive(&mut input).is_err());
    }
}

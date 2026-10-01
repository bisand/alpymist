//! Asking a provider's API, through `curl`.
//!
//! curl is on every Alpymist system (alpymist-tools), speaks TLS with the
//! system's certificates, and takes its whole request from a configuration
//! on its standard input — so a key in a header is never an argument, where
//! any process on the machine could read it.

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};

/// How long a provider may take to answer, in seconds.
const PATIENCE: &str = "20";

/// A string as curl's configuration quotes one.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// The configuration for a GET of `url` with these headers.
#[must_use]
pub fn request(url: &str, headers: &[(&str, &str)]) -> String {
    let mut config = format!("url = {}\n", quoted(url));
    for (name, value) in headers {
        let _ = writeln!(config, "header = {}", quoted(&format!("{name}: {value}")));
    }
    config
}

/// The configuration for a POST of `body`, as JSON, to `url`.
#[must_use]
pub fn request_with(url: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut config = request(url, headers);
    let _ = writeln!(config, "header = \"Content-Type: application/json\"");
    let _ = writeln!(config, "data = {}", quoted(body));
    config
}

/// GET `url` with `headers`, and its body when the answer is a success.
///
/// # Errors
/// curl is missing, the provider could not be reached, or it answered with
/// an error: its status and the start of what it said.
pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    send(&request(url, headers))
}

/// POST `body`, as JSON, to `url` with `headers`, and the answer's body
/// when it is a success.
///
/// # Errors
/// As [`get`].
pub fn post(url: &str, headers: &[(&str, &str)], body: &str) -> Result<String, String> {
    send(&request_with(url, headers, body))
}

/// Run curl over a configuration.
fn send(config: &str) -> Result<String, String> {
    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--globoff",
            "--proto",
            "=https",
            "--max-time",
            PATIENCE,
            "--write-out",
            "\n%{http_code}",
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(config.as_bytes())
            .map_err(|e| format!("curl: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr);
        let why = why.trim().trim_start_matches("curl: ");
        return Err(format!("could not reach it: {why}"));
    }
    answer(&String::from_utf8_lossy(&output.stdout))
}

/// A body from what curl printed: the body, a newline, and the status.
fn answer(printed: &str) -> Result<String, String> {
    let (body, status) = printed.rsplit_once('\n').unwrap_or(("", printed));
    match status.trim().parse::<u16>() {
        Ok(200..=299) => Ok(body.to_owned()),
        Ok(401 | 403) => Err(format!(
            "the key was refused (HTTP {status}): {}",
            brief(body)
        )),
        Ok(429) => Err("asked too often (HTTP 429)".into()),
        Ok(code) => Err(format!("HTTP {code}: {}", brief(body))),
        Err(_) => Err("no answer".into()),
    }
}

/// What an error body says, on one line: its message where it is JSON with
/// one, as every provider here answers, and the start of it otherwise.
fn brief(body: &str) -> String {
    let said = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            [&v["error"]["message"], &v["error"], &v["message"]]
                .into_iter()
                .find_map(|m| m.as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| body.to_owned());
    let line: String = said.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(160).collect()
}

#[cfg(test)]
mod tests {
    use super::{answer, request, request_with};

    #[test]
    fn a_request_is_curls_configuration_with_everything_quoted() {
        assert_eq!(
            request(
                "https://example.org/v1/key?a=1",
                &[("Authorization", "Bearer sk-\"x\"\\y"), ("x-version", "1")]
            ),
            "url = \"https://example.org/v1/key?a=1\"\n\
             header = \"Authorization: Bearer sk-\\\"x\\\"\\\\y\"\n\
             header = \"x-version: 1\"\n"
        );
    }

    #[test]
    fn a_post_carries_its_body_quoted_too() {
        assert_eq!(
            request_with("https://example.org/v1:ask", &[], "{\"project\": \"a\"}"),
            "url = \"https://example.org/v1:ask\"\n\
             header = \"Content-Type: application/json\"\n\
             data = \"{\\\"project\\\": \\\"a\\\"}\"\n"
        );
    }

    #[test]
    fn an_answer_is_its_body_or_why_not() {
        assert_eq!(answer("{\"a\":\n1}\n200").as_deref(), Ok("{\"a\":\n1}"));
        let refused = answer("{\"error\": \"bad key\"}\n401").unwrap_err();
        assert_eq!(refused, "the key was refused (HTTP 401): bad key");
        let nested = answer(
            "{\"error\": {\"message\": \"User not found.\", \"code\": 401}}
401",
        );
        assert_eq!(
            nested.unwrap_err(),
            "the key was refused (HTTP 401): User not found."
        );
        assert!(answer("\n429").unwrap_err().contains("too often"));
        assert!(answer("oops\n500").unwrap_err().starts_with("HTTP 500"));
    }
}

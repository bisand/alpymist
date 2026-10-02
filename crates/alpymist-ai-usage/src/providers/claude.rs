//! A Claude subscription: the 5-hour and weekly limits, as Claude Code
//! says them.
//!
//! Claude Code hands its status line command a JSON document on every
//! update, and for Pro and Max subscribers that document carries
//! `rate_limits`: how much of each window is used and when it resets. That
//! is the documented way to read them, and the only one used here. So this
//! provider has three parts:
//!
//! - **The status line command** (`alpymist-ai-provider claude-statusline`),
//!   which keeps `rate_limits` in a file and then runs whatever status line
//!   was there before, passing its output on, so nothing on screen changes.
//! - **The reader**, which reports from that file.
//! - **Turning it on and off**, which puts the command in
//!   `~/.claude/settings.json` and takes it out again, the status line that
//!   was there kept beside Alpymist's own settings meanwhile.
//!
//! An account with no status line of its own has Alpymist's, [`DEFAULT`]:
//! a script in the prompt's colours, which `/etc/skel` names for a new
//! account and turning this on gives to one that had none. It is a status
//! line like any other, kept and run after this one, and left in place when
//! this is turned off.
//!
//! What it cannot do: the figures are as fresh as the last time Claude Code
//! ran on this account, and nothing is known before the first answer of a
//! session. The login Claude Code keeps is never read: Anthropic's terms
//! keep that for Claude Code itself.

use super::{json, number};
use crate::report::{Meter, Report};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// The status line command Claude Code is given.
pub const COMMAND: &str = "alpymist-ai-provider claude-statusline";

/// The status line Alpymist ships, for an account with none of its own.
pub const DEFAULT: &str = "/usr/share/alpymist/claude-statusline.sh";

/// [`DEFAULT`], as Claude Code's settings name a status line.
#[must_use]
pub fn default_status_line() -> Value {
    json!({ "type": "command", "command": DEFAULT })
}

/// The windows Claude Code names, and what to call them.
const WINDOWS: [(&str, &str); 3] = [
    ("five_hour", "5-hour limit"),
    ("seven_day", "Weekly limit"),
    ("spend_limit", "Spend limit"),
];

/// What to keep of a status line document seen at `now`: its `rate_limits`,
/// and when. `None` when it has none — before a session's first answer, or
/// on an account with no subscription — so that what was known stays known.
#[must_use]
pub fn capture(document: &str, now: i64) -> Option<String> {
    let document: Value = serde_json::from_str(document).ok()?;
    let limits = document.get("rate_limits")?.as_object()?;
    if limits.is_empty() {
        return None;
    }
    Some(json!({ "captured_at": now, "rate_limits": limits }).to_string())
}

/// The report from what was kept.
///
/// # Errors
/// Nothing was kept, or it has no window in it.
pub fn report(kept: &str, now: i64) -> Result<Report, String> {
    let kept = json(kept)?;
    let mut meters = Vec::new();
    for (key, label) in WINDOWS {
        let window = &kept["rate_limits"][key];
        let Some(used) = number(&window["used_percentage"]) else {
            continue;
        };
        let resets_at = window["resets_at"].as_i64();
        // A window whose reset has passed began again unused, and Claude
        // Code has not run since to say otherwise.
        let (used, resets_at) = match resets_at {
            Some(at) if at <= now => (0.0, None),
            other => (used / 100.0, other),
        };
        meters.push(Meter::Window {
            label: label.to_owned(),
            used,
            resets_at,
        });
    }
    if meters.is_empty() {
        return Err("Claude Code reported no limits: they are a Pro or Max subscription's".into());
    }
    Ok(Report { plan: None, meters })
}

/// Where the status line's `rate_limits` are kept.
#[must_use]
pub fn kept_path() -> Option<PathBuf> {
    crate::store::path("claude-statusline")
}

/// Where the status line that was there before is kept while this is on.
#[must_use]
pub fn before_path() -> Option<PathBuf> {
    crate::config::path().map(|p| p.with_file_name("ai-usage-claude-statusline.json"))
}

/// Claude Code's own settings, for this account.
#[must_use]
pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/settings.json"))
}

/// Whether `status_line` is the one this sets.
fn ours(status_line: &Value) -> bool {
    status_line["command"]
        .as_str()
        .is_some_and(|c| c.trim() == COMMAND)
}

/// Claude Code's settings with this as the status line, and the status line
/// that was there, to keep. `settings` is the file's text, or `None` when
/// there is no file yet. Everything else in it is left as it is.
///
/// # Errors
/// The file is there and is not a JSON object: not something to write over.
pub fn enable(settings: Option<&str>) -> Result<(String, Option<Value>), String> {
    let mut root = object(settings)?;
    let before = root.get("statusLine").filter(|s| !ours(s)).cloned();
    root.insert(
        "statusLine".into(),
        json!({ "type": "command", "command": COMMAND }),
    );
    Ok((pretty(&root), before))
}

/// Claude Code's settings with this taken out, and `before` put back where
/// there was one. A status line someone set since is theirs, and stays.
///
/// # Errors
/// As [`enable`].
pub fn disable(settings: Option<&str>, before: Option<Value>) -> Result<String, String> {
    let mut root = object(settings)?;
    if root.get("statusLine").is_some_and(ours) {
        match before {
            Some(before) => root.insert("statusLine".into(), before),
            None => root.remove("statusLine"),
        };
    }
    Ok(pretty(&root))
}

fn object(settings: Option<&str>) -> Result<Map<String, Value>, String> {
    match settings.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Map::new()),
        Some(text) => match serde_json::from_str(text) {
            Ok(Value::Object(root)) => Ok(root),
            _ => Err("Claude Code's settings.json is not something this can add to".into()),
        },
    }
}

fn pretty(root: &Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(root).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

/// The command of the status line that was there before, to run after this.
#[must_use]
pub fn before_command(before: &str) -> Option<String> {
    let before: Value = serde_json::from_str(before).ok()?;
    (before["type"].as_str() == Some("command"))
        .then(|| before["command"].as_str().map(str::to_owned))
        .flatten()
        .filter(|c| !c.trim().is_empty() && c.trim() != COMMAND)
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND, DEFAULT, before_command, capture, default_status_line, disable, enable, report,
    };
    use serde_json::{Value, json};

    /// The `rate_limits` of Claude Code's documented status line example.
    const DOCUMENT: &str = r#"{ "model": { "id": "claude-opus-5-5", "display_name": "Opus" },
        "version": "2.1.90",
        "rate_limits": {
          "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
          "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 },
          "spend_limit": { "used_percentage": 62.8, "resets_at": 1740787200 } } }"#;

    #[test]
    fn the_limits_are_kept_and_reported_and_a_passed_window_starts_again() {
        let kept = capture(DOCUMENT, 1_738_420_000).unwrap();
        let r = report(&kept, 1_738_420_000).unwrap();
        let said: Vec<String> = r
            .meters
            .iter()
            .map(|m| format!("{}: {}", m.label(), m.says(1_738_420_000)))
            .collect();
        assert_eq!(
            said,
            [
                "5-hour limit: 24% used, resets in 1 h 33 min",
                "Weekly limit: 41% used, resets in 5 days",
                "Spend limit: 63% used, resets in 27 days"
            ]
        );
        // An hour after the 5-hour window reset, with nothing said since.
        let later = report(&kept, 1_738_429_200).unwrap();
        assert_eq!(later.meters[0].says(1_738_429_200), "0% used");
        assert!((later.meters[1].used().unwrap() - 0.412).abs() < 1e-9);

        // Before a session's first answer there is nothing to keep, and what
        // was kept is not written over with it.
        assert_eq!(capture(r#"{"model": {"id": "x"}}"#, 5), None);
        assert_eq!(capture(r#"{"rate_limits": {}}"#, 5), None);
        assert!(report(r#"{"captured_at": 1, "rate_limits": {"other": {}}}"#, 5).is_err());
    }

    #[test]
    fn turning_it_on_keeps_what_was_there_and_turning_it_off_puts_it_back() {
        let theirs = r#"{ "theme": "auto",
            "statusLine": { "type": "command", "command": "~/.claude/statusline.sh", "padding": 2 } }"#;
        let (on, before) = enable(Some(theirs)).unwrap();
        let root: Value = serde_json::from_str(&on).unwrap();
        assert_eq!(root["statusLine"]["command"], COMMAND);
        assert_eq!(root["theme"], "auto", "the rest is left alone");
        let before = before.unwrap();
        assert_eq!(
            before_command(&before.to_string()).as_deref(),
            Some("~/.claude/statusline.sh")
        );
        // On twice is on once, and keeps nothing of its own as "before".
        let (again, none) = enable(Some(&on)).unwrap();
        assert_eq!(again, on);
        assert_eq!(none, None);

        let off = disable(Some(&on), Some(before)).unwrap();
        let root: Value = serde_json::from_str(&off).unwrap();
        assert_eq!(root["statusLine"]["command"], "~/.claude/statusline.sh");
        assert_eq!(root["statusLine"]["padding"], 2);

        // No file, and none before: made, and taken out again.
        let (fresh, before) = enable(None).unwrap();
        assert_eq!(before, None);
        assert_eq!(disable(Some(&fresh), None).unwrap(), "{}\n");
        // One set by hand since stays.
        let mine = json!({ "statusLine": { "type": "command", "command": "mine" } }).to_string();
        assert!(disable(Some(&mine), None).unwrap().contains("mine"));
        assert!(enable(Some("[1, 2]")).is_err());
    }

    /// An account with none is given Alpymist's as the one that was there:
    /// it is run after this, and is what turning this off leaves.
    #[test]
    fn the_default_is_a_status_line_like_any_other() {
        let (on, before) = enable(None).unwrap();
        assert_eq!(before, None);
        let before = default_status_line();
        assert_eq!(
            before_command(&before.to_string()).as_deref(),
            Some(DEFAULT)
        );
        let off = disable(Some(&on), Some(before)).unwrap();
        let root: Value = serde_json::from_str(&off).unwrap();
        assert_eq!(root["statusLine"], default_status_line());
    }
}

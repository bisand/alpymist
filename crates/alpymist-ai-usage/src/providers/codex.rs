//! A `ChatGPT` subscription's Codex limits: the 5-hour and weekly windows.
//!
//! **Undocumented.** `GET https://chatgpt.com/backend-api/wham/usage` is
//! what the Codex CLI asks for its own `/status`, with the login it keeps in
//! `~/.codex/auth.json`. The fields here are the ones in Codex's source
//! (`codex-backend-openapi-models`): `plan_type`, and under `rate_limit` a
//! `primary_window` and a `secondary_window`, each with `used_percent` from
//! 0 to 100, `limit_window_seconds` and `reset_at` in Unix seconds.
//!
//! A workspace plan with a credit allowance has no windows: `rate_limit` is
//! null, and `spend_control.individual_limit` carries `used_percent` and
//! `reset_at` for the member's credits instead. That shape is from a live
//! answer to a business account, 2026-10-01.
//!
//! The login is read and never written. Codex renews it when Codex is used;
//! one that has run out is said to have, and the last answer stays.

use super::{USER_AGENT, home, json, number, plan};
use crate::http;
use crate::report::{Meter, Report};
use crate::when;
use serde_json::Value;
use std::path::PathBuf;

const USAGE: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Where Codex keeps its login: under `CODEX_HOME`, or `~/.codex`.
#[must_use]
pub fn login_path() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| home(".codex"))
        .map(|dir| dir.join("auth.json"))
}

/// The access token and the account it is for, from Codex's login file.
///
/// # Errors
/// The file is not a `ChatGPT` login: Codex is logged in with an API key, or
/// not at all.
pub fn login(file: &str) -> Result<(String, Option<String>), String> {
    let auth = json(file)?;
    let tokens = &auth["tokens"];
    let token = tokens["access_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or("Codex is not logged in with a ChatGPT account: run codex login")?;
    let account = tokens["account_id"].as_str().map(str::to_owned);
    Ok((token.to_owned(), account))
}

/// What a window of `seconds` is called.
fn window_label(seconds: Option<f64>) -> String {
    match seconds {
        // Whole hours and days, however the vendor rounds them.
        Some(s) if (17_000.0..=19_000.0).contains(&s) => "5-hour limit".into(),
        Some(s) if (600_000.0..=610_000.0).contains(&s) => "Weekly limit".into(),
        #[allow(clippy::cast_possible_truncation)]
        Some(s) if s >= 60.0 => format!("{} limit", when::span(s as i64)),
        _ => "Limit".into(),
    }
}

fn window(snapshot: &Value) -> Option<Meter> {
    let used = number(&snapshot["used_percent"])?;
    #[allow(clippy::cast_possible_truncation)]
    let resets_at = number(&snapshot["reset_at"])
        .filter(|at| *at > 0.0)
        .map(|at| at as i64);
    Some(Meter::Window {
        label: window_label(number(&snapshot["limit_window_seconds"])),
        used: (used / 100.0).clamp(0.0, 1.0),
        resets_at,
    })
}

/// A workspace member's credit allowance.
fn credits(limit: &Value) -> Option<Meter> {
    let used = number(&limit["used_percent"]).or_else(|| {
        let all = number(&limit["limit"]).filter(|all| *all > 0.0)?;
        Some(number(&limit["used"])? / all * 100.0)
    })?;
    #[allow(clippy::cast_possible_truncation)]
    let resets_at = number(&limit["reset_at"])
        .filter(|at| *at > 0.0)
        .map(|at| at as i64);
    Some(Meter::Window {
        label: "Credits".into(),
        used: (used / 100.0).clamp(0.0, 1.0),
        resets_at,
    })
}

/// The report from the endpoint's answer.
///
/// # Errors
/// The answer has no window in it.
pub fn report(body: &str) -> Result<Report, String> {
    let usage = json(body)?;
    let limits = &usage["rate_limit"];
    let meters: Vec<Meter> = [&limits["primary_window"], &limits["secondary_window"]]
        .into_iter()
        .filter_map(window)
        .chain(credits(&usage["spend_control"]["individual_limit"]))
        .collect();
    if meters.is_empty() {
        return Err("the answer has no limits in it: OpenAI may have changed it".into());
    }
    Ok(Report {
        plan: usage["plan_type"].as_str().and_then(plan),
        meters,
    })
}

/// Ask `ChatGPT`, as Codex does.
///
/// # Errors
/// Codex is not logged in, its login has run out, or `ChatGPT` could not be
/// asked.
pub fn fetch() -> Result<Report, String> {
    let path = login_path().ok_or("no home directory")?;
    let file = std::fs::read_to_string(&path)
        .map_err(|_| "Codex is not logged in: run codex login".to_owned())?;
    let (token, account) = login(&file)?;
    let bearer = format!("Bearer {token}");
    let mut headers = vec![
        ("Authorization", bearer.as_str()),
        ("User-Agent", USER_AGENT),
    ];
    if let Some(account) = &account {
        headers.push(("ChatGPT-Account-Id", account));
    }
    let body = http::get(USAGE, &headers).map_err(|why| {
        if why.starts_with("the key was refused") {
            "Codex's login has run out: it is renewed when Codex is next used".to_owned()
        } else {
            why
        }
    })?;
    report(&body)
}

#[cfg(test)]
mod tests {
    use super::{login, report};

    #[test]
    fn the_two_windows_are_named_by_their_length() {
        let r = report(
            r#"{ "plan_type": "plus",
                 "rate_limit": { "allowed": true, "limit_reached": false,
                   "primary_window": { "used_percent": 42, "limit_window_seconds": 18000,
                                       "reset_after_seconds": 7800, "reset_at": 7800 },
                   "secondary_window": { "used_percent": 86, "limit_window_seconds": 604800,
                                         "reset_after_seconds": 259200, "reset_at": 259200 } },
                 "credits": null }"#,
        )
        .unwrap();
        assert_eq!(r.plan.as_deref(), Some("Plus"));
        let said: Vec<String> = r
            .meters
            .iter()
            .map(|m| format!("{}: {}", m.label(), m.says(0)))
            .collect();
        assert_eq!(
            said,
            [
                "5-hour limit: 42% used, resets in 2 h 10 min",
                "Weekly limit: 86% used, resets in 3 days"
            ]
        );
    }

    /// A live answer to a business account, the fields read.
    #[test]
    fn a_workspace_plan_says_its_credits() {
        let r = report(
            r#"{ "plan_type": "business", "rate_limit": null,
                 "credits": { "has_credits": true, "unlimited": false, "balance": null },
                 "spend_control": { "reached": false, "individual_limit": {
                   "source": "workspace_spend_controls", "unit": "credit",
                   "limit": "1500", "used": "375.0", "remaining": "1125.0",
                   "used_percent": 25, "remaining_percent": 75,
                   "reset_after_seconds": 7200, "reset_at": 7200 } } }"#,
        )
        .unwrap();
        assert_eq!(r.plan.as_deref(), Some("Business"));
        assert_eq!(r.meters.len(), 1);
        assert_eq!(r.meters[0].label(), "Credits");
        assert_eq!(r.meters[0].says(0), "25% used, resets in 2 h");
    }

    #[test]
    fn one_window_or_none() {
        let one = r#"{ "plan_type": "free", "rate_limit": { "primary_window":
            { "used_percent": 100, "limit_window_seconds": 3600, "reset_at": 0 },
            "secondary_window": null } }"#;
        let r = report(one).unwrap();
        assert_eq!(r.meters.len(), 1);
        assert_eq!(r.meters[0].label(), "1 h limit");
        assert_eq!(r.meters[0].says(0), "100% used");
        assert!(report(r#"{ "plan_type": "free", "rate_limit": null }"#).is_err());
    }

    #[test]
    fn the_login_is_a_chatgpt_one_or_it_is_said() {
        let (token, account) = login(
            r#"{ "OPENAI_API_KEY": null, "tokens": { "id_token": "a.b.c",
                 "access_token": "at", "refresh_token": "rt", "account_id": "acc" } }"#,
        )
        .unwrap();
        assert_eq!((token.as_str(), account.as_deref()), ("at", Some("acc")));
        let key_only = login(r#"{ "OPENAI_API_KEY": "sk-x" }"#).unwrap_err();
        assert!(key_only.contains("codex login"));
    }
}

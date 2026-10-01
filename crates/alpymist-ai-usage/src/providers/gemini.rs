//! Gemini CLI's quota: how much of each model's allowance is used.
//!
//! **Undocumented, and the least sure of the three.** Gemini CLI asks
//! `https://cloudcode-pa.googleapis.com/v1internal` for its own `/stats`,
//! with the Google login it keeps in `~/.gemini/oauth_creds.json`:
//! `loadCodeAssist` says which project the account's use is counted under,
//! and `retrieveUserQuota` answers `buckets`, each with a `modelId`, a
//! `remainingFraction` from 0 to 1 and a `resetTime`. Those names are from
//! Gemini CLI's source. Others report the fraction staying at 1 at times.
//!
//! The login is read and never written, and it lasts an hour. Renewing it
//! takes Gemini CLI's own client secret, which is not ours to use, so this
//! is as fresh as Gemini CLI's last use: the same bargain as Claude's.

use super::{USER_AGENT, home, json, number};
use crate::http;
use crate::report::{Meter, Report};
use crate::when;
use std::path::PathBuf;

const BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

/// What Gemini CLI says it is when it asks which project to use.
const METADATA: &str = r#"{"metadata":{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}}"#;

/// What is said when the login is past its hour.
const RUN_OUT: &str = "Gemini CLI's login has run out: it is renewed when Gemini CLI is next used";

/// Where Gemini CLI keeps its login.
#[must_use]
pub fn login_path() -> Option<PathBuf> {
    home(".gemini/oauth_creds.json")
}

/// The access token from Gemini CLI's login file, while it lasts.
///
/// # Errors
/// There is no token, or it ran out before `now`.
pub fn login(file: &str, now: i64) -> Result<String, String> {
    let creds = json(file)?;
    let token = creds["access_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or("Gemini CLI is not logged in with a Google account: run gemini")?;
    // Milliseconds since the epoch.
    #[allow(clippy::cast_possible_truncation)]
    let until = number(&creds["expiry_date"]).map(|at| (at / 1000.0) as i64);
    if until.is_some_and(|until| until <= now) {
        return Err(RUN_OUT.into());
    }
    Ok(token.to_owned())
}

/// The project and the tier's name, from `loadCodeAssist`'s answer.
///
/// # Errors
/// The answer names no project.
pub fn project(body: &str) -> Result<(String, Option<String>), String> {
    let loaded = json(body)?;
    let project = &loaded["cloudaicompanionProject"];
    let id = project
        .as_str()
        .or_else(|| project["id"].as_str())
        .filter(|id| !id.is_empty())
        .ok_or("Google named no project for this account: use Gemini CLI once first")?;
    let tier = [&loaded["paidTier"]["name"], &loaded["currentTier"]["name"]]
        .into_iter()
        .find_map(|name| name.as_str())
        .map(str::to_owned);
    Ok((id.to_owned(), tier))
}

/// The report from `retrieveUserQuota`'s answer: a meter for each model, the
/// one closest to its limit where a model has several buckets.
///
/// # Errors
/// The answer has no bucket in it.
pub fn report(body: &str, plan: Option<String>) -> Result<Report, String> {
    let quota = json(body)?;
    let mut models: Vec<(String, f64, Option<i64>)> = Vec::new();
    for bucket in quota["buckets"].as_array().into_iter().flatten() {
        let (Some(model), Some(left)) = (
            bucket["modelId"].as_str(),
            number(&bucket["remainingFraction"]),
        ) else {
            continue;
        };
        let used = (1.0 - left).clamp(0.0, 1.0);
        let resets_at = bucket["resetTime"].as_str().and_then(when::parse_rfc3339);
        match models.iter_mut().find(|(name, _, _)| name == model) {
            Some(seen) if used > seen.1 => (seen.1, seen.2) = (used, resets_at),
            Some(_) => {}
            None => models.push((model.to_owned(), used, resets_at)),
        }
    }
    if models.is_empty() {
        return Err("the answer has no quota in it: Google may have changed it".into());
    }
    Ok(Report {
        plan,
        meters: models
            .into_iter()
            .map(|(label, used, resets_at)| Meter::Window {
                label,
                used,
                resets_at,
            })
            .collect(),
    })
}

/// Ask Google, as Gemini CLI does.
///
/// # Errors
/// Gemini CLI is not logged in, its login has run out, or Google could not
/// be asked.
pub fn fetch() -> Result<Report, String> {
    let path = login_path().ok_or("no home directory")?;
    let file = std::fs::read_to_string(&path)
        .map_err(|_| "Gemini CLI is not logged in: run gemini".to_owned())?;
    let token = login(&file, when::now())?;
    let bearer = format!("Bearer {token}");
    let headers = [
        ("Authorization", bearer.as_str()),
        ("User-Agent", USER_AGENT),
    ];
    let ask = |method: &str, body: &str| {
        http::post(&format!("{BASE}:{method}"), &headers, body).map_err(|why| {
            if why.starts_with("the key was refused (HTTP 401)") {
                RUN_OUT.to_owned()
            } else {
                why
            }
        })
    };
    let (project, tier) = project(&ask("loadCodeAssist", METADATA)?)?;
    let asked = serde_json::json!({ "project": project }).to_string();
    report(&ask("retrieveUserQuota", &asked)?, tier)
}

#[cfg(test)]
mod tests {
    use super::{login, project, report};

    #[test]
    fn each_model_has_a_meter_and_its_worst_bucket() {
        let r = report(
            r#"{ "buckets": [
                 { "modelId": "gemini-2.5-pro", "remainingFraction": 0.25,
                   "resetTime": "1970-01-01T02:00:00Z", "tokenType": "REQUESTS" },
                 { "modelId": "gemini-2.5-pro", "remainingFraction": 0.9, "tokenType": "TOKENS" },
                 { "modelId": "gemini-2.5-flash", "remainingFraction": 1 },
                 { "remainingAmount": "12" } ] }"#,
            Some("Gemini Code Assist for individuals".into()),
        )
        .unwrap();
        let said: Vec<String> = r
            .meters
            .iter()
            .map(|m| format!("{}: {}", m.label(), m.says(0)))
            .collect();
        assert_eq!(
            said,
            [
                "gemini-2.5-pro: 75% used, resets in 2 h",
                "gemini-2.5-flash: 0% used"
            ]
        );
        assert!(report(r#"{ "buckets": [] }"#, None).is_err());
        assert!(report("{}", None).is_err());
    }

    #[test]
    fn the_project_is_named_either_way_and_the_paid_tier_first() {
        let (id, tier) = project(
            r#"{ "cloudaicompanionProject": "plucky-123",
                 "currentTier": { "id": "free-tier", "name": "Free" },
                 "paidTier": { "id": "g1-pro-tier", "name": "Google AI Pro" } }"#,
        )
        .unwrap();
        assert_eq!(
            (id.as_str(), tier.as_deref()),
            ("plucky-123", Some("Google AI Pro"))
        );
        let (id, tier) = project(r#"{ "cloudaicompanionProject": { "id": "p" } }"#).unwrap();
        assert_eq!((id.as_str(), tier), ("p", None));
        assert!(project(r#"{ "currentTier": null }"#).is_err());
    }

    #[test]
    fn a_login_lasts_until_its_expiry() {
        let file = r#"{ "access_token": "ya29.x", "refresh_token": "1//y",
                        "expiry_date": 5000000, "token_type": "Bearer" }"#;
        assert_eq!(login(file, 4000).as_deref(), Ok("ya29.x"));
        assert!(login(file, 5000).unwrap_err().contains("run out"));
        assert!(login("{}", 0).unwrap_err().contains("not logged in"));
    }
}

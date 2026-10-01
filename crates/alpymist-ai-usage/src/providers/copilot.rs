//! GitHub Copilot: the month's allowance, what is left of it, and when it
//! starts again.
//!
//! **Undocumented.** `GET /copilot_internal/user` is what GitHub's own
//! editors ask; GitHub documents no way for a person to read their own
//! allowance. It is asked through `gh api`, so the login never leaves `gh`:
//! this program is handed no key and reads none.
//!
//! Two shapes have been seen. A paid plan answers `quota_snapshots`, each
//! with `entitlement`, `remaining`, `percent_remaining` and `unlimited`, and
//! `quota_reset_date`. The free plan answers `limited_user_quotas` (what is
//! left) beside `monthly_quotas` (what there was), and
//! `limited_user_reset_date`.

use super::{json, number, plan};
use crate::report::{Meter, Report};
use crate::when;
use serde_json::Value;
use std::process::Command;

/// The quotas shown, in order, and what each is called.
const QUOTAS: [(&str, &str); 3] = [
    ("premium_interactions", "Premium requests"),
    ("chat", "Chat"),
    ("completions", "Completions"),
];

/// A reset date: a whole RFC 3339 time, or only a date.
fn reset(value: &Value) -> Option<i64> {
    let text = value.as_str()?;
    when::parse_rfc3339(text).or_else(|| when::parse_rfc3339(&format!("{text}T00:00:00Z")))
}

/// The report from the endpoint's answer.
///
/// # Errors
/// The answer is neither shape, or the plan limits nothing.
pub fn report(body: &str) -> Result<Report, String> {
    let user = json(body)?;
    let mut meters = Vec::new();

    let resets_at = reset(&user["quota_reset_date"]);
    for (key, label) in QUOTAS {
        let quota = &user["quota_snapshots"][key];
        if quota["unlimited"].as_bool() == Some(true) {
            continue;
        }
        let left = number(&quota["percent_remaining"]).or_else(|| {
            let entitlement = number(&quota["entitlement"]).filter(|e| *e > 0.0)?;
            Some(number(&quota["remaining"])? / entitlement * 100.0)
        });
        if let Some(left) = left {
            meters.push(Meter::Window {
                label: label.into(),
                used: (1.0 - left / 100.0).clamp(0.0, 1.0),
                resets_at,
            });
        }
    }

    let resets_at = reset(&user["limited_user_reset_date"]);
    for (key, label) in QUOTAS {
        let left = number(&user["limited_user_quotas"][key]);
        let all = number(&user["monthly_quotas"][key]).filter(|a| *a > 0.0);
        if let (Some(left), Some(all)) = (left, all) {
            meters.push(Meter::Window {
                label: label.into(),
                used: (1.0 - left / all).clamp(0.0, 1.0),
                resets_at,
            });
        }
    }

    if meters.is_empty() {
        return Err(if user["quota_snapshots"].is_object() {
            "this plan limits nothing that can be read".into()
        } else {
            "the answer has no allowance in it: GitHub may have changed it".to_owned()
        });
    }
    Ok(Report {
        plan: user["copilot_plan"].as_str().and_then(plan),
        meters,
    })
}

/// Ask GitHub, through `gh`.
///
/// # Errors
/// `gh` is missing or not logged in, or GitHub refused.
pub fn fetch() -> Result<Report, String> {
    let output = Command::new("gh")
        .args(["api", "copilot_internal/user"])
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.lines().next().unwrap_or("").trim();
        return Err(if said.contains("gh auth login") {
            "gh is not logged in to GitHub: run gh auth login".into()
        } else if said.is_empty() {
            "gh failed".into()
        } else {
            said.to_owned()
        });
    }
    report(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::report;

    #[test]
    fn a_paid_plan_says_what_is_left_of_each_allowance() {
        let r = report(
            r#"{ "copilot_plan": "pro", "quota_reset_date": "1970-01-02T00:00:00Z",
                 "quota_snapshots": {
                   "premium_interactions": { "percent_remaining": 80, "entitlement": 300,
                                             "remaining": 240, "unlimited": false },
                   "chat": { "percent_remaining": 100, "entitlement": 0, "unlimited": true },
                   "completions": { "entitlement": 200, "remaining": 50 } } }"#,
        )
        .unwrap();
        assert_eq!(r.plan.as_deref(), Some("Pro"));
        let said: Vec<String> = r
            .meters
            .iter()
            .map(|m| format!("{}: {}", m.label(), m.says(0)))
            .collect();
        assert_eq!(
            said,
            [
                "Premium requests: 20% used, resets in 24 h",
                "Completions: 75% used, resets in 24 h"
            ]
        );
    }

    #[test]
    fn the_free_plan_is_read_from_its_own_shape() {
        let r = report(
            r#"{ "copilot_plan": "individual", "access_type_sku": "free_limited_copilot",
                 "limited_user_quotas": { "chat": 410, "completions": 4000 },
                 "monthly_quotas": { "chat": 500, "completions": 4000 },
                 "limited_user_reset_date": "1970-01-03" }"#,
        )
        .unwrap();
        assert_eq!(r.meters[0].says(0), "18% used, resets in 2 days");
        assert_eq!(r.meters[1].used(), Some(0.0));
    }

    #[test]
    fn nothing_limited_and_nothing_known_say_so() {
        let unlimited = r#"{ "quota_snapshots": { "chat": { "unlimited": true } } }"#;
        assert!(report(unlimited).unwrap_err().contains("limits nothing"));
        assert!(report("{}").unwrap_err().contains("changed"));
        assert!(report("<html>").is_err());
    }
}

//! `OpenRouter`: what this key has used, its limit, and the credits left.
//!
//! Documented, with an ordinary key: `GET /api/v1/key` says what the key
//! has used this month and its limit. `GET /api/v1/credits`, the account's
//! credits bought and used, needs a management key; given one, the balance
//! is shown too, and without one it is simply not.

use super::{json, number};
use crate::http;
use crate::report::{Meter, Report};
use crate::run::Input;

const KEY: &str = "https://openrouter.ai/api/v1/key";
const CREDITS: &str = "https://openrouter.ai/api/v1/credits";

/// The report from `/key`'s answer, and `/credits`' where there is one.
///
/// # Errors
/// The key's answer is not what `OpenRouter` documents.
pub fn report(key: &str, credits: Option<&str>) -> Result<Report, String> {
    let key = json(key)?;
    let data = &key["data"];
    let month = number(&data["usage_monthly"])
        .or_else(|| number(&data["usage"]))
        .ok_or("the answer has no usage in it")?;
    let mut meters = Vec::new();

    if let Some(credits) = credits.and_then(|c| json(c).ok()) {
        let total = number(&credits["data"]["total_credits"]);
        let used = number(&credits["data"]["total_usage"]);
        if let (Some(total), Some(used)) = (total, used) {
            meters.push(Meter::Balance {
                label: "Credits".into(),
                remaining: (total - used).max(0.0),
                total: Some(total),
                currency: "USD".into(),
            });
        }
    }
    // A key with a limit: what is left of it, and how it starts again.
    if let (Some(limit), Some(remaining)) =
        (number(&data["limit"]), number(&data["limit_remaining"]))
    {
        let label = match data["limit_reset"].as_str() {
            Some(reset) if !reset.is_empty() => format!("Key limit, {reset}"),
            _ => "Key limit".into(),
        };
        meters.push(Meter::Balance {
            label,
            remaining: remaining.max(0.0),
            total: Some(limit),
            currency: "USD".into(),
        });
    }
    meters.push(Meter::Spend {
        label: "This month".into(),
        amount: month,
        currency: "USD".into(),
        budget: None,
    });
    Ok(Report {
        plan: data["is_free_tier"]
            .as_bool()
            .map(|free| if free { "Free tier" } else { "Paid" }.to_owned()),
        meters,
    })
}

/// Ask `OpenRouter`.
///
/// # Errors
/// No key, or `OpenRouter` could not be asked or refused.
pub fn fetch(input: &Input) -> Result<Report, String> {
    let bearer = |key: &str| format!("Bearer {key}");
    let key = http::get(KEY, &[("Authorization", &bearer(input.need("api-key")?))])?;
    let credits = input
        .credentials
        .get("management-key")
        .filter(|k| !k.is_empty())
        .and_then(|k| http::get(CREDITS, &[("Authorization", &bearer(k))]).ok());
    report(&key, credits.as_deref())
}

#[cfg(test)]
mod tests {
    use super::report;
    use crate::report::Meter;

    /// The example in `OpenRouter`'s `OpenAPI` specification, the fields read.
    const KEY: &str = r#"{ "data": {
        "is_free_tier": false, "label": "sk-or-v1-au7...890",
        "limit": 100, "limit_remaining": 74.5, "limit_reset": "monthly",
        "usage": 25.5, "usage_daily": 25.5, "usage_monthly": 25.5, "usage_weekly": 25.5 } }"#;
    const CREDITS: &str = r#"{ "data": { "total_credits": 100.5, "total_usage": 25.75 } }"#;

    #[test]
    fn the_key_says_its_limit_and_the_month_and_a_management_key_the_balance() {
        let r = report(KEY, Some(CREDITS)).unwrap();
        assert_eq!(r.plan.as_deref(), Some("Paid"));
        let said: Vec<String> = r
            .meters
            .iter()
            .map(|m| format!("{}: {}", m.label(), m.says(0)))
            .collect();
        assert_eq!(
            said,
            [
                "Credits: $74.75 left of $100.50",
                "Key limit, monthly: $74.50 left of $100.00",
                "This month: $25.50 spent"
            ]
        );
        // The credits are the closer of the two to running out.
        assert!((r.worst().unwrap() - 25.75 / 100.5).abs() < 1e-9);
    }

    #[test]
    fn a_key_without_a_limit_says_only_what_it_spent() {
        let key = r#"{ "data": { "is_free_tier": true, "limit": null, "limit_remaining": null,
            "limit_reset": null, "usage": 1.25, "usage_monthly": 0.5 } }"#;
        let r = report(key, None).unwrap();
        assert_eq!(r.plan.as_deref(), Some("Free tier"));
        assert_eq!(r.meters.len(), 1);
        assert!(
            matches!(&r.meters[0], Meter::Spend { amount, .. } if (*amount - 0.5).abs() < 1e-9)
        );
        assert!(report(r#"{"error": {"message": "No auth"}}"#, None).is_err());
    }
}

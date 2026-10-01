//! The Anthropic API: what the organisation has spent this month.
//!
//! Documented: `GET /v1/organizations/cost_report`, daily buckets of cost,
//! with an Admin API key (`sk-ant-admin…`), which only an organisation's
//! administrator can make — an individual account has no Admin API. There
//! is no endpoint for what is left of prepaid credit, so the spend is all
//! this can say. Anthropic asks for no more than one asking a minute, and
//! the figures run about five minutes behind.

use super::{json, number};
use crate::http;
use crate::report::{Meter, Report};
use crate::run::Input;
use crate::when;

const COSTS: &str = "https://api.anthropic.com/v1/organizations/cost_report";
/// The most pages read: a month of daily buckets is one.
const PAGES: usize = 4;

/// The sum of a page's costs in dollars, its currency, and the next page's
/// token when there is one.
///
/// # Errors
/// The page is not what Anthropic documents.
pub fn page(body: &str) -> Result<(f64, Option<String>, Option<String>), String> {
    let page = json(body)?;
    let buckets = page["data"]
        .as_array()
        .ok_or("the answer has no data in it")?;
    let mut cents = 0.0;
    let mut currency = None;
    for result in buckets
        .iter()
        .filter_map(|b| b["results"].as_array())
        .flatten()
    {
        // In the currency's lowest unit, as a decimal string: "123.45" is
        // $1.23.
        cents += number(&result["amount"]).unwrap_or(0.0);
        currency = currency.or_else(|| result["currency"].as_str().map(str::to_owned));
    }
    let next = page["has_more"]
        .as_bool()
        .unwrap_or(false)
        .then(|| page["next_page"].as_str().map(str::to_owned))
        .flatten();
    Ok((cents / 100.0, currency, next))
}

/// The report for a month's spend.
#[must_use]
pub fn report(amount: f64, currency: Option<String>) -> Report {
    Report {
        plan: None,
        meters: vec![Meter::Spend {
            label: "This month".into(),
            amount,
            currency: currency.unwrap_or_else(|| "USD".into()),
            budget: None,
        }],
    }
}

/// Ask Anthropic for this month's cost.
///
/// # Errors
/// No key, or Anthropic could not be asked or refused.
pub fn fetch(input: &Input) -> Result<Report, String> {
    let key = input.need("admin-key")?;
    let now = when::now();
    let base = format!(
        "{COSTS}?starting_at={}&bucket_width=1d&limit=31",
        when::rfc3339(when::month_start(now))
    );
    let (mut total, mut currency, mut next) = (0.0, None, None::<String>);
    for _ in 0..PAGES {
        let url = match &next {
            Some(token) => format!("{base}&page={token}"),
            None => base.clone(),
        };
        let body = http::get(
            &url,
            &[
                ("x-api-key", key),
                ("anthropic-version", "2023-06-01"),
                (
                    "User-Agent",
                    concat!("alpymist-ai-usage/", env!("CARGO_PKG_VERSION")),
                ),
            ],
        )?;
        let (amount, seen, more) = page(&body)?;
        total += amount;
        currency = currency.or(seen);
        next = more;
        if next.is_none() {
            break;
        }
    }
    Ok(report(total, currency))
}

#[cfg(test)]
mod tests {
    use super::{page, report};

    #[test]
    fn a_page_of_costs_is_summed_from_cents_and_names_the_next() {
        // Anthropic's documented example, and an empty day beside it.
        let body = r#"{ "data": [
            { "starting_at": "2025-08-01T00:00:00Z", "ending_at": "2025-08-02T00:00:00Z",
              "results": [ { "amount": "123.78912", "currency": "USD", "cost_type": "tokens",
                             "model": "claude-opus-5", "workspace_id": null } ] },
            { "starting_at": "2025-08-02T00:00:00Z", "ending_at": "2025-08-03T00:00:00Z",
              "results": [] } ],
          "has_more": true, "next_page": "page_MjAyNS0wNS0xNFQwMDowMDowMFo=" }"#;
        let (amount, currency, next) = page(body).unwrap();
        assert!((amount - 1.237_891_2).abs() < 1e-9, "{amount}");
        assert_eq!(currency.as_deref(), Some("USD"));
        assert_eq!(next.as_deref(), Some("page_MjAyNS0wNS0xNFQwMDowMDowMFo="));

        let last = r#"{ "data": [], "has_more": false, "next_page": null }"#;
        assert_eq!(page(last).unwrap(), (0.0, None, None));
        assert!(page(r#"{"type":"error"}"#).is_err());
        assert_eq!(report(1.5, None).meters[0].says(0), "$1.50 spent");
    }
}

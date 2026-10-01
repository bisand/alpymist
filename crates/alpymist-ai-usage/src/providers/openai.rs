//! The `OpenAI` API: what the organisation has spent this month.
//!
//! Documented: `GET /v1/organization/costs`, daily buckets of cost, with an
//! admin key made in the platform's organisation settings. There is no
//! endpoint for the credit left — the old one wants a browser's session now
//! — so the spend is all this can say.

use super::{json, number};
use crate::http;
use crate::report::{Meter, Report};
use crate::run::Input;
use crate::when;

const COSTS: &str = "https://api.openai.com/v1/organization/costs";
/// The most pages read: a month of daily buckets is one.
const PAGES: usize = 4;

/// The sum of a page's costs in dollars, its currency, and the next page's
/// cursor when there is one.
///
/// # Errors
/// The page is not what `OpenAI` documents.
pub fn page(body: &str) -> Result<(f64, Option<String>, Option<String>), String> {
    let page = json(body)?;
    let buckets = page["data"]
        .as_array()
        .ok_or("the answer has no data in it")?;
    let mut total = 0.0;
    let mut currency = None;
    for result in buckets
        .iter()
        .filter_map(|b| b["results"].as_array())
        .flatten()
    {
        // In the currency itself, as a number: 0.06 is six cents.
        total += number(&result["amount"]["value"]).unwrap_or(0.0);
        currency = currency.or_else(|| result["amount"]["currency"].as_str().map(str::to_owned));
    }
    let next = page["has_more"]
        .as_bool()
        .unwrap_or(false)
        .then(|| page["next_page"].as_str().map(str::to_owned))
        .flatten();
    Ok((total, currency, next))
}

/// Ask `OpenAI` for this month's cost.
///
/// # Errors
/// No key, or `OpenAI` could not be asked or refused.
pub fn fetch(input: &Input) -> Result<Report, String> {
    let authorization = format!("Bearer {}", input.need("admin-key")?);
    let base = format!(
        "{COSTS}?start_time={}&bucket_width=1d&limit=31",
        when::month_start(when::now())
    );
    let (mut total, mut currency, mut next) = (0.0, None, None::<String>);
    for _ in 0..PAGES {
        let url = match &next {
            Some(cursor) => format!("{base}&page={cursor}"),
            None => base.clone(),
        };
        let body = http::get(
            &url,
            &[
                ("Authorization", &authorization),
                ("Content-Type", "application/json"),
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
    Ok(Report {
        plan: None,
        meters: vec![Meter::Spend {
            label: "This month".into(),
            amount: total,
            currency: currency.unwrap_or_else(|| "usd".into()),
            budget: None,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::page;

    #[test]
    fn a_page_of_costs_is_summed_in_dollars() {
        // OpenAI's documented example, with a second day.
        let body = r#"{ "object": "page", "data": [
            { "object": "bucket", "start_time": 1730419200, "end_time": 1730505600,
              "results": [ { "object": "organization.costs.result",
                             "amount": { "value": 0.06, "currency": "usd" },
                             "line_item": null, "project_id": null } ] },
            { "object": "bucket", "start_time": 1730505600, "end_time": 1730592000,
              "results": [ { "object": "organization.costs.result",
                             "amount": { "value": 1.5, "currency": "usd" } },
                           { "object": "organization.costs.result" } ] } ],
          "has_more": false, "next_page": null }"#;
        let (amount, currency, next) = page(body).unwrap();
        assert!((amount - 1.56).abs() < 1e-9, "{amount}");
        assert_eq!(currency.as_deref(), Some("usd"));
        assert_eq!(next, None);
        assert!(page(r#"{"error": {"message": "Incorrect API key"}}"#).is_err());
    }
}

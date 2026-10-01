//! What a provider says: its plan, and the meters it can read.
//!
//! A provider program prints one of these on its standard output, as JSON,
//! and exits with success; or says why not on its standard error and exits
//! with failure. That is the whole protocol:
//!
//! ```json
//! { "plan": "Max",
//!   "meters": [
//!     { "kind": "window",  "label": "5-hour limit", "used": 0.42, "resets_at": 1790840000 },
//!     { "kind": "spend",   "label": "This month", "amount": 12.34, "currency": "USD" },
//!     { "kind": "balance", "label": "Credits", "remaining": 3.2, "total": 10.0, "currency": "USD" } ] }
//! ```

use crate::when;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// What a provider reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Report {
    /// The plan's name, where the provider says: "Max", "Pro".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// What it could read, in the order to show it.
    #[serde(default)]
    pub meters: Vec<Meter>,
}

/// One thing a provider can read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Meter {
    /// A limit over a window of time, as subscriptions have: this much of it
    /// used, from 0 to 1, and when it starts again.
    Window {
        /// What it is: "5-hour limit".
        label: String,
        /// How much is used, from 0 to 1.
        used: f64,
        /// When it resets, in seconds since the epoch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<i64>,
    },
    /// Money spent over a period, against a budget where there is one.
    Spend {
        /// What period: "This month".
        label: String,
        /// How much.
        amount: f64,
        /// In what: "USD".
        currency: String,
        /// The most that may be spent, where the provider has a limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget: Option<f64>,
    },
    /// Money left of what was paid in.
    Balance {
        /// What it is: "Credits".
        label: String,
        /// How much is left.
        remaining: f64,
        /// How much there was, where the provider says.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<f64>,
        /// In what: "USD".
        currency: String,
    },
}

impl Meter {
    /// What it is called.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Window { label, .. }
            | Self::Spend { label, .. }
            | Self::Balance { label, .. } => label,
        }
    }

    /// How close to its limit it is, from 0 to 1, where it has one: what
    /// the bar compares providers by.
    #[must_use]
    pub fn used(&self) -> Option<f64> {
        let fraction = match self {
            Self::Window { used, .. } => *used,
            Self::Spend {
                amount,
                budget: Some(budget),
                ..
            } if *budget > 0.0 => amount / budget,
            Self::Balance {
                remaining,
                total: Some(total),
                ..
            } if *total > 0.0 => 1.0 - remaining / total,
            _ => return None,
        };
        fraction.is_finite().then(|| fraction.clamp(0.0, 1.0))
    }

    /// It, in a line: "42% used, resets in 2 h 10 min".
    #[must_use]
    pub fn says(&self, now: i64) -> String {
        match self {
            Self::Window {
                used, resets_at, ..
            } => {
                let mut line = format!("{} used", percent(*used));
                if let Some(at) = resets_at.filter(|at| *at > now) {
                    let _ = write!(line, ", resets in {}", when::span(at - now));
                }
                line
            }
            Self::Spend {
                amount,
                currency,
                budget,
                ..
            } => match budget {
                Some(budget) => format!(
                    "{} of {} spent",
                    money(*amount, currency),
                    money(*budget, currency)
                ),
                None => format!("{} spent", money(*amount, currency)),
            },
            Self::Balance {
                remaining,
                total,
                currency,
                ..
            } => match total {
                Some(total) => format!(
                    "{} left of {}",
                    money(*remaining, currency),
                    money(*total, currency)
                ),
                None => format!("{} left", money(*remaining, currency)),
            },
        }
    }
}

impl Report {
    /// The meter closest to its limit, as a fraction from 0 to 1; `None`
    /// when nothing it reports has a limit.
    #[must_use]
    pub fn worst(&self) -> Option<f64> {
        self.meters
            .iter()
            .filter_map(Meter::used)
            .fold(None, |worst: Option<f64>, used| {
                Some(worst.map_or(used, |w| w.max(used)))
            })
    }

    /// Read a provider's output.
    ///
    /// # Errors
    /// It is not a report, or reports nothing.
    pub fn parse(output: &str) -> Result<Self, String> {
        let report: Self =
            serde_json::from_str(output.trim()).map_err(|e| format!("not a report: {e}"))?;
        if report.meters.is_empty() {
            return Err("the provider reported nothing".into());
        }
        Ok(report)
    }
}

/// A fraction as a percentage: "42%".
#[must_use]
pub fn percent(fraction: f64) -> String {
    let fraction = if fraction.is_finite() { fraction } else { 0.0 };
    format!("{:.0}%", (fraction * 100.0).clamp(0.0, 999.0))
}

/// An amount in a currency: "$12.34", "12.34 EUR".
#[must_use]
pub fn money(amount: f64, currency: &str) -> String {
    match currency.to_ascii_uppercase().as_str() {
        "USD" => format!("${amount:.2}"),
        other => format!("{amount:.2} {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Meter, Report};

    #[test]
    fn a_report_is_read_and_says_what_is_closest_to_its_limit() {
        let report = Report::parse(
            r#"{ "plan": "Max", "meters": [
                { "kind": "window", "label": "5-hour limit", "used": 0.42, "resets_at": 7800 },
                { "kind": "window", "label": "Weekly limit", "used": 0.9 },
                { "kind": "spend", "label": "This month", "amount": 12.341, "currency": "USD" },
                { "kind": "balance", "label": "Credits", "remaining": 2.5, "total": 10, "currency": "usd" } ] }"#,
        )
        .unwrap();
        assert_eq!(report.plan.as_deref(), Some("Max"));
        assert_eq!(report.worst(), Some(0.9));
        let said: Vec<String> = report.meters.iter().map(|m| m.says(0)).collect();
        assert_eq!(
            said,
            [
                "42% used, resets in 2 h 10 min",
                "90% used",
                "$12.34 spent",
                "$2.50 left of $10.00"
            ]
        );
        assert_eq!(report.meters[3].used(), Some(0.75));
        assert_eq!(
            report.meters[2].used(),
            None,
            "spend with no budget has no limit"
        );
    }

    #[test]
    fn nonsense_is_not_a_report_and_stays_in_range() {
        assert!(Report::parse("<html>").is_err());
        assert!(Report::parse(r#"{"meters": []}"#).is_err());
        let over = Meter::Window {
            label: "x".into(),
            used: 1.7,
            resets_at: None,
        };
        assert_eq!(over.used(), Some(1.0));
        let nan = Meter::Window {
            label: "x".into(),
            used: f64::NAN,
            resets_at: None,
        };
        assert_eq!(nan.used(), None);
    }
}

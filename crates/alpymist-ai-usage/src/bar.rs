//! What the bar shows, and what `alpymist-ai-usage status` prints.
//!
//! Waybar runs `alpymist-ai-usage --waybar` as a custom module and reads a
//! line of JSON whenever it changes: an icon with the percentage of whichever
//! provider is closest to its limit, a tooltip listing every provider turned
//! on, and a class the stylesheet colours. With nothing turned on the text
//! is empty, and Waybar hides the module.

use crate::config::Config;
use crate::definition::Definition;
use crate::report::percent;
use crate::store::Kept;
use crate::when;
use std::fmt::Write as _;

/// The bar's icon: Nerd Font's Material Design robot.
pub const ICON: &str = "\u{f06a9}";

/// From how much used the bar says critical rather than warns.
const CRITICAL: f64 = 0.95;

/// How much older than its asking interval a report may be before it is
/// called old.
const STALE_AFTER: i64 = 3;

/// A provider turned on, and what is kept for it.
pub struct Entry<'a> {
    /// The provider.
    pub definition: &'a Definition,
    /// What it last said.
    pub kept: &'a Kept,
    /// How many seconds apart it is asked.
    pub every: i64,
}

/// The provider closest to its limit, and how close, from 0 to 1.
#[must_use]
pub fn worst<'a>(entries: &'a [Entry<'a>]) -> Option<(&'a Definition, f64)> {
    entries
        .iter()
        .filter_map(|e| Some((e.definition, e.kept.report.as_ref()?.worst()?)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// A class for the bar's stylesheet.
#[must_use]
pub fn class(entries: &[Entry<'_>], config: &Config) -> &'static str {
    if entries.is_empty() {
        return "off";
    }
    match worst(entries) {
        Some((_, used)) if used >= CRITICAL => "critical",
        Some((_, used)) if used * 100.0 >= f64::from(config.warn_at) => "warning",
        Some(_) => "ok",
        // Nothing with a limit: fine if anything reported at all.
        None if entries.iter().any(|e| e.kept.report.is_some()) => "ok",
        None => "unknown",
    }
}

/// Everything turned on, a few lines each: the tooltip, and what `status`
/// prints.
#[must_use]
pub fn summary(entries: &[Entry<'_>], now: i64) -> String {
    if entries.is_empty() {
        return "No AI provider is turned on.\nTurn one on with: alpymist-ai-usage enable <provider>"
            .into();
    }
    let mut out = String::new();
    for entry in entries {
        if !out.is_empty() {
            out.push('\n');
        }
        let kept = entry.kept;
        out.push_str(&entry.definition.name);
        if let Some(plan) = kept.report.as_ref().and_then(|r| r.plan.as_deref()) {
            let _ = write!(out, " · {plan}");
        }
        for meter in kept.report.iter().flat_map(|r| &r.meters) {
            let _ = write!(out, "\n  {}: {}", meter.label(), meter.says(now));
        }
        let age = kept.fetched_at.map(|at| now - at);
        match (&kept.error, age) {
            (Some(why), Some(age)) => {
                let _ = write!(out, "\n  Last known, {} old. {why}", when::span(age));
            }
            (Some(why), None) => {
                let _ = write!(out, "\n  {why}");
            }
            (None, Some(age)) if age > entry.every * STALE_AFTER => {
                let _ = write!(out, "\n  Last known, {} old.", when::span(age));
            }
            (None, None) => out.push_str("\n  Not asked yet."),
            (None, Some(_)) => {}
        }
    }
    out
}

/// The bar's line.
#[must_use]
pub fn waybar(entries: &[Entry<'_>], config: &Config, now: i64) -> String {
    if entries.is_empty() {
        return alpymist_widget::waybar::line("", "", "off");
    }
    let text = match worst(entries) {
        Some((_, used)) => format!("{ICON} {}", percent(used)),
        None => ICON.to_owned(),
    };
    alpymist_widget::waybar::line(&text, &summary(entries, now), class(entries, config))
}

#[cfg(test)]
mod tests {
    use super::{Entry, class, summary, waybar, worst};
    use crate::config::Config;
    use crate::definition::Definition;
    use crate::report::{Meter, Report};
    use crate::store::Kept;

    fn def(id: &str, name: &str) -> Definition {
        Definition::parse(
            id,
            &format!("name = \"{name}\"\ndescription = \"\"\nexec = \"x\"\n"),
        )
        .unwrap()
    }

    fn window(used: f64) -> Report {
        Report {
            plan: Some("Max".into()),
            meters: vec![Meter::Window {
                label: "5-hour limit".into(),
                used,
                resets_at: Some(4600),
            }],
        }
    }

    #[test]
    fn the_bar_shows_whichever_is_closest_to_its_limit() {
        let (claude, router, openai) = (
            def("claude", "Claude"),
            def("openrouter", "OpenRouter"),
            def("openai-api", "OpenAI API"),
        );
        let spend = Report {
            plan: None,
            meters: vec![Meter::Spend {
                label: "This month".into(),
                amount: 3.5,
                currency: "USD".into(),
                budget: None,
            }],
        };
        let a = Kept::default().after(1000, Ok(window(0.42)));
        let b = Kept::default().after(1000, Ok(window(0.86)));
        // Asked once and reached, then not: the last answer and its age.
        let c = Kept::default()
            .after(400, Ok(spend))
            .after(1000, Err("could not reach it: no network".into()));
        let entries = [
            Entry {
                definition: &claude,
                kept: &a,
                every: 600,
            },
            Entry {
                definition: &router,
                kept: &b,
                every: 600,
            },
            Entry {
                definition: &openai,
                kept: &c,
                every: 600,
            },
        ];
        let config = Config::default();
        assert_eq!(
            worst(&entries).map(|(d, u)| (d.id.as_str(), u)),
            Some(("openrouter", 0.86))
        );
        assert_eq!(class(&entries, &config), "warning");
        let line: serde_json::Value =
            serde_json::from_str(&waybar(&entries, &config, 1000)).unwrap();
        assert!(line["text"].as_str().unwrap().ends_with(" 86%"));
        assert_eq!(
            summary(&entries, 1000),
            "Claude · Max\n  5-hour limit: 42% used, resets in 1 h\n\
             OpenRouter · Max\n  5-hour limit: 86% used, resets in 1 h\n\
             OpenAI API\n  This month: $3.50 spent\n  Last known, 10 min old. could not reach it: no network"
        );
    }

    #[test]
    fn nothing_turned_on_hides_it_and_nothing_known_says_so() {
        let config = Config::default();
        let line: serde_json::Value = serde_json::from_str(&waybar(&[], &config, 0)).unwrap();
        assert_eq!(line["text"], "", "an empty text hides the module");

        let claude = def("claude", "Claude");
        let never = Kept::default().after(10, Err("No API key yet.".into()));
        let entries = [Entry {
            definition: &claude,
            kept: &never,
            every: 600,
        }];
        assert_eq!(class(&entries, &config), "unknown");
        assert_eq!(summary(&entries, 20), "Claude\n  No API key yet.");
        let full = Kept::default().after(10, Ok(window(0.97)));
        let entries = [Entry {
            definition: &claude,
            kept: &full,
            every: 600,
        }];
        assert_eq!(class(&entries, &config), "critical");
    }
}

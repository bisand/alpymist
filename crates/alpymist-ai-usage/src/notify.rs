//! Saying so when a provider is running out.
//!
//! One notification when a provider passes the warning threshold, and one
//! more when it is nearly spent; none again until it has been back under.
//! What was last told is kept with the provider's report
//! ([`crate::store::Kept::told`]), so a bar restarted, or the three bars of
//! three screens, do not each say it again.
//!
//! Sent to the session's notification daemon — mako — over D-Bus with
//! `gdbus`, as `alpymist firmware watch` sends its own: no library, and
//! nothing that could hold the bar up for longer than its time limit.

use crate::config::Config;
use crate::definition::Definition;
use crate::report::Report;
use crate::store::Kept;
use std::process::{Command, Stdio};

/// From how much used a provider is nearly spent: the same as the bar's
/// critical colour.
pub const CRITICAL: f64 = 0.95;

/// How far along a provider is, for telling: 0 fine, 1 past the warning, 2
/// nearly spent. `None` when nothing it reports has a limit.
#[must_use]
pub fn level(report: &Report, config: &Config) -> Option<u8> {
    let used = report.worst()?;
    Some(if used >= CRITICAL {
        2
    } else {
        u8::from(used * 100.0 >= f64::from(config.warn_at))
    })
}

/// What to say about `def` now, and the level to remember having said it at:
/// `Some` only when it has gone further along than was last told. Going back
/// under is remembered without a word, by the level in the answer being
/// lower than `kept.told`.
#[must_use]
pub fn due(
    def: &Definition,
    kept: &Kept,
    config: &Config,
    now: i64,
) -> (u8, Option<(String, String)>) {
    let Some(report) = &kept.report else {
        return (kept.told, None);
    };
    let Some(level) = level(report, config) else {
        return (0, None);
    };
    if level <= kept.told || !config.notify {
        return (level, None);
    }
    let summary = if level >= 2 {
        format!("{} is nearly used up", def.name)
    } else {
        format!("{} is running low", def.name)
    };
    let body = report
        .worst_meter()
        .map(|m| format!("{}: {}", m.label(), m.says(now)))
        .unwrap_or_default();
    (level, Some((summary, body)))
}

/// A string in `GVariant`'s text format, which `gdbus call` reads its
/// arguments in.
fn gvariant_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The arguments to `gdbus` that put up a notification: `urgent` ones stay
/// until dismissed, the others for the desktop's usual while.
#[must_use]
pub fn arguments(summary: &str, body: &str, urgent: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "call",
        "--session",
        "--timeout",
        "5",
        "--dest",
        "org.freedesktop.Notifications",
        "--object-path",
        "/org/freedesktop/Notifications",
        "--method",
        "org.freedesktop.Notifications.Notify",
        // Whatever follows is the method's arguments, not options.
        "--",
        "'AI usage'",
        "0",
        "''",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(gvariant_string(summary));
    args.push(gvariant_string(body));
    args.push("[]".into());
    // The urgency hint: 2 is critical, 1 normal.
    args.push(format!(
        "{{'urgency': <byte {}>}}",
        if urgent { 2 } else { 1 }
    ));
    args.push(if urgent { "0" } else { "-1" }.into());
    args
}

/// Put a notification up. Nothing is done about one that could not be sent:
/// the bar's colour and tooltip say the same.
pub fn send(summary: &str, body: &str, urgent: bool) {
    let _ = Command::new("gdbus")
        .args(arguments(summary, body, urgent))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::{arguments, due};
    use crate::config::Config;
    use crate::definition::Definition;
    use crate::report::{Meter, Report};
    use crate::store::Kept;

    fn def() -> Definition {
        Definition::parse(
            "claude",
            "name = \"Claude\"\ndescription = \"\"\nexec = \"x\"\n",
        )
        .unwrap()
    }

    fn at(used: f64, told: u8) -> Kept {
        let mut kept = Kept::default().after(
            100,
            Ok(Report {
                plan: None,
                meters: vec![
                    Meter::Window {
                        label: "5-hour limit".into(),
                        used,
                        resets_at: Some(3700),
                    },
                    Meter::Window {
                        label: "Weekly limit".into(),
                        used: 0.2,
                        resets_at: None,
                    },
                ],
            }),
        );
        kept.told = told;
        kept
    }

    #[test]
    fn a_crossing_is_told_once_and_again_only_when_it_gets_worse() {
        let (config, def) = (Config::default(), def());
        assert_eq!(due(&def, &at(0.5, 0), &config, 100), (0, None));

        let (level, said) = due(&def, &at(0.86, 0), &config, 100);
        assert_eq!(level, 1);
        assert_eq!(
            said,
            Some((
                "Claude is running low".into(),
                "5-hour limit: 86% used, resets in 1 h".into()
            ))
        );
        // Told already: not again at the next look, nor a little further on.
        assert_eq!(due(&def, &at(0.9, 1), &config, 100), (1, None));
        // Nearly spent is worth saying once more.
        let (level, said) = due(&def, &at(0.97, 1), &config, 100);
        assert_eq!(level, 2);
        assert_eq!(said.unwrap().0, "Claude is nearly used up");
        // Back under, quietly; and the next crossing is told again.
        assert_eq!(due(&def, &at(0.1, 2), &config, 100), (0, None));
        assert!(due(&def, &at(0.86, 0), &config, 100).1.is_some());
    }

    #[test]
    fn turned_off_it_says_nothing_but_still_remembers_where_things_are() {
        let config = Config {
            notify: false,
            ..Config::default()
        };
        assert_eq!(due(&def(), &at(0.97, 0), &config, 100), (2, None));
        // A provider with nothing that has a limit has nothing to cross.
        let spend = Kept::default().after(
            1,
            Ok(Report {
                plan: None,
                meters: vec![Meter::Spend {
                    label: "This month".into(),
                    amount: 900.0,
                    currency: "USD".into(),
                    budget: None,
                }],
            }),
        );
        assert_eq!(due(&def(), &spend, &Config::default(), 100), (0, None));
    }

    #[test]
    fn notification_text_is_quoted_for_gdbus() {
        let args = arguments("It's low", "5-hour: 86% \\ used", true);
        assert!(args.contains(&"'It\\'s low'".to_owned()));
        assert!(args.contains(&"'5-hour: 86% \\\\ used'".to_owned()));
        assert_eq!(
            args.last().map(String::as_str),
            Some("0"),
            "stays until dismissed"
        );
        assert!(arguments("a", "b", false).contains(&"{'urgency': <byte 1>}".to_owned()));
    }
}

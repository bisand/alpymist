//! The last thing each provider said, kept between askings.
//!
//! `~/.cache/alpymist/ai-usage/<id>.json`. Offline, or with a provider that
//! cannot be reached, the bar shows the last known report and its age, never
//! a blank; and the time of the last try is what paces the next one, whether
//! it worked or not.

use crate::report::Report;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What is kept for a provider.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Kept {
    /// The last report it gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    /// When it gave it, in seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<i64>,
    /// When it was last asked, answer or none.
    #[serde(default)]
    pub tried_at: i64,
    /// Why the last asking failed, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Kept {
    /// What is kept after an asking at `now` that ended this way: a report
    /// replaces the last, and a failure keeps the last beside its reason.
    #[must_use]
    pub fn after(self, now: i64, outcome: Result<Report, String>) -> Self {
        match outcome {
            Ok(report) => Self {
                report: Some(report),
                fetched_at: Some(now),
                tried_at: now,
                error: None,
            },
            Err(why) => Self {
                tried_at: now,
                error: Some(why),
                ..self
            },
        }
    }

    /// Whether it is time to ask again, `every` seconds apart.
    #[must_use]
    pub fn due(&self, now: i64, every: i64) -> bool {
        now - self.tried_at >= every || self.tried_at > now
    }
}

/// Where a provider's last answer is kept.
#[must_use]
pub fn path(id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("alpymist/ai-usage").join(format!("{id}.json")))
}

/// What is kept for `id`: nothing yet, when there is no file or it does not
/// read.
#[must_use]
pub fn load(id: &str) -> Kept {
    path(id)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keep `kept` for `id`. A cache that cannot be written is not worth failing
/// over: the next asking simply comes sooner.
pub fn save(id: &str, kept: &Kept) {
    let Some(path) = path(id) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(kept) {
        let tmp = path.with_extension("json.new");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kept;
    use crate::report::{Meter, Report};

    #[test]
    fn a_failure_keeps_the_last_report_and_paces_the_next_try() {
        let report = Report {
            plan: None,
            meters: vec![Meter::Window {
                label: "5-hour limit".into(),
                used: 0.5,
                resets_at: None,
            }],
        };
        let kept = Kept::default().after(100, Ok(report.clone()));
        assert_eq!(kept.fetched_at, Some(100));
        assert!(!kept.due(400, 600) && kept.due(700, 600));

        let kept = kept.after(700, Err("offline".into()));
        assert_eq!(kept.report, Some(report), "never a blank");
        assert_eq!(kept.fetched_at, Some(100), "and its age is the report's");
        assert_eq!(kept.error.as_deref(), Some("offline"));
        assert!(!kept.due(900, 600), "a failure is not retried at once");
        // A clock that went backwards does not stop the asking for ever.
        assert!(kept.due(10, 600));
    }
}

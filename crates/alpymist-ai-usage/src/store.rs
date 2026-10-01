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
    /// How far along it was when a notification last said so: 0 for not at
    /// all, 1 for the warning, 2 for nearly spent. What keeps one crossing
    /// from being told every few minutes.
    #[serde(default)]
    pub told: u8,
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
                told: self.told,
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

/// How long a claim on a provider stands when whoever made it never let go.
const CLAIM_LASTS: std::time::Duration = std::time::Duration::from_mins(2);

/// A claim on asking provider `id`, held until dropped. The bar runs once
/// for every screen, and each of them wakes at the same moment to find the
/// same provider due; only the one that gets this asks it, and notifies.
pub struct Claim(Option<PathBuf>);

impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Claim provider `id`, or `None` when another process has it. Where there
/// is nowhere to keep a claim, everyone gets one: asking twice is better
/// than never.
#[must_use]
pub fn claim(id: &str) -> Option<Claim> {
    let Some(lock) = path(id).map(|p| p.with_extension("lock")) else {
        return Some(Claim(None));
    };
    claim_at(lock)
}

/// As [`claim`], with the lock at this path.
fn claim_at(lock: PathBuf) -> Option<Claim> {
    if let Some(dir) = lock.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => return Some(Claim(Some(lock))),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Left by a process that died holding it: take it over.
                let stale = std::fs::metadata(&lock)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|at| at.elapsed().ok())
                    .is_some_and(|age| age > CLAIM_LASTS);
                if !stale {
                    return None;
                }
                let _ = std::fs::remove_file(&lock);
            }
            Err(_) => return Some(Claim(None)),
        }
    }
    None
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
    use super::{Kept, claim_at};
    use crate::report::{Meter, Report};

    #[test]
    fn only_one_asks_at_a_time_and_a_claim_let_go_can_be_taken() {
        let lock =
            std::env::temp_dir().join(format!("alpymist-ai-claim-{}.lock", std::process::id()));
        std::fs::remove_file(&lock).ok();
        let first = claim_at(lock.clone());
        assert!(first.is_some());
        assert!(
            claim_at(lock.clone()).is_none(),
            "the bar on another screen waits"
        );
        drop(first);
        assert!(!lock.exists(), "let go when done");
        assert!(claim_at(lock.clone()).is_some());
        assert!(!lock.exists());
    }

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

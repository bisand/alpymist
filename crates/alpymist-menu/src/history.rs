//! What has been launched before, so it comes first next time.
//!
//! A line per entry, `count<TAB>key`, in `$XDG_STATE_HOME/alpymist/menu-history`.
//! Counts only, no timestamps: a menu is used a few times a day, and what was
//! used often is a better guess than what was used last. Counts are halved
//! once any reaches a ceiling, so a habit that stopped fades rather than
//! owning the top of the list forever.
//!
//! Losing the file costs an ordering and nothing else, so every failure here
//! is silent.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// When any count reaches this, every count is halved.
const CEILING: u32 = 64;

/// Launch counts by entry key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    counts: HashMap<String, u32>,
}

impl History {
    /// Where history is kept.
    #[must_use]
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))?;
        Some(base.join("alpymist/menu-history"))
    }

    /// Read history, or start empty.
    #[must_use]
    pub fn load(path: Option<&Path>) -> Self {
        let text = path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Self::parse(&text)
    }

    fn parse(text: &str) -> Self {
        let counts = text
            .lines()
            .filter_map(|line| {
                let (count, key) = line.split_once('\t')?;
                Some((key.to_owned(), count.parse().ok()?))
            })
            .collect();
        Self { counts }
    }

    /// How often `key` has been launched.
    #[must_use]
    pub fn count(&self, key: &str) -> u32 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Note a launch.
    pub fn record(&mut self, key: &str) {
        let count = self.counts.entry(key.to_owned()).or_insert(0);
        *count += 1;
        if *count >= CEILING {
            self.counts.retain(|_, c| {
                *c /= 2;
                *c > 0
            });
        }
    }

    fn render(&self) -> String {
        let mut lines: Vec<_> = self.counts.iter().collect();
        lines.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let mut out = String::new();
        for (key, count) in lines {
            let _ = writeln!(out, "{count}\t{key}");
        }
        out
    }

    /// Write history back, atomically, so two menus closing at once cannot
    /// leave half a file.
    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, self.render()).is_ok() {
            std::fs::rename(&tmp, path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CEILING, History};

    #[test]
    fn counts_round_trip() {
        let mut h = History::default();
        h.record("apps/foot.desktop");
        h.record("apps/foot.desktop");
        h.record("system/Lock");
        let back = History::parse(&h.render());
        assert_eq!(back, h);
        assert_eq!(back.count("apps/foot.desktop"), 2);
        assert_eq!(back.count("never"), 0);
    }

    #[test]
    fn garbage_lines_are_skipped() {
        let h = History::parse("3\tgood\nnot a line\nx\tbad count\n");
        assert_eq!(h.count("good"), 3);
        assert_eq!(h.counts.len(), 1);
    }

    #[test]
    fn old_habits_fade() {
        let mut h = History::default();
        h.record("once");
        for _ in 0..CEILING {
            h.record("often");
        }
        assert!(h.count("often") < CEILING);
        assert_eq!(h.count("once"), 0, "a count halved to nothing is dropped");
    }

    #[test]
    fn saving_creates_the_directory() {
        let dir = std::env::temp_dir().join(format!("alpymist-history-{}", std::process::id()));
        let path = dir.join("nested/menu-history");
        let mut h = History::default();
        h.record("k");
        h.save(&path);
        assert_eq!(History::load(Some(&path)).count("k"), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}

//! Finding entries: every source at once, at every keystroke.
//!
//! A query is words; an entry matches when every word is found in its name,
//! its id, its keywords or its summary. Where a word is found decides how
//! well it matches — the whole name beats the start of the name beats a word
//! in it beats somewhere in the summary — and applications outrank the
//! libraries and tools beside them, so "firefox" puts Firefox above
//! `firefox-esr-lang`.
//!
//! Nothing is precomputed beyond lowercase copies, and nothing needs to be:
//! thirty thousand entries are a few milliseconds of `str::find`.

use crate::catalog::{At, Catalog, Entry};

/// A query's words, lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    words: Vec<String>,
    whole: String,
}

impl Query {
    /// The words of `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let whole = text.trim().to_lowercase();
        Self {
            words: whole.split_whitespace().map(str::to_owned).collect(),
            whole,
        }
    }

    /// Whether there is nothing to search for.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether every entry this matches is among those `other` matched: the
    /// query was typed onwards from `other`, so its results can be narrowed
    /// from `other`'s instead of searched afresh.
    #[must_use]
    pub fn narrows(&self, other: &Self) -> bool {
        !other.is_empty()
            && self.words.len() >= other.words.len()
            && other
                .words
                .iter()
                .zip(&self.words)
                .all(|(old, new)| new.contains(old.as_str()))
    }

    /// How well `entry` matches, or `None` when it does not.
    #[must_use]
    pub fn score(&self, entry: &Entry) -> Option<i32> {
        if self.words.is_empty() {
            return Some(0);
        }
        // Split-off packages only come up when named in full.
        if entry.hidden && entry.name_lc != self.whole && entry.id != self.whole {
            return None;
        }
        let mut total = 0;
        for word in &self.words {
            total += word_score(word, entry)?;
        }
        // The thing itself, by name or by the end of its id: GIMP is
        // org.gimp.GIMP, and should stand beside the gimp package.
        let id_tail = entry.id.rsplit('.').next().unwrap_or(&entry.id);
        if entry.name_lc == self.whole || id_tail.eq_ignore_ascii_case(&self.whole) {
            total += 2000;
        }
        if entry.app {
            total += 150;
        }
        if entry.state.installed() {
            total += 40;
        }
        // Among equals, the shorter name is the thing itself.
        let length = i32::try_from(entry.name.len()).unwrap_or(i32::MAX);
        Some(total - length.min(60))
    }
}

/// How well one word matches an entry.
fn word_score(word: &str, e: &Entry) -> Option<i32> {
    let name = e.name_lc.as_str();
    if name == word {
        return Some(1000);
    }
    if name.starts_with(word) {
        return Some(700);
    }
    if let Some(at) = name.find(word) {
        let at_word = name[..at].ends_with(|c: char| !c.is_alphanumeric());
        return Some(if at_word { 500 } else { 300 });
    }
    // Flatpak ids end in the application's own name: org.gimp.GIMP.
    let id_tail = e.id.rsplit('.').next().unwrap_or(&e.id);
    if id_tail.eq_ignore_ascii_case(word) {
        return Some(450);
    }
    if contains_ignore_ascii_case(&e.id, word) {
        return Some(250);
    }
    if let Some(at) = e.keywords.find(word) {
        let at_word = at == 0 || e.keywords.as_bytes()[at - 1] == b' ';
        return Some(if at_word { 220 } else { 120 });
    }
    if let Some(at) = e.summary_lc.find(word) {
        let at_word = e.summary_lc[..at].ends_with(|c: char| !c.is_alphanumeric());
        return Some(if at_word { 90 } else { 40 });
    }
    None
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    n.len() <= h.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// Entries matching `query` among `candidates`, best first.
pub fn rank(catalog: &Catalog, query: &Query, candidates: impl Iterator<Item = At>) -> Vec<At> {
    let mut scored: Vec<(i32, At)> = candidates
        .filter_map(|at| {
            let entry = catalog.get(at)?;
            query.score(entry).map(|s| (s, at))
        })
        .collect();
    scored.sort_unstable_by(|(sa, a), (sb, b)| {
        sb.cmp(sa).then_with(|| {
            let name = |at: &At| catalog.get(*at).map_or("", |e| e.name_lc.as_str());
            name(a).cmp(name(b)).then(a.cmp(b))
        })
    });
    scored.into_iter().map(|(_, at)| at).collect()
}

#[cfg(test)]
mod tests {
    use super::{Query, rank};
    use crate::catalog::{Catalog, Entry};

    fn entry(id: &str, name: &str, summary: &str, app: bool) -> Entry {
        let mut e = Entry {
            id: id.into(),
            name: name.into(),
            summary: summary.into(),
            app,
            ..Entry::default()
        };
        e.index();
        e
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::new(2);
        c.replace(
            0,
            vec![
                entry(
                    "org.mozilla.firefox",
                    "Firefox",
                    "Fast, private web browser",
                    true,
                ),
                entry(
                    "org.gimp.GIMP",
                    "GNU Image Manipulation Program",
                    "Edit photos",
                    true,
                ),
            ],
        );
        let mut lang = entry("firefox-esr-lang", "firefox-esr-lang", "Languages", false);
        lang.hidden = true;
        c.replace(
            1,
            vec![
                entry("firefox-esr", "firefox-esr", "Firefox web browser", false),
                lang,
                entry("gimp", "gimp", "GNU Image Manipulation Program", false),
            ],
        );
        c
    }

    fn names(c: &Catalog, q: &str) -> Vec<String> {
        let all: Vec<_> = c.iter().map(|(at, _)| at).collect();
        rank(c, &Query::new(q), all.into_iter())
            .into_iter()
            .map(|at| c.get(at).unwrap().id.clone())
            .collect()
    }

    #[test]
    fn the_application_comes_before_the_package() {
        let c = catalog();
        assert_eq!(names(&c, "firefox"), ["org.mozilla.firefox", "firefox-esr"]);
    }

    #[test]
    fn every_word_must_match_somewhere() {
        let c = catalog();
        assert_eq!(
            names(&c, "web browser"),
            ["org.mozilla.firefox", "firefox-esr"]
        );
        assert!(names(&c, "web gimp").is_empty());
    }

    #[test]
    fn ids_find_what_names_do_not() {
        let c = catalog();
        let mut found = names(&c, "gimp");
        found.sort();
        assert_eq!(found, ["gimp", "org.gimp.GIMP"]);
    }

    #[test]
    fn hidden_packages_need_their_full_name() {
        let c = catalog();
        assert!(!names(&c, "firefox-esr").contains(&"firefox-esr-lang".to_owned()));
        assert_eq!(names(&c, "firefox-esr-lang"), ["firefox-esr-lang"]);
    }

    #[test]
    fn typing_onwards_narrows() {
        assert!(Query::new("fire").narrows(&Query::new("fir")));
        assert!(Query::new("fir box").narrows(&Query::new("fir")));
        assert!(!Query::new("fi").narrows(&Query::new("fir")));
        assert!(!Query::new("fir").narrows(&Query::new("")));
    }
}

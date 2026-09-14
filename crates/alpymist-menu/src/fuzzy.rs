//! Fuzzy matching: does a query fit a name, how well, and which letters matched.
//!
//! The query's characters must appear in the candidate in order, not
//! necessarily together — `ff` finds *Firefox*, `sd` finds *Shut down*. What
//! makes a matcher feel right is not whether it matches but how it ranks, so
//! the score rewards what people actually type: the start of the name, the
//! start of a word, and letters that run together. It is the scheme fzf made
//! familiar, and the same dynamic programme: the best placement of every query
//! letter is found, not merely the first one, so `term` scores *Terminal* on
//! the leading `T` rather than on some later `t`.
//!
//! Candidates are menu entries — tens of characters, a few hundred of them —
//! so an exhaustive O(query × name) search per candidate is well under a
//! millisecond for the whole list, and cheaper than being clever.

/// Points for each matched character.
const MATCH: i32 = 16;
/// Extra for matching the very first character of the name.
const BONUS_FIRST: i32 = 24;
/// Extra for matching the first character of a word.
const BONUS_WORD: i32 = 14;
/// Extra for a character that directly follows the previous match.
const BONUS_CONSECUTIVE: i32 = 16;
/// Cost of starting the first match further into the name.
const PENALTY_LEADING: i32 = 1;
/// Cost of skipping any characters at all between two matches.
const PENALTY_GAP_OPEN: i32 = 4;
/// Cost of each skipped character after the first.
const PENALTY_GAP_EXTEND: i32 = 1;

/// Below any score a real match can reach: an unreachable cell.
const NONE: i32 = i32::MIN / 2;

/// A successful match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Higher is better. Only comparable between matches of the same query.
    pub score: i32,
    /// Character (not byte) indices of the matched letters, ascending.
    pub positions: Vec<usize>,
}

/// Whether `ch` begins a word, given the character before it.
fn starts_word(prev: Option<char>, ch: char) -> bool {
    match prev {
        None => true,
        Some(p) => {
            (!p.is_alphanumeric() && ch.is_alphanumeric())
                || (p.is_lowercase() && ch.is_uppercase())
                || (!p.is_numeric() && ch.is_numeric())
        }
    }
}

/// Characters compared without regard to case.
fn fold(ch: char) -> char {
    // Single-character lowercase covers every alphabet a menu name is written
    // in; the few characters that lowercase to several (İ) simply match
    // themselves.
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => ch,
    }
}

/// Match `query` against `candidate`.
///
/// An empty query matches everything with a score of zero. Whitespace in the
/// query is ignored, so `shut down` and `shutdown` find the same entry.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn score(query: &str, candidate: &str) -> Option<Match> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    if query.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars: Vec<char> = candidate.chars().collect();
    let (m, n) = (query.len(), chars.len());
    if m > n {
        return None;
    }

    // Cheap rejection before any table is built: most candidates fail here.
    let mut q = query.iter().peekable();
    for &c in &chars {
        if q.peek().is_some_and(|&&want| want == fold(c)) {
            q.next();
        }
    }
    if q.peek().is_some() {
        return None;
    }

    let bonus: Vec<i32> = chars
        .iter()
        .enumerate()
        .map(|(j, &c)| {
            let prev = j.checked_sub(1).map(|p| chars[p]);
            if j == 0 {
                BONUS_FIRST
            } else if starts_word(prev, c) {
                BONUS_WORD
            } else {
                0
            }
        })
        .collect();

    // best[i][j]: the best score with query[i] matched exactly at chars[j].
    // from[i][j]: where query[i - 1] was matched on that best path.
    let mut best = vec![NONE; m * n];
    let mut from = vec![usize::MAX; m * n];

    for j in 0..n {
        if fold(chars[j]) == query[0] {
            let leading = i32::try_from(j).unwrap_or(i32::MAX / 4) * PENALTY_LEADING;
            best[j] = MATCH + bonus[j] - leading.min(MATCH);
        }
    }
    // A gap is priced affinely, fzf's way: opening one costs more than
    // widening it, so one gap beats two and a run beats an acronym.
    // Rewritten as best[k] + EXTEND*k - (EXTEND*(j-1) + OPEN - EXTEND), the
    // part depending on k can be maximised along the row as it goes.
    let at = |k: usize| i32::try_from(k).unwrap_or(i32::MAX / 4) * PENALTY_GAP_EXTEND;
    for i in 1..m {
        // For query[i] at j, the previous letter sits either right before it
        // (a run, rewarded) or at some k < j-1 (a gap of j-1-k, penalised).
        // `far` carries max(best[i-1][k] + at(k)) over k <= j-2, so the gap
        // case costs O(1) per column rather than a scan back along the row.
        let mut far = NONE;
        let mut far_at = usize::MAX;
        for j in i..n {
            if j >= 2 {
                let k = j - 2;
                let at_k = best[(i - 1) * n + k];
                if at_k > NONE && at_k + at(k) > far {
                    far = at_k + at(k);
                    far_at = k;
                }
            }
            if fold(chars[j]) != query[i] {
                continue;
            }
            let near = best[(i - 1) * n + (j - 1)];
            let adjacent = if near > NONE {
                near + BONUS_CONSECUTIVE
            } else {
                NONE
            };
            let gapped = if far > NONE {
                far - at(j - 1) - (PENALTY_GAP_OPEN - PENALTY_GAP_EXTEND)
            } else {
                NONE
            };
            let (base, came_from) = if adjacent >= gapped {
                (adjacent, j - 1)
            } else {
                (gapped, far_at)
            };
            if base > NONE {
                best[i * n + j] = base + MATCH + bonus[j];
                from[i * n + j] = came_from;
            }
        }
    }

    let last = (m - 1) * n;
    let (end, &top) = best[last..]
        .iter()
        .enumerate()
        .max_by_key(|&(j, &s)| (s, std::cmp::Reverse(j)))?;
    if top <= NONE {
        return None;
    }

    let mut positions = vec![0; m];
    let mut j = end;
    for i in (0..m).rev() {
        positions[i] = j;
        if i > 0 {
            j = from[i * n + j];
        }
    }
    Some(Match {
        score: top,
        positions,
    })
}

#[cfg(test)]
mod tests {
    use super::score;

    fn s(q: &str, c: &str) -> i32 {
        score(q, c).map_or(i32::MIN, |m| m.score)
    }

    #[test]
    fn an_empty_query_matches_everything_equally() {
        assert_eq!(score("", "Anything").unwrap().score, 0);
        assert_eq!(score("  ", "Anything").unwrap().score, 0);
    }

    #[test]
    fn letters_must_appear_in_order() {
        assert!(score("ff", "Firefox").is_some());
        assert!(score("xof", "Firefox").is_none());
        assert!(score("firefoxes", "Firefox").is_none());
    }

    #[test]
    fn matching_ignores_case() {
        assert!(score("LIBRE", "librewolf").is_some());
        assert!(score("øy", "Øya").is_some());
    }

    #[test]
    fn whitespace_in_the_query_is_ignored() {
        assert_eq!(
            score("shut down", "Shut down"),
            score("shutdown", "Shut down")
        );
    }

    #[test]
    fn a_prefix_beats_the_same_letters_later_on() {
        assert!(s("term", "Terminal") > s("term", "Determined"));
    }

    #[test]
    fn word_starts_beat_letters_inside_words() {
        // S-D on word boundaries against s…d buried mid-word.
        assert!(s("sd", "Shut down") > s("sd", "Pasted"));
    }

    #[test]
    fn letters_that_run_together_beat_scattered_ones() {
        assert!(s("wolf", "LibreWolf") > s("wolf", "Write Old Log File"));
    }

    #[test]
    fn the_best_placement_is_found_not_the_first() {
        // A greedy matcher takes the first `t` and misses the word start.
        // S-t-a-r-t, space, T(6)-e-r-m(9).
        let m = score("tm", "Start Terminal").unwrap();
        assert_eq!(m.positions, vec![6, 9], "{m:?}");
    }

    #[test]
    fn positions_are_character_indices_not_bytes() {
        let m = score("ål", "Blåbær-lyst").unwrap();
        assert_eq!(m.positions, vec![2, 7]);
    }

    #[test]
    fn camel_case_humps_count_as_word_starts() {
        assert!(s("pc", "PulseControl") > s("pc", "Apocalypse"));
    }

    #[test]
    fn positions_follow_the_score() {
        let m = score("ghost", "com.mitchellh.ghostty").unwrap();
        assert_eq!(m.positions, vec![14, 15, 16, 17, 18]);
    }

    #[test]
    fn a_shorter_name_wins_a_tie_on_the_same_letters() {
        assert!(s("fox", "Fox") >= s("fox", "Foxes and hounds"));
    }
}

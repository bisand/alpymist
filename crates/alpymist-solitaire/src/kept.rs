//! What is kept from one opening to the next: the back, how many cards the
//! stock turns, and the best games.
//!
//! One small file of lines in the account's state directory, written by the
//! program and readable by anyone: `back=Mist`, `turn=3`, and a line for
//! each of the best games. It is read as whatever a file may hold, since
//! someone may have edited it: a line that makes no sense is passed over,
//! and a name is cut to what a name may be.

/// How many of the best games are kept for each way of turning the stock.
pub const BEST: usize = 5;

/// A game that was won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    /// How long it took, in seconds.
    pub seconds: u32,
    /// In how many moves.
    pub moves: u32,
    /// Whose it was: up to three letters or digits.
    pub initials: String,
}

/// Everything kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The back's name.
    pub back: String,
    /// How many cards the stock turns: one or three.
    pub turn: usize,
    /// The best games turning one card, the best first.
    pub one: Vec<Score>,
    /// The best turning three.
    pub three: Vec<Score>,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            back: String::new(),
            turn: 1,
            one: Vec::new(),
            three: Vec::new(),
        }
    }
}

/// What someone typed, as initials: letters and digits, capitals, three at
/// most.
#[must_use]
pub fn initials(typed: &str) -> String {
    typed
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(3)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A time as a clock shows it: `3:07`, or `1:02:03` past the hour.
#[must_use]
pub fn clock(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

impl Kept {
    /// Read what a file holds.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut kept = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "back" => {
                    kept.back = value
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .take(24)
                        .collect();
                }
                "turn" => kept.turn = if value == "3" { 3 } else { 1 },
                "best" => {
                    // turn seconds moves initials
                    let mut words = value.split_whitespace();
                    let turn = words.next();
                    let (Some(seconds), Some(moves)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) else {
                        continue;
                    };
                    let score = Score {
                        seconds,
                        moves,
                        initials: initials(words.next().unwrap_or("")),
                    };
                    match turn {
                        Some("1") => kept.one.push(score),
                        Some("3") => kept.three.push(score),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        for table in [&mut kept.one, &mut kept.three] {
            table.sort_by_key(|s| (s.seconds, s.moves));
            table.truncate(BEST);
        }
        kept
    }

    /// The file's text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = format!("back={}\nturn={}\n", self.back, self.turn);
        for (turn, table) in [(1, &self.one), (3, &self.three)] {
            for s in table {
                let line = format!("best={turn} {} {} {}\n", s.seconds, s.moves, s.initials);
                text.push_str(&line);
            }
        }
        text
    }

    /// The best games for a way of turning the stock.
    #[must_use]
    pub fn best(&self, turn: usize) -> &[Score] {
        if turn == 3 { &self.three } else { &self.one }
    }

    /// Where a game of `seconds` and `moves` would stand among the best, the
    /// first place being 0, if it would stand among them at all. The shorter
    /// time is the better game, and of two as long, the fewer moves.
    #[must_use]
    pub fn place(&self, turn: usize, seconds: u32, moves: u32) -> Option<usize> {
        let place = self
            .best(turn)
            .iter()
            .take_while(|s| (s.seconds, s.moves) <= (seconds, moves))
            .count();
        (place < BEST).then_some(place)
    }

    /// Put a game among the best. Returns its place, or `None` for one that
    /// does not make it.
    pub fn record(&mut self, turn: usize, score: Score) -> Option<usize> {
        let place = self.place(turn, score.seconds, score.moves)?;
        let table = if turn == 3 {
            &mut self.three
        } else {
            &mut self.one
        };
        table.insert(place, score);
        table.truncate(BEST);
        Some(place)
    }
}

#[cfg(test)]
mod tests {
    use super::{BEST, Kept, Score, clock, initials};

    fn score(seconds: u32, moves: u32, who: &str) -> Score {
        Score {
            seconds,
            moves,
            initials: who.into(),
        }
    }

    #[test]
    fn a_clock_reads_as_one() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(187), "3:07");
        assert_eq!(clock(3723), "1:02:03");
    }

    #[test]
    fn initials_are_three_capitals_at_most() {
        assert_eq!(initials("ab"), "AB");
        assert_eq!(initials("andré b"), "AND");
        assert_eq!(initials("a=1\nb"), "A1B");
        assert_eq!(initials(""), "");
    }

    #[test]
    fn what_is_written_is_what_is_read() {
        let mut kept = Kept {
            back: "16-bit".into(),
            turn: 3,
            ..Kept::default()
        };
        kept.record(1, score(200, 90, "AB"));
        kept.record(3, score(400, 120, ""));
        assert_eq!(Kept::parse(&kept.text()), kept);
        assert_eq!(
            kept.text(),
            "back=16-bit\nturn=3\nbest=1 200 90 AB\nbest=3 400 120 \n"
        );
    }

    #[test]
    fn a_file_of_anything_reads_as_something() {
        let kept = Kept::parse(
            "turn=seven\nback=../../etc/passwd\nbest=1 x y\nbest=9 1 1 AB\n\
             best=1 30 10 <script>\nnonsense\n=\nbest=1 20 5\n",
        );
        assert_eq!(kept.turn, 1);
        assert_eq!(kept.back, "etcpasswd");
        assert_eq!(kept.one, [score(20, 5, ""), score(30, 10, "SCR")]);
        assert!(kept.three.is_empty());
        assert_eq!(Kept::parse(""), Kept::default());
    }

    #[test]
    fn the_best_five_are_kept_shortest_first() {
        let mut kept = Kept::default();
        assert_eq!(
            kept.place(1, 999, 999),
            Some(0),
            "the first game is a record"
        );
        for (i, seconds) in [300, 100, 500, 200, 400].into_iter().enumerate() {
            assert!(
                kept.record(1, score(seconds, 80, &format!("P{i}")))
                    .is_some()
            );
        }
        let times: Vec<u32> = kept.best(1).iter().map(|s| s.seconds).collect();
        assert_eq!(times, [100, 200, 300, 400, 500]);
        assert_eq!(kept.place(1, 600, 1), None, "slower than all five");
        assert_eq!(kept.place(1, 500, 80), None, "no better than the last");
        // As long as the second and in fewer moves: before it.
        assert_eq!(kept.record(1, score(200, 70, "NEW")), Some(1));
        assert_eq!(kept.best(1).len(), BEST);
        assert_eq!(kept.best(1)[1].initials, "NEW");
        assert_eq!(kept.best(1)[4].seconds, 400);
        // The other way of turning has its own.
        assert!(kept.best(3).is_empty());
        assert_eq!(kept.place(3, 9999, 9999), Some(0));
    }
}

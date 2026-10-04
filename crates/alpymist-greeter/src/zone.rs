//! How far the machine's clock is from UTC, now.
//!
//! `/etc/localtime` is a `TZif` file (RFC 9636): every change of offset the
//! zone has had, each with the second it happened at, and after them a line
//! in POSIX's `TZ` notation that says how the changes go on from the last one
//! recorded. Both are read here. A file may record its changes far ahead or
//! stop decades ago and leave the rest to that line; tzdata is built either
//! way, and which way is the builder's choice, not ours.
//!
//! Only the offset is read. Not the abbreviations, not the leap seconds, and
//! nothing about a time before the zone's first change but the first offset
//! in the file: the question here is what the clock on the wall says now.

use std::path::Path;

/// A time zone: the offset from UTC at any second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// The offset before the first change, in seconds east of UTC.
    first: i32,
    /// Each change: the second it happens at, and the offset from then on.
    changes: Vec<(i64, i32)>,
    /// How it goes on after the last of them.
    rule: Option<Rule>,
}

impl Zone {
    /// UTC, which is what a machine with no zone set keeps.
    #[must_use]
    pub const fn utc() -> Self {
        Self {
            first: 0,
            changes: Vec::new(),
            rule: None,
        }
    }

    /// The machine's zone: what `TZ` names, where it is set, and
    /// `/etc/localtime` otherwise. UTC when neither can be read.
    #[must_use]
    pub fn system() -> Self {
        let named = std::env::var("TZ").ok().filter(|tz| !tz.is_empty());
        match named {
            Some(tz) => Self::named(tz.strip_prefix(':').unwrap_or(&tz)),
            None => Self::read(Path::new("/etc/localtime")),
        }
        .unwrap_or_else(Self::utc)
    }

    /// What `TZ` says: a file, a zone by tzdata's name, or a rule written
    /// out.
    fn named(tz: &str) -> Option<Self> {
        if tz.starts_with('/') {
            return Self::read(Path::new(tz));
        }
        // A name is a path under tzdata and nowhere else.
        let inside = !tz
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.'));
        inside
            .then(|| Self::read(&Path::new("/usr/share/zoneinfo").join(tz)))
            .flatten()
            .or_else(|| {
                Rule::parse(tz).map(|rule| Self {
                    first: rule.std,
                    changes: Vec::new(),
                    rule: Some(rule),
                })
            })
    }

    fn read(path: &Path) -> Option<Self> {
        Self::parse(&std::fs::read(path).ok()?)
    }

    /// Read a `TZif` file. `None` when it is not one, or is cut short.
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<Self> {
        let first = Block::at(data, 0, 4)?;
        if first.version < b'2' {
            return Some(first.zone(data, None));
        }
        // The same again with times that do not run out in 2038, and it is
        // that one a reader is to use where it is there.
        let second = Block::at(data, first.end, 8)?;
        let footer = data.get(second.end..)?.strip_prefix(b"\n")?;
        let line = footer.split(|b| *b == b'\n').next()?;
        let rule = std::str::from_utf8(line).ok().and_then(Rule::parse);
        Some(second.zone(data, rule))
    }

    /// Seconds east of UTC at `unix`, a count of seconds since 1970.
    #[must_use]
    pub fn offset_at(&self, unix: i64) -> i32 {
        let past_the_last = self.changes.last().is_none_or(|(at, _)| unix >= *at);
        if let (true, Some(rule)) = (past_the_last, &self.rule) {
            return rule.offset_at(unix);
        }
        let before = self.changes.partition_point(|(at, _)| *at <= unix);
        match before.checked_sub(1) {
            Some(i) => self.changes[i].1,
            None => self.first,
        }
    }
}

/// One of a `TZif` file's two blocks of data: where its parts are.
struct Block {
    version: u8,
    /// How many bytes a time takes: four in the first block, eight after.
    width: usize,
    times: usize,
    kinds: usize,
    /// Where the transition times start, and where the block ends.
    start: usize,
    end: usize,
}

impl Block {
    /// The block whose header is at `at`.
    fn at(data: &[u8], at: usize, width: usize) -> Option<Self> {
        let header = data.get(at..at.checked_add(44)?)?;
        if !header.starts_with(b"TZif") {
            return None;
        }
        let count = |i: usize| {
            let bytes = header.get(20 + i * 4..24 + i * 4)?;
            usize::try_from(u32::from_be_bytes(bytes.try_into().ok()?)).ok()
        };
        let (utc, std, leaps, times, kinds, chars) = (
            count(0)?,
            count(1)?,
            count(2)?,
            count(3)?,
            count(4)?,
            count(5)?,
        );
        let start = at + 44;
        let size = times
            .checked_mul(width + 1)?
            .checked_add(kinds.checked_mul(6)?)?
            .checked_add(chars)?
            .checked_add(leaps.checked_mul(width + 4)?)?
            .checked_add(std)?
            .checked_add(utc)?;
        let end = start.checked_add(size)?;
        (end <= data.len() && kinds > 0).then_some(Self {
            version: header[4],
            width,
            times,
            kinds,
            start,
            end,
        })
    }

    /// The zone this block describes, going on as `rule` says.
    fn zone(&self, data: &[u8], rule: Option<Rule>) -> Zone {
        let indices = self.start + self.times * self.width;
        let kinds = indices + self.times;
        // `at` checked that all of this is inside `data`.
        let offset = |kind: usize| {
            let at = kinds + kind.min(self.kinds - 1) * 6;
            i32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
        };
        let changes = (0..self.times)
            .map(|i| {
                let at = self.start + i * self.width;
                let when = if self.width == 8 {
                    let mut bytes = [0; 8];
                    bytes.copy_from_slice(&data[at..at + 8]);
                    i64::from_be_bytes(bytes)
                } else {
                    i64::from(i32::from_be_bytes([
                        data[at],
                        data[at + 1],
                        data[at + 2],
                        data[at + 3],
                    ]))
                };
                (when, offset(usize::from(data[indices + i])))
            })
            .collect();
        Zone {
            first: offset(0),
            changes,
            rule,
        }
    }
}

/// A zone in POSIX's `TZ` notation, as `CET-1CEST,M3.5.0,M10.5.0/3`: a
/// standard offset, and where there is summer time, its offset and the two
/// days of the year it starts and ends on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    /// Seconds east of UTC, which is the notation's own sign turned round.
    std: i32,
    dst: Option<Summer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Summer {
    offset: i32,
    start: Change,
    end: Change,
}

/// A day of the year and a time on it, by the clock in force until then.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    day: Day,
    /// Seconds after midnight; may be negative, or more than a day.
    time: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Day {
    /// `Jn`: day 1 to 365, 29 February never counted.
    Julian(i64),
    /// `n`: day 0 to 365, 29 February counted.
    Counted(i64),
    /// `Mm.w.d`: weekday `d`, Sunday being 0, of week `w` of month `m`, week
    /// 5 being the last there is.
    Weekday { month: i64, week: i64, weekday: i64 },
}

impl Rule {
    fn parse(text: &str) -> Option<Self> {
        let mut rest = text;
        name(&mut rest)?;
        let std = -clock(&mut rest)?;
        if rest.is_empty() {
            return Some(Self { std, dst: None });
        }
        name(&mut rest)?;
        let offset = if rest.starts_with(',') || rest.is_empty() {
            std + 3600
        } else {
            -clock(&mut rest)?
        };
        // Summer time with no days given has no meaning POSIX agrees on.
        let (start, end) = rest.strip_prefix(',')?.split_once(',')?;
        Some(Self {
            std,
            dst: Some(Summer {
                offset,
                start: Change::parse(start)?,
                end: Change::parse(end)?,
            }),
        })
    }

    fn offset_at(&self, unix: i64) -> i32 {
        let Some(summer) = &self.dst else {
            return self.std;
        };
        // The year by the standard clock: the two changes of that year are
        // then enough, whichever half of the world this is.
        let (year, _, _) = civil(unix.saturating_add(self.std.into()).div_euclid(86400));
        let start = summer.start.at(year) - i64::from(self.std);
        let end = summer.end.at(year) - i64::from(summer.offset);
        let in_summer = if start <= end {
            (start..end).contains(&unix)
        } else {
            !(end..start).contains(&unix)
        };
        if in_summer { summer.offset } else { self.std }
    }
}

impl Change {
    fn parse(text: &str) -> Option<Self> {
        let (day, time) = match text.split_once('/') {
            Some((day, mut time)) => {
                let seconds = clock(&mut time)?;
                time.is_empty().then_some((day, seconds))?
            }
            None => (text, 7200),
        };
        let day = if let Some(month) = day.strip_prefix('M') {
            let mut parts = month.split('.').map(|part| part.parse::<i64>().ok());
            let (month, week, weekday) = (parts.next()??, parts.next()??, parts.next()??);
            if parts.next().is_some()
                || !(1..=12).contains(&month)
                || !(1..=5).contains(&week)
                || !(0..=6).contains(&weekday)
            {
                return None;
            }
            Day::Weekday {
                month,
                week,
                weekday,
            }
        } else if let Some(n) = day.strip_prefix('J') {
            Day::Julian(n.parse().ok().filter(|n| (1..=365).contains(n))?)
        } else {
            Day::Counted(day.parse().ok().filter(|n| (0..=365).contains(n))?)
        };
        Some(Self { day, time })
    }

    /// The second this falls on in `year`, as the clock on the wall counts:
    /// taking the offset in force away gives the second it is.
    fn at(&self, year: i64) -> i64 {
        let day = match self.day {
            Day::Julian(n) => days(year, 1, 1) + n - 1 + i64::from(leap(year) && n >= 60),
            Day::Counted(n) => days(year, 1, 1) + n,
            Day::Weekday {
                month,
                week,
                weekday,
            } => {
                let first = days(year, month, 1);
                let next = if month == 12 {
                    days(year + 1, 1, 1)
                } else {
                    days(year, month + 1, 1)
                };
                // 1 January 1970 was a Thursday.
                let on = first + (weekday - (first + 4)).rem_euclid(7) + 7 * (week - 1);
                if on >= next { on - 7 } else { on }
            }
        };
        day * 86400 + i64::from(self.time)
    }
}

/// Take a zone's name off the front of `rest`: letters, or anything at all
/// between angle brackets.
fn name(rest: &mut &str) -> Option<()> {
    let end = if let Some(quoted) = rest.strip_prefix('<') {
        quoted.find('>')? + 2
    } else {
        let end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        (end >= 3).then_some(end)?
    };
    *rest = rest.get(end..)?;
    Some(())
}

/// Take `[+-]h[:mm[:ss]]` off the front of `rest`, in seconds.
fn clock(rest: &mut &str) -> Option<i32> {
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | ':')))
        .unwrap_or(rest.len());
    let (text, after) = rest.split_at(end);
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let mut seconds = 0i32;
    let mut parts = 0;
    for (part, unit) in digits.split(':').zip([3600, 60, 1]) {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: i32 = part.parse().ok().filter(|n| *n <= 167)?;
        seconds += n * unit;
        parts += 1;
    }
    (parts == digits.split(':').count()).then_some(())?;
    *rest = after;
    Some(sign * seconds)
}

const fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days from 1 January 1970 to a date. The arithmetic, and that of [`civil`],
/// is Howard Hinnant's `days_from_civil`: years counted from March, so that
/// the day a leap year adds is the last of its year.
const fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    era * 146_097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year - 719_468
}

/// The year, month and day that is `days` after 1 January 1970.
#[must_use]
pub const fn civil(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year + year / 4 - year / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    (
        year + era * 400 + if month <= 2 { 1 } else { 0 },
        month,
        day,
    )
}

#[cfg(test)]
mod tests {
    use super::{Rule, Zone, civil, days};

    /// A zone file of Alpine's own, kept beside the tests.
    pub(crate) fn zone(name: &str) -> Zone {
        let path = format!("{}/tests/zones/{name}", env!("CARGO_MANIFEST_DIR"));
        Zone::parse(&std::fs::read(path).unwrap()).unwrap()
    }

    /// Every change of offset in ten zones over seventy years, as another
    /// reader of the same files has them: the second before each is still
    /// the old offset, and the second of it is the new one.
    fn against_the_table(read: impl Fn(&str) -> Zone) -> usize {
        let table = include_str!("../tests/zones/transitions.tsv");
        let mut checked = 0;
        let mut last: Option<(&str, Zone, i32)> = None;
        for line in table.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split('\t');
            let name = fields.next().unwrap();
            let at: i64 = fields.next().unwrap().parse().unwrap();
            let offset: i32 = fields.next().unwrap().parse().unwrap();
            let (zone, before) = match last.take() {
                Some((was, zone, before)) if was == name => (zone, Some(before)),
                _ => (read(name), None),
            };
            if let Some(before) = before {
                assert_eq!(zone.offset_at(at - 1), before, "{name} just before {at}");
            }
            assert_eq!(zone.offset_at(at), offset, "{name} at {at}");
            // And well inside the stretch that follows.
            assert_eq!(zone.offset_at(at + 86400 * 3), offset, "{name} after {at}");
            checked += 1;
            last = Some((name, zone, offset));
        }
        checked
    }

    #[test]
    fn every_change_is_where_another_reader_of_the_file_has_it() {
        assert!(against_the_table(zone) > 1000);
    }

    /// The same, from the line at the end of each file alone: what a file
    /// built to record no change ahead of time leaves a reader with. Only
    /// from the year the zone's present rule has held since.
    #[test]
    fn the_line_at_the_end_says_the_same_as_the_changes_recorded() {
        for (name, since) in [
            ("Europe_Oslo", 1997),
            ("America_New_York", 2008),
            ("Australia_Lord_Howe", 2009),
            ("Pacific_Chatham", 2008),
            ("Europe_Dublin", 1997),
            ("America_Nuuk", 2025),
            ("Asia_Kolkata", 1990),
        ] {
            let whole = zone(name);
            let rule = whole.rule.clone().unwrap_or_else(|| panic!("{name}"));
            let alone = Zone {
                first: rule.std,
                changes: Vec::new(),
                rule: Some(rule),
            };
            let mut at = days(since, 1, 1) * 86400;
            let mut checked = 0;
            // Every hour and a bit, to 2037, where the recorded changes end.
            while at < days(2037, 1, 1) * 86400 {
                assert_eq!(alone.offset_at(at), whole.offset_at(at), "{name} at {at}");
                at += 3907;
                checked += 1;
            }
            assert!(checked > 80_000);
        }
    }

    #[test]
    fn a_rule_is_read_as_posix_writes_it() {
        let oslo = Rule::parse("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        assert_eq!(oslo.std, 3600);
        assert_eq!(oslo.dst.as_ref().unwrap().offset, 7200);
        // 2026-07-01 and 2026-12-01, at noon UTC.
        assert_eq!(oslo.offset_at(days(2026, 7, 1) * 86400 + 43200), 7200);
        assert_eq!(oslo.offset_at(days(2026, 12, 1) * 86400 + 43200), 3600);

        let india = Rule::parse("IST-5:30").unwrap();
        assert_eq!((india.std, india.dst), (19800, None));
        let nuuk = Rule::parse("<-02>2<-01>,M3.5.0/-1,M10.5.0/0").unwrap();
        assert_eq!(nuuk.std, -7200);
        assert_eq!(nuuk.dst.as_ref().unwrap().start.time, -3600);
        let gaza = Rule::parse("EET-2EEST,M3.4.4/50,M10.4.4/50").unwrap();
        assert_eq!(gaza.dst.as_ref().unwrap().end.time, 50 * 3600);
        // Days by number: the whole year in summer time.
        let always = Rule::parse("XXX3EDT4,0/0,J365/23").unwrap();
        assert_eq!(
            always.offset_at(days(2026, 1, 1) * 86400 + 5 * 3600),
            -14400
        );
        assert_eq!(always.offset_at(days(2026, 6, 15) * 86400), -14400);

        for wrong in [
            "",
            "CE-1",
            "CET",
            "CET-1CEST",
            "CET-1CEST,M3.5.0",
            "CET-1CEST,M13.5.0,M10.5.0",
            "CET-1CEST,M3.6.0,M10.5.0",
            "CET-1CEST,M3.5.7,M10.5.0",
            "CET-1CEST,J0,J365",
            "CET-1CEST,366,1",
            "CET-1:CEST,M3.5.0,M10.5.0",
            "CET-999",
            "<+01-1",
        ] {
            assert_eq!(Rule::parse(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn what_is_not_a_zone_file_is_not_read_as_one() {
        let oslo = std::fs::read(format!(
            "{}/tests/zones/Europe_Oslo",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert!(Zone::parse(&oslo).is_some());
        assert_eq!(Zone::parse(b""), None);
        assert_eq!(Zone::parse(b"TZif"), None);
        assert_eq!(
            Zone::parse(b"not a zone file, though long enough to hold a header"),
            None
        );
        // Cut short anywhere, it is not read as far as it goes.
        for cut in [43, 44, 100, 900, oslo.len() - 40] {
            assert_eq!(Zone::parse(&oslo[..cut]), None, "cut at {cut}");
        }
        // Counts that promise more than any file holds do not overflow.
        let mut huge = oslo.clone();
        huge[20..44].fill(0xff);
        assert_eq!(Zone::parse(&huge), None);
        assert_eq!(Zone::utc().offset_at(1_789_362_300), 0);
    }

    #[test]
    fn a_date_and_its_count_of_days_are_each_others() {
        assert_eq!(days(1970, 1, 1), 0);
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(days(2000, 2, 29), 11016);
        assert_eq!(civil(11016), (2000, 2, 29));
        assert_eq!(civil(11017), (2000, 3, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        for day in (-200_000..200_000).step_by(37) {
            let (y, m, d) = civil(day);
            assert_eq!(days(y, m, d), day);
        }
    }
}

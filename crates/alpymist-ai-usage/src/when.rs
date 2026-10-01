//! Time, as much of it as this needs: now, the start of the month, and how
//! long until something. No time zone but UTC, which is what the providers
//! bill in.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the epoch.
#[must_use]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

const DAY: i64 = 86_400;

/// The year, month and day of `epoch`, in UTC. Howard Hinnant's
/// `civil_from_days`, which is exact over the whole proleptic calendar.
#[must_use]
pub fn date(epoch: i64) -> (i64, i64, i64) {
    let z = epoch.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The epoch of midnight UTC on this date: `days_from_civil`.
#[must_use]
pub fn epoch(year: i64, month: i64, day: i64) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * DAY
}

/// Midnight UTC on the first of the month `at` is in.
#[must_use]
pub fn month_start(at: i64) -> i64 {
    let (year, month, _) = date(at);
    epoch(year, month, 1)
}

/// `at` as RFC 3339, in UTC: `2026-10-01T00:00:00Z`.
#[must_use]
pub fn rfc3339(at: i64) -> String {
    let (year, month, day) = date(at);
    let secs = at.rem_euclid(DAY);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// The epoch of an RFC 3339 time, as providers write them: a date, `T`, a
/// time, fractions of a second or none, and `Z` or an offset.
#[must_use]
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    // The offset starts at the first sign or Z after the time.
    let cut = rest.find(['Z', 'z', '+', '-']).unwrap_or(rest.len());
    let (time, zone) = rest.split_at(cut);
    let mut hms = time.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms
        .next()
        .and_then(|s| s.split('.').next())
        .map_or(Some(0), |s| s.parse().ok())?;
    let offset = match zone.chars().next() {
        None | Some('Z' | 'z') => 0,
        Some(sign) => {
            let mut hm = zone[1..].split(':');
            let h: i64 = hm.next()?.parse().ok()?;
            let m: i64 = hm.next().map_or(Some(0), |m| m.parse().ok())?;
            (h * 3600 + m * 60) * if sign == '-' { -1 } else { 1 }
        }
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(epoch(year, month, day) + hour * 3600 + minute * 60 + second - offset)
}

/// A length of time as a person says it: "now", "5 min", "2 h 10 min",
/// "3 days".
#[must_use]
pub fn span(seconds: i64) -> String {
    let s = seconds.max(0);
    match s {
        0..=59 => "under a minute".into(),
        60..=3599 => format!("{} min", s / 60),
        3600..=172_799 => match (s / 3600, s % 3600 / 60) {
            (h, 0) => format!("{h} h"),
            (h, m) => format!("{h} h {m} min"),
        },
        _ => format!("{} days", s / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::{date, epoch, month_start, parse_rfc3339, rfc3339, span};

    #[test]
    fn dates_go_there_and_back() {
        assert_eq!(date(0), (1970, 1, 1));
        assert_eq!(epoch(1970, 1, 1), 0);
        // 2026-10-01T06:30:15Z
        let at = 1_790_836_215;
        assert_eq!(date(at), (2026, 10, 1));
        assert_eq!(rfc3339(at), "2026-10-01T06:30:15Z");
        assert_eq!(parse_rfc3339("2026-10-01T06:30:15Z"), Some(at));
        assert_eq!(parse_rfc3339("2026-10-01T06:30:15.123456+00:00"), Some(at));
        assert_eq!(parse_rfc3339("2026-10-01T08:30:15+02:00"), Some(at));
        assert_eq!(parse_rfc3339("yesterday"), None);
        // A leap day, and the month it is in.
        assert_eq!(date(epoch(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(rfc3339(month_start(at)), "2026-10-01T00:00:00Z");
        assert_eq!(rfc3339(month_start(at - 7 * 3600)), "2026-09-01T00:00:00Z");
    }

    #[test]
    fn a_length_of_time_reads_as_one() {
        assert_eq!(span(-5), "under a minute");
        assert_eq!(span(300), "5 min");
        assert_eq!(span(7800), "2 h 10 min");
        assert_eq!(span(7200), "2 h");
        assert_eq!(span(3 * 86_400 + 5), "3 days");
    }
}

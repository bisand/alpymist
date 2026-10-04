//! What the clock in the sky says.
//!
//! In the machine's own time zone, which the installer set: `/etc/localtime`,
//! read directly by [`crate::zone`], because musl's `localtime` is not
//! something Rust's standard library exposes and a clock an hour out is worse
//! than none.
//!
//! In 24 hours unless the system says otherwise: `datetime.24-hour` in
//! `/etc/alpymist/settings.toml`, which Settings writes. Both are read once,
//! since the login screen starts again for every login and the lock screen
//! for every lock.

use crate::zone::{Zone, civil};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// The system's settings, where `datetime.24-hour` is kept.
const SETTINGS: &str = "/etc/alpymist/settings.toml";

const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The time as `HH:MM` or `H:MM PM`, and the date written out, for now.
#[must_use]
pub fn now() -> (String, String) {
    static TWENTY_FOUR: OnceLock<bool> = OnceLock::new();
    static ZONE: OnceLock<Zone> = OnceLock::new();
    let hours = *TWENTY_FOUR
        .get_or_init(|| twenty_four_hour(&std::fs::read_to_string(SETTINGS).unwrap_or_default()));
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or_default();
    format(ZONE.get_or_init(Zone::system), unix, hours)
}

/// Whether `settings`, the text of the system's settings file, asks for a
/// 24-hour clock. It does unless it plainly says not.
#[must_use]
pub fn twenty_four_hour(settings: &str) -> bool {
    settings
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("datetime.24-hour")?.as_bool())
        .unwrap_or(true)
}

/// The time and date in `zone` at `unix`, a count of seconds since 1970, on
/// a 24-hour clock or a 12-hour one.
#[must_use]
pub fn format(zone: &Zone, unix: i64, twenty_four: bool) -> (String, String) {
    let local = unix.saturating_add(zone.offset_at(unix).into());
    let day = local.div_euclid(86400);
    let second = local.rem_euclid(86400);
    let (hour, minute) = (second / 3600, second % 3600 / 60);
    let time = if twenty_four {
        format!("{hour:02}:{minute:02}")
    } else {
        // Midnight and noon are both twelve.
        let twelve = (hour + 11) % 12 + 1;
        let half = if hour < 12 { "AM" } else { "PM" };
        format!("{twelve}:{minute:02} {half}")
    };
    let (_, month, of_month) = civil(day);
    // 1 January 1970, day 0, was a Thursday; `rem_euclid` and a month from
    // 1 to 12 keep both of these inside their tables.
    let weekday = WEEKDAYS[usize::try_from(day.rem_euclid(7)).unwrap_or_default()];
    let month = MONTHS[usize::try_from(month - 1).unwrap_or_default() % 12];
    (time, format!("{weekday} {of_month} {month}"))
}

#[cfg(test)]
mod tests {
    use super::{format, twenty_four_hour};
    use crate::zone::Zone;

    fn oslo() -> Zone {
        let path = format!("{}/tests/zones/Europe_Oslo", env!("CARGO_MANIFEST_DIR"));
        Zone::parse(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_clock_is_twenty_four_hour_and_the_date_is_written_out() {
        // 2026-09-14T07:05:00 in Oslo, which is two hours ahead in September.
        let (clock, date) = format(&oslo(), 1_789_362_300, true);
        assert_eq!(clock, "07:05");
        assert_eq!(date, "Monday 14 September");
    }

    #[test]
    fn a_twelve_hour_clock_says_am_or_pm() {
        // 14:30 and 00:05 on the same day.
        assert_eq!(format(&oslo(), 1_789_389_000, false).0, "2:30 PM");
        assert_eq!(format(&oslo(), 1_789_337_100, false).0, "12:05 AM");
        // Noon, in January, when Oslo is one hour ahead.
        assert_eq!(format(&oslo(), 1_768_474_800, false).0, "12:00 PM");
    }

    #[test]
    fn the_date_is_the_zones_and_not_utcs() {
        // A minute to midnight on 29 February 2024 in Oslo, and 22:59 in UTC.
        let (clock, date) = format(&oslo(), 1_709_247_540, true);
        assert_eq!(
            (clock.as_str(), date.as_str()),
            ("23:59", "Thursday 29 February")
        );
        // A minute on, it is March there and still February in UTC.
        let (clock, date) = format(&oslo(), 1_709_247_600, true);
        assert_eq!((clock.as_str(), date.as_str()), ("00:00", "Friday 1 March"));
        let (clock, date) = format(&Zone::utc(), 1_709_247_600, true);
        assert_eq!(
            (clock.as_str(), date.as_str()),
            ("23:00", "Thursday 29 February")
        );
    }

    #[test]
    fn only_a_plain_no_makes_it_twelve_hour() {
        assert!(twenty_four_hour(""));
        assert!(twenty_four_hour("not toml at all ["));
        assert!(twenty_four_hour("\"datetime.24-hour\" = true\n"));
        assert!(!twenty_four_hour(
            "\"keyboard.layout\" = \"no\"\n\"datetime.24-hour\" = false\n"
        ));
    }
}

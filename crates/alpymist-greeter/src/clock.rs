//! What the clock in the sky says.
//!
//! In the machine's own time zone, which the installer set: `/etc/localtime`,
//! read directly, because musl's `localtime` is not something Rust's standard
//! library exposes and a clock an hour out is worse than none.
//!
//! In 24 hours unless the system says otherwise: `datetime.24-hour` in
//! `/etc/alpymist/settings.toml`, which Settings writes. Read once, since the
//! login screen starts again for every login and the lock screen for every
//! lock.

use jiff::Zoned;
use std::sync::OnceLock;

/// The system's settings, where `datetime.24-hour` is kept.
const SETTINGS: &str = "/etc/alpymist/settings.toml";

/// The time as `HH:MM` or `H:MM PM`, and the date written out, for now.
#[must_use]
pub fn now() -> (String, String) {
    static TWENTY_FOUR: OnceLock<bool> = OnceLock::new();
    let hours = *TWENTY_FOUR
        .get_or_init(|| twenty_four_hour(&std::fs::read_to_string(SETTINGS).unwrap_or_default()));
    format(&Zoned::now(), hours)
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

/// The time and date for `at`, on a 24-hour clock or a 12-hour one.
#[must_use]
pub fn format(at: &Zoned, twenty_four: bool) -> (String, String) {
    let time = if twenty_four { "%H:%M" } else { "%-I:%M %p" };
    (
        at.strftime(time).to_string(),
        at.strftime("%A %-d %B").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::{format, twenty_four_hour};

    #[test]
    fn the_clock_is_twenty_four_hour_and_the_date_is_written_out() {
        let at: jiff::Zoned = "2026-09-14T07:05:00[Europe/Oslo]".parse().unwrap();
        let (clock, date) = format(&at, true);
        assert_eq!(clock, "07:05");
        assert_eq!(date, "Monday 14 September");
    }

    #[test]
    fn a_twelve_hour_clock_says_am_or_pm() {
        let at: jiff::Zoned = "2026-09-14T14:30:00[Europe/Oslo]".parse().unwrap();
        assert_eq!(format(&at, false).0, "2:30 PM");
        let at: jiff::Zoned = "2026-09-14T00:05:00[Europe/Oslo]".parse().unwrap();
        assert_eq!(format(&at, false).0, "12:05 AM");
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

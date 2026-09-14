//! What the clock in the sky says.
//!
//! In the machine's own time zone, which the installer set: `/etc/localtime`,
//! read directly, because musl's `localtime` is not something Rust's standard
//! library exposes and a clock an hour out is worse than none.

use jiff::Zoned;

/// The time as `HH:MM`, and the date written out, for now.
#[must_use]
pub fn now() -> (String, String) {
    format(&Zoned::now())
}

/// The time and date for `at`.
#[must_use]
pub fn format(at: &Zoned) -> (String, String) {
    (
        at.strftime("%H:%M").to_string(),
        at.strftime("%A %-d %B").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::format;

    #[test]
    fn the_clock_is_twenty_four_hour_and_the_date_is_written_out() {
        let at: jiff::Zoned = "2026-09-14T07:05:00[Europe/Oslo]".parse().unwrap();
        let (clock, date) = format(&at);
        assert_eq!(clock, "07:05");
        assert_eq!(date, "Monday 14 September");
    }
}

//! Noticing a desktop that stopped as soon as it started.
//!
//! greetd starts the login screen again whenever a session ends, so a desktop
//! that dies at once only looks like a login that did nothing, again and
//! again. The greeter notes when it hands the display over, in seconds since
//! boot so that neither the clock nor a reboot can confuse it. Started again
//! within [`QUICK`] of that, it says the desktop stopped, and records whose it
//! was and in which boot, so a login at a text console can print why:
//! `/etc/profile.d/alpymist-session.sh` reads [`STOPPED`] and shows the end of
//! the session's log, which `alpymist session` writes.

/// Seconds since boot and the account, from the last hand-over.
pub const HANDED_OVER: &str = "/var/cache/alpymist-greeter/handed-over";

/// The account and boot of a desktop that stopped as it started. Read by
/// `/etc/profile.d/alpymist-session.sh`, so the format is plain: `USER BOOT_ID`.
pub const STOPPED: &str = "/var/cache/alpymist-greeter/stopped";

/// Where the kernel says which boot this is.
pub const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// A session shorter than this did not really start: nobody logs in and out
/// again that fast on purpose, and a compositor that cannot start gives up well
/// within it.
pub const QUICK: f64 = 20.0;

/// Seconds since boot, from the contents of `/proc/uptime`.
#[must_use]
pub fn uptime(proc_uptime: &str) -> Option<f64> {
    proc_uptime.split_whitespace().next()?.parse().ok()
}

/// What [`HANDED_OVER`] holds after `user` logs in `at` seconds after boot.
#[must_use]
pub fn handed_over(user: &str, at: f64) -> String {
    format!("{at:.1} {user}\n")
}

/// The account whose desktop stopped as soon as it started, if [`HANDED_OVER`]
/// says there was one: handed over less than [`QUICK`] before `now`, in this
/// boot.
#[must_use]
pub fn stopped_quickly(record: &str, now: f64) -> Option<&str> {
    let (at, user) = record.trim().split_once(' ')?;
    let at: f64 = at.parse().ok()?;
    let user = user.trim();
    // A later boot starts the count again, so a hand-over "after" now was in
    // an earlier one.
    ((0.0..QUICK).contains(&(now - at)) && !user.is_empty()).then_some(user)
}

/// What [`STOPPED`] holds for `user`'s desktop in the boot `boot_id`.
#[must_use]
pub fn stopped(user: &str, boot_id: &str) -> String {
    format!("{user} {}\n", boot_id.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_is_the_first_number_the_kernel_gives() {
        assert_eq!(uptime("52087.31 101532.12\n"), Some(52087.31));
        assert_eq!(uptime(""), None);
    }

    #[test]
    fn a_session_that_ended_within_seconds_is_one_that_stopped() {
        let record = handed_over("andre", 120.0);
        assert_eq!(stopped_quickly(&record, 121.5), Some("andre"));
    }

    #[test]
    fn a_session_that_ran_a_while_is_not() {
        let record = handed_over("andre", 120.0);
        assert_eq!(stopped_quickly(&record, 120.0 + QUICK), None);
        assert_eq!(stopped_quickly(&record, 4000.0), None);
    }

    #[test]
    fn a_hand_over_from_an_earlier_boot_is_not() {
        let record = handed_over("andre", 5000.0);
        assert_eq!(stopped_quickly(&record, 30.0), None);
    }

    #[test]
    fn a_damaged_record_is_not() {
        assert_eq!(stopped_quickly("", 10.0), None);
        assert_eq!(stopped_quickly("soon andre", 10.0), None);
        assert_eq!(stopped_quickly("9.0 ", 10.0), None);
    }

    #[test]
    fn the_stopped_record_is_what_the_profile_script_compares() {
        assert_eq!(
            stopped("andre", "8b0c4f6e-7b1a-4c35-9d9e-1f0d6a1c2b3d\n"),
            "andre 8b0c4f6e-7b1a-4c35-9d9e-1f0d6a1c2b3d\n"
        );
    }
}

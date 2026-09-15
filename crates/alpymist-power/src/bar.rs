//! The battery as the bar shows it, and as the command line prints it.
//!
//! Waybar runs `alpymist-power --waybar` as a custom module. The icon is one of
//! eleven levels, 0 to 100 in tens, with a bolt through it while charging, so
//! it says what the battery says rather than one of five rough steps; beside
//! it, whatever the user chose in the popup. Without a battery the text is
//! empty and Waybar hides the module.

use crate::battery::{Power, Status, clock, spoken};
use crate::config::Bar;
use crate::profile::Profile;
use std::fmt::Write as _;

/// Discharging, by tens from empty to full, from Nerd Font's Material Design
/// set.
pub const LEVELS: [&str; 11] = [
    "\u{f008e}",
    "\u{f007a}",
    "\u{f007b}",
    "\u{f007c}",
    "\u{f007d}",
    "\u{f007e}",
    "\u{f007f}",
    "\u{f0080}",
    "\u{f0081}",
    "\u{f0082}",
    "\u{f0079}",
];
/// Charging, by tens from empty to full.
pub const CHARGING: [&str; 11] = [
    "\u{f089f}",
    "\u{f089c}",
    "\u{f0086}",
    "\u{f0087}",
    "\u{f0088}",
    "\u{f089d}",
    "\u{f0089}",
    "\u{f089e}",
    "\u{f008a}",
    "\u{f008b}",
    "\u{f0085}",
];
/// The level cannot be read.
pub const UNKNOWN: &str = "\u{f0091}";
/// Very low and not charging.
pub const ALERT: &str = "\u{f0083}";
/// On mains power, with no battery.
pub const PLUG: &str = "\u{f06a5}";

/// At or under this, the bar warns.
pub const WARNING: u8 = 20;
/// At or under this, the bar alarms.
pub const CRITICAL: u8 = 10;

/// The icon for a battery at `level`, charging or not.
#[must_use]
pub fn level_icon(level: u8, charging: bool) -> &'static str {
    // Rounded to the nearest ten, so 95% shows full and 4% empty.
    let step = usize::from((level.min(100) + 5) / 10);
    if charging {
        CHARGING[step]
    } else {
        LEVELS[step]
    }
}

/// The icon for the bar.
#[must_use]
pub fn icon(power: &Power) -> &'static str {
    if !power.has_battery() {
        return "";
    }
    let Some(level) = power.level() else {
        return UNKNOWN;
    };
    match power.status() {
        Status::Charging => level_icon(level, true),
        // Plugged in and holding shows the plain level: nothing is happening.
        Status::Discharging | Status::Unknown if level <= CRITICAL / 2 => ALERT,
        _ => level_icon(level, false),
    }
}

/// A class for the bar's stylesheet.
#[must_use]
pub fn class(power: &Power) -> &'static str {
    if !power.has_battery() {
        return "no-battery";
    }
    let level = power.level().unwrap_or(100);
    match power.status() {
        Status::Charging => "charging",
        Status::Full => "full",
        Status::NotCharging => "plugged",
        _ if level <= CRITICAL => "critical",
        _ if level <= WARNING => "warning",
        _ => "discharging",
    }
}

/// What goes beside the icon.
#[must_use]
pub fn text(power: &Power, profile: Option<Profile>, show: &Bar) -> String {
    let icon = icon(power);
    if icon.is_empty() {
        return String::new();
    }
    let mut parts = vec![icon.to_owned()];
    if show.profile
        && let Some(p) = profile
        && p != Profile::Balanced
    {
        parts.push(p.icon().to_owned());
    }
    if show.percentage
        && let Some(level) = power.level()
    {
        parts.push(format!("{level}%"));
    }
    if show.time
        && let Some(left) = power.time_left()
    {
        parts.push(clock(left));
    }
    if show.power
        && let Some(w) = power.power_w().filter(|w| *w > 0.05)
    {
        parts.push(format!("{w:.1} W"));
    }
    parts.join(" ")
}

/// A sentence for where the battery is: `Discharging · 3 h 12 min left`.
#[must_use]
pub fn status_line(power: &Power) -> String {
    if !power.has_battery() {
        return "No battery · on mains power".into();
    }
    let left = power.time_left().map(spoken);
    match (power.status(), left) {
        (Status::Charging, Some(t)) => format!("Charging · full in {t}"),
        (Status::Charging, None) => "Charging".into(),
        (Status::Discharging, Some(t)) => format!("On battery · {t} left"),
        (Status::Discharging, None) => "On battery".into(),
        (Status::Full, _) => "Fully charged".into(),
        (Status::NotCharging, _) => "Plugged in · not charging".into(),
        (Status::Unknown, _) => "Battery".into(),
    }
}

/// A few lines saying where power is: the tooltip, and `alpymist-power status`.
#[must_use]
pub fn summary(power: &Power, profile: Option<Profile>) -> String {
    let mut out = match power.level() {
        Some(level) if power.has_battery() => format!("Battery {level}%\n{}", status_line(power)),
        _ => status_line(power),
    };
    if let Some(w) = power.power_w().filter(|w| *w > 0.05) {
        let _ = write!(out, "\n{w:.1} W");
    }
    if let Some(health) = power.main().and_then(crate::battery::Battery::health) {
        let _ = write!(out, "\nHealth {health}%");
    }
    if let Some(p) = profile {
        let _ = write!(out, "\nMode: {}", p.label());
    }
    out
}

/// The line Waybar reads.
#[must_use]
pub fn waybar(power: &Power, profile: Option<Profile>, show: &Bar) -> String {
    alpymist_widget::waybar::line(
        &text(power, profile, show),
        &summary(power, profile),
        class(power),
    )
}

#[cfg(test)]
mod tests {
    use super::{ALERT, CHARGING, LEVELS, class, icon, level_icon, text, waybar};
    use crate::battery::{Power, Status, sample};
    use crate::config::Bar;
    use crate::profile::Profile;

    #[test]
    fn the_icon_follows_the_level_in_tens_and_the_charger() {
        assert_eq!(level_icon(0, false), LEVELS[0]);
        assert_eq!(level_icon(4, false), LEVELS[0]);
        assert_eq!(level_icon(5, false), LEVELS[1]);
        assert_eq!(level_icon(78, false), LEVELS[8]);
        assert_eq!(level_icon(95, true), CHARGING[10]);
        assert_eq!(level_icon(100, true), CHARGING[10]);
        let mut p = sample();
        assert_eq!(icon(&p), LEVELS[8]);
        p.batteries[0].status = Status::Charging;
        assert_eq!(icon(&p), CHARGING[8]);
        assert_eq!(class(&p), "charging");
    }

    #[test]
    fn nearly_empty_on_battery_is_an_alert() {
        let mut p = sample();
        p.batteries[0].energy_wh = Some(2.0);
        assert_eq!(icon(&p), ALERT);
        assert_eq!(class(&p), "critical");
    }

    #[test]
    fn beside_the_icon_is_what_was_chosen() {
        let p = sample();
        let all = Bar {
            percentage: true,
            time: true,
            power: true,
            profile: true,
        };
        assert_eq!(
            text(&p, Some(Profile::PowerSaver), &all),
            format!(
                "{} {} 78% 5:17 7.4 W",
                LEVELS[8],
                Profile::PowerSaver.icon()
            )
        );
        let none = Bar {
            percentage: false,
            time: false,
            power: false,
            profile: false,
        };
        assert_eq!(text(&p, Some(Profile::Balanced), &none), LEVELS[8]);
    }

    #[test]
    fn no_battery_hides_the_module() {
        let line = waybar(&Power::default(), None, &Bar::default());
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["text"], "");
        assert!(!line.contains('\n'));
    }
}

//! Date & time: the time zone, setting the clock from the network, and a
//! 24-hour or 12-hour clock.
//!
//! The time zone is `/etc/localtime`, in the shape Alpine's `setup-timezone`
//! leaves it: a copy of the zone under `/etc/zoneinfo`, and the link pointing
//! at it, so it stands even if tzdata is ever removed. It is written here
//! rather than by `setup-timezone`, which adds and removes tzdata with apk
//! every time it runs; tzdata is always installed, because the top bar needs
//! it. A zone copied before is left where it is.
//!
//! Network time is busybox's `ntpd`, which every system already has, as a
//! client only: in the default runlevel and running, or neither.
//!
//! The 24-hour clock is the system's, since the login screen shows a clock
//! before anyone has logged in: in `/etc/alpymist/settings.toml`, which the
//! login and lock screens read, and from it `/etc/alpymist/waybar/clock.jsonc`,
//! which the packaged top bar includes ahead of its own clock, so it wins.
//! Waybar's formats cannot drop a leading zero, so the bar says `03:30 PM`
//! where the login screen says `3:30 PM`.

use crate::env::Env;
use crate::generated;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use crate::values::Values;
use alpymist_core::catalog::{self, Zone};
use std::path::Path;

/// The link to the zone.
pub const LOCALTIME: &str = "etc/localtime";
/// Where the zone is copied to.
pub const COPIES: &str = "etc/zoneinfo";
/// Where tzdata keeps every zone.
pub const TZDATA: &str = "usr/share/zoneinfo";
/// The 24-hour clock's id.
pub const TWENTY_FOUR: &str = "datetime.24-hour";
/// The top bar's clock, generated.
pub const BAR_CLOCK: &str = "etc/alpymist/waybar/clock.jsonc";

/// How a zone reads in a list: `Oslo, Norway`.
fn label(z: &Zone) -> String {
    if z.country.is_empty() {
        return z.zone.to_owned();
    }
    let city = z
        .zone
        .rsplit('/')
        .next()
        .unwrap_or(z.zone)
        .replace('_', " ");
    format!("{city}, {}", z.name)
}

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![
        Setting {
            id: "datetime.timezone",
            title: "Time zone",
            description: "The time zone the clock shows, for everyone on this computer.",
            keywords: &["timezone", "tz", "clock", "region", "city", "daylight"],
            kind: Kind::Choice(
                catalog::zones()
                    .iter()
                    .map(|z| Choice::new(z.zone, label(z)))
                    .collect(),
            ),
            default: Value::Text(catalog::DEFAULT_ZONE.into()),
            scope: Scope::System,
            applies: Applies::NextLogin,
        },
        Setting {
            id: "datetime.network-time",
            title: "Set the time from the network",
            description: "Keep the clock right by asking time servers on the internet.",
            keywords: &["ntp", "ntpd", "sync", "clock", "internet time"],
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::Now,
        },
        Setting {
            id: TWENTY_FOUR,
            title: "24-hour clock",
            description: "14:30 rather than 2:30 PM, in the top bar and on the login and lock screens.",
            keywords: &["12-hour", "am", "pm", "time format", "clock"],
            kind: Kind::Switch,
            default: Value::Bool(true),
            scope: Scope::System,
            applies: Applies::Now,
        },
    ]
}

/// The zone `/etc/localtime` points at, as tzdata names it.
fn zone_of(link: &Path) -> Option<String> {
    let target = std::fs::read_link(link).ok()?;
    let target = target.to_str()?;
    target
        .split_once("/zoneinfo/")
        .map(|(_, zone)| zone.to_owned())
}

/// Its value.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    if setting.id == "datetime.network-time" {
        return Ok(Value::Bool(crate::service::at_boot(env, "ntpd")));
    }
    if setting.id == TWENTY_FOUR {
        return Ok(Values::load(&env.system(super::keyboard::VALUES))?
            .get(TWENTY_FOUR)
            .unwrap_or_else(|| setting.default.clone()));
    }
    let link = env.system(LOCALTIME);
    match zone_of(&link) {
        Some(zone) => Ok(Value::Text(zone)),
        // No zone set at all is UTC, to musl and everyone else.
        None if link.symlink_metadata().is_err() => Ok(Value::Text(catalog::DEFAULT_ZONE.into())),
        None => Err(format!(
            "{} is not a link to a zone, so which one it is cannot be told",
            link.display()
        )),
    }
}

/// Set one, or reset it with `None`, as root. `force` replaces a bar clock
/// edited by hand.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>, force: bool) -> Result<(), String> {
    if setting.id == TWENTY_FOUR {
        return clock(env, value, force);
    }
    let value = value.unwrap_or(&setting.default);
    if setting.id == "datetime.network-time" {
        return network_time(env, value.as_bool().unwrap_or(false));
    }
    timezone(env, value.as_text().unwrap_or(catalog::DEFAULT_ZONE))
}

/// Copy the zone to `/etc/zoneinfo` and point `/etc/localtime` at it, the
/// link replaced in one step.
fn timezone(env: &Env, zone: &str) -> Result<(), String> {
    let from = env.system(TZDATA).join(zone);
    let bytes = std::fs::read(&from).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{} is missing: is tzdata installed?", from.display())
        } else {
            crate::io_error(&from, &e)
        }
    })?;
    let copy = env.system(COPIES).join(zone);
    if let Some(dir) = copy.parent() {
        std::fs::create_dir_all(dir).map_err(|e| crate::io_error(dir, &e))?;
    }
    std::fs::write(&copy, bytes).map_err(|e| crate::io_error(&copy, &e))?;

    let link = env.system(LOCALTIME);
    let mut new = link.as_os_str().to_owned();
    new.push(".alpymist-new");
    let new = std::path::PathBuf::from(new);
    std::fs::remove_file(&new).ok();
    std::os::unix::fs::symlink(Path::new("/").join(COPIES).join(zone), &new)
        .map_err(|e| crate::io_error(&new, &e))?;
    std::fs::rename(&new, &link).map_err(|e| crate::io_error(&link, &e))
}

/// Write the bar's clock, then the value, which the login and lock screens
/// read themselves.
fn clock(env: &Env, value: Option<&Value>, force: bool) -> Result<(), String> {
    let path = env.system(super::keyboard::VALUES);
    let mut values = Values::load(&path)?;
    match value {
        Some(v) => values.set(TWENTY_FOUR, v),
        None => values.remove(TWENTY_FOUR),
    }
    let hours = values
        .get(TWENTY_FOUR)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // The bar's file first: if it was edited by hand, nothing changes.
    generated::write(
        &env.system(BAR_CLOCK),
        "//",
        "/etc/alpymist/settings.toml",
        &bar_clock(hours),
        |_| false,
        force,
    )?;
    values.save(&path)
}

/// The bar's clock: as the packaged bar has it, or in 12 hours.
fn bar_clock(twenty_four: bool) -> String {
    let time = if twenty_four { "%H:%M" } else { "%I:%M %p" };
    format!(
        "{{\n  \"clock\": {{\n    \"format\": \"{{:%a {time}}}\",\n    \
         \"format-alt\": \"{{:%A %d %B {time}}}\"\n  }}\n}}\n"
    )
}

/// Have the account's top bar read its configuration again. A bar that is not
/// running has nothing to show, so its absence is not a failure.
pub fn live(env: &Env, setting: &Setting) {
    if setting.id == TWENTY_FOUR {
        let _ = env.run(&["pkill", "-USR2", "-x", "waybar"]);
    }
}

/// Start ntpd now and at every boot, or stop it and take it out.
fn network_time(env: &Env, on: bool) -> Result<(), String> {
    if on {
        crate::service::start(env, "ntpd")
    } else {
        crate::service::stop(env, "ntpd")
    }
}

#[cfg(test)]
mod tests {
    use super::{BAR_CLOCK, COPIES, LOCALTIME, TWENTY_FOUR, TZDATA, bar_clock, label};
    use crate::env::Env;
    use crate::{Error, Settings, Value};
    use alpymist_core::catalog::zones;
    use std::sync::Mutex;

    fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("alpymist-datetime-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn a_zone_reads_as_its_city_and_country() {
        let find = |z: &str| zones().iter().find(|x| x.zone == z).unwrap();
        assert_eq!(label(find("Europe/Oslo")), "Oslo, Norway");
        assert_eq!(label(find("America/New_York")), "New York, United States");
        assert_eq!(label(find("UTC")), "UTC");
    }

    #[test]
    fn a_zone_is_copied_and_linked_as_setup_timezone_leaves_it() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("zone");
        let settings = Settings::new();
        assert_eq!(
            settings.get(&Env::test(&d, false, &RAN), "datetime.timezone"),
            Ok(Value::Text("UTC".into()))
        );
        assert_eq!(
            settings.set(
                &Env::test(&d, false, &RAN),
                "datetime.timezone",
                "Europe/Oslo",
                false
            ),
            Err(Error::NeedsRoot("datetime.timezone".into()))
        );

        let env = Env::test(&d, true, &RAN);
        let tzdata = env.system(TZDATA).join("Europe");
        std::fs::create_dir_all(&tzdata).unwrap();
        std::fs::write(tzdata.join("Oslo"), "TZif oslo").unwrap();
        std::fs::write(tzdata.join("London"), "TZif london").unwrap();
        std::fs::create_dir_all(env.system("etc")).unwrap();

        settings
            .set(&env, "datetime.timezone", "Europe/Oslo", false)
            .unwrap();
        let link = env.system(LOCALTIME);
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            std::path::Path::new("/etc/zoneinfo/Europe/Oslo")
        );
        assert_eq!(
            std::fs::read_to_string(env.system(COPIES).join("Europe/Oslo")).unwrap(),
            "TZif oslo"
        );
        assert_eq!(
            settings.get(&env, "datetime.timezone"),
            Ok(Value::Text("Europe/Oslo".into()))
        );

        settings
            .set(&env, "datetime.timezone", "Europe/London", false)
            .unwrap();
        assert_eq!(
            settings.get(&env, "datetime.timezone"),
            Ok(Value::Text("Europe/London".into()))
        );

        let missing = settings.set(&env, "datetime.timezone", "Asia/Tokyo", false);
        assert!(
            matches!(missing, Err(Error::Failed(ref m)) if m.contains("tzdata")),
            "{missing:?}"
        );
        assert_eq!(
            settings.get(&env, "datetime.timezone"),
            Ok(Value::Text("Europe/London".into()))
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_24_hour_clock_is_the_packaged_bars_own() {
        let packaged = include_str!("../../../../desktop/waybar/modules.jsonc");
        for line in [
            "\"format\": \"{:%a %H:%M}\"",
            "\"format-alt\": \"{:%A %d %B %H:%M}\"",
        ] {
            assert!(packaged.contains(line), "the packaged bar has no {line}");
            assert!(bar_clock(true).contains(line), "{}", bar_clock(true));
        }
        // Waybar's formats refuse %-I; %I is what it takes.
        assert!(bar_clock(false).contains("\"format\": \"{:%a %I:%M %p}\""));
    }

    #[test]
    fn the_clock_writes_the_bars_file_and_the_systems_value() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("clock");
        let env = Env::test(&d, true, &RAN);
        let settings = Settings::new();
        assert_eq!(settings.get(&env, TWENTY_FOUR), Ok(Value::Bool(true)));
        settings.set(&env, TWENTY_FOUR, "off", false).unwrap();
        assert_eq!(settings.get(&env, TWENTY_FOUR), Ok(Value::Bool(false)));
        let values = std::fs::read_to_string(env.system(crate::areas::keyboard::VALUES)).unwrap();
        assert!(values.contains("\"datetime.24-hour\" = false"), "{values}");
        let bar = std::fs::read_to_string(env.system(BAR_CLOCK)).unwrap();
        assert!(bar.starts_with("// Written by alpymist"), "{bar}");
        assert!(bar.contains("%I:%M %p"), "{bar}");

        settings
            .live(&env, TWENTY_FOUR, &Value::Bool(false))
            .unwrap();
        assert_eq!(RAN.lock().unwrap().as_slice(), ["pkill -USR2 -x waybar"]);

        std::fs::write(env.system(BAR_CLOCK), "{}\n").unwrap();
        let refused = settings.set(&env, TWENTY_FOUR, "on", false);
        assert!(
            matches!(refused, Err(Error::Failed(ref m)) if m.contains("by hand")),
            "{refused:?}"
        );
        assert_eq!(settings.get(&env, TWENTY_FOUR), Ok(Value::Bool(false)));
        settings.reset(&env, TWENTY_FOUR, true).unwrap();
        assert_eq!(settings.get(&env, TWENTY_FOUR), Ok(Value::Bool(true)));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn network_time_is_ntpd_at_boot() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("ntp");
        let env = Env::test(&d, true, &RAN);
        let settings = Settings::new();
        assert_eq!(
            settings.get(&env, "datetime.network-time"),
            Ok(Value::Bool(false))
        );
        settings
            .set(&env, "datetime.network-time", "on", false)
            .unwrap();
        settings
            .reset(&env, "datetime.network-time", false)
            .unwrap();
        let link = crate::service::link(&env, "ntpd");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "").unwrap();
        assert_eq!(
            settings.get(&env, "datetime.network-time"),
            Ok(Value::Bool(true))
        );
        settings
            .set(&env, "datetime.network-time", "off", false)
            .unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "rc-update add ntpd default",
                "rc-service ntpd start",
                "rc-service --ifstarted ntpd stop",
                "rc-service --ifstarted ntpd stop",
                "rc-update del ntpd default",
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

//! Date & time: the time zone, and setting the clock from the network.
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

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::catalog::{self, Zone};
use std::path::Path;

/// The link to the zone.
pub const LOCALTIME: &str = "etc/localtime";
/// Where the zone is copied to.
pub const COPIES: &str = "etc/zoneinfo";
/// Where tzdata keeps every zone.
pub const TZDATA: &str = "usr/share/zoneinfo";
/// ntpd's link in the default runlevel.
pub const RUNLEVEL: &str = "etc/runlevels/default/ntpd";

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
        return Ok(Value::Bool(env.system(RUNLEVEL).symlink_metadata().is_ok()));
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

/// Set one, or reset it with `None`, as root.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
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

/// Start ntpd now and at every boot, or stop it and take it out.
fn network_time(env: &Env, on: bool) -> Result<(), String> {
    if on {
        env.run(&["rc-update", "add", "ntpd", "default"])?;
        return env.run(&["rc-service", "ntpd", "start"]).map(drop);
    }
    env.run(&["rc-service", "--ifstarted", "ntpd", "stop"])?;
    // rc-update fails for a service that is not in the runlevel.
    if env.system(RUNLEVEL).symlink_metadata().is_ok() {
        env.run(&["rc-update", "del", "ntpd", "default"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{COPIES, LOCALTIME, RUNLEVEL, TZDATA, label};
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
        let link = env.system(RUNLEVEL);
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

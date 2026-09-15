//! Batteries and the mains, as the kernel reports them.
//!
//! Everything is read from `/sys/class/power_supply`: no daemon to start, and
//! the same numbers `UPower` would pass on. A supply is either the system's
//! battery, a charger (mains, USB), or a peripheral's battery — a mouse, a
//! headset — which the kernel marks with `scope = Device` and which is left
//! out: the bar is about the machine.
//!
//! Drivers report either energy (µWh, µW) or charge (µAh, µA); charge is
//! turned into energy with the design voltage, so everything downstream
//! speaks watt-hours.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the kernel lists power supplies.
pub const SYSFS: &str = "/sys/class/power_supply";

/// What a battery is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Taking charge.
    Charging,
    /// Running the machine.
    Discharging,
    /// Plugged in and full.
    Full,
    /// Plugged in and holding: a charge limit, or a charger too weak.
    NotCharging,
    /// The driver does not say.
    #[default]
    Unknown,
}

impl Status {
    fn parse(s: &str) -> Self {
        match s {
            "Charging" => Self::Charging,
            "Discharging" => Self::Discharging,
            "Full" => Self::Full,
            "Not charging" => Self::NotCharging,
            _ => Self::Unknown,
        }
    }
}

/// One battery.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Battery {
    /// The kernel's name for it: `BAT0`.
    pub name: String,
    /// What it is doing.
    pub status: Status,
    /// Charge, in percent, as the driver reports it.
    pub capacity: Option<u8>,
    /// Energy now, Wh.
    pub energy_wh: Option<f64>,
    /// Energy when full now, Wh.
    pub full_wh: Option<f64>,
    /// Energy when full as designed, Wh.
    pub design_wh: Option<f64>,
    /// Power flowing in or out, W, always positive.
    pub power_w: Option<f64>,
    /// Voltage now, V.
    pub voltage_v: Option<f64>,
    /// Charge cycles counted.
    pub cycles: Option<u32>,
    /// Chemistry: `Li-ion`.
    pub technology: Option<String>,
    /// Who made it.
    pub manufacturer: Option<String>,
    /// Its model name.
    pub model: Option<String>,
    /// Temperature, °C.
    pub temperature_c: Option<f64>,
    /// Where charging stops, in percent, where the firmware allows a limit.
    pub charge_limit: Option<u8>,
}

impl Battery {
    /// Health: full now against full as designed, in percent.
    #[must_use]
    pub fn health(&self) -> Option<u8> {
        let (full, design) = (self.full_wh?, self.design_wh?);
        (design > 0.0).then(|| percent(full / design))
    }
}

/// The machine's power, read at one moment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Power {
    /// System batteries, in the kernel's order.
    pub batteries: Vec<Battery>,
    /// Whether a charger is connected, where one is listed.
    pub plugged: Option<bool>,
}

fn percent(fraction: f64) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p = (fraction * 100.0).round().clamp(0.0, 255.0) as u8;
    p
}

impl Power {
    /// Read every supply under `root` — [`SYSFS`], or a copy of it in a test.
    #[must_use]
    pub fn read(root: &Path) -> Self {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
            .map(|dir| dir.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        entries.sort();
        let mut power = Self::default();
        for dir in entries {
            let kind = read(&dir, "type").unwrap_or_default();
            let device_scope = read(&dir, "scope").is_some_and(|s| s == "Device");
            match kind.as_str() {
                "Battery" if !device_scope => power.batteries.push(battery(&dir)),
                "Mains" | "USB" | "Wireless" if !device_scope => {
                    if let Some(online) = number(&dir, "online") {
                        let on = online > 0.0;
                        power.plugged = Some(power.plugged.unwrap_or(false) || on);
                    }
                }
                _ => {}
            }
        }
        power
    }

    /// Read the machine's supplies.
    #[must_use]
    pub fn now() -> Self {
        Self::read(Path::new(SYSFS))
    }

    /// Whether there is a battery at all.
    #[must_use]
    pub fn has_battery(&self) -> bool {
        !self.batteries.is_empty()
    }

    /// The first battery, whose details the popup shows.
    #[must_use]
    pub fn main(&self) -> Option<&Battery> {
        self.batteries.first()
    }

    fn sum(&self, f: impl Fn(&Battery) -> Option<f64>) -> Option<f64> {
        self.batteries.iter().map(f).sum()
    }

    /// Charge across all batteries, in percent.
    #[must_use]
    pub fn level(&self) -> Option<u8> {
        if let (Some(now), Some(full)) = (self.sum(|b| b.energy_wh), self.sum(|b| b.full_wh))
            && full > 0.0
        {
            return Some(percent(now / full).min(100));
        }
        let known: Vec<u8> = self.batteries.iter().filter_map(|b| b.capacity).collect();
        let count = u32::try_from(known.len()).ok().filter(|n| *n > 0)?;
        let total: u32 = known.iter().map(|c| u32::from(*c)).sum();
        u8::try_from(total / count).ok().map(|l| l.min(100))
    }

    /// What the batteries are doing, together: charging if any is, full if
    /// all are.
    #[must_use]
    pub fn status(&self) -> Status {
        let all = |s: Status| self.batteries.iter().all(|b| b.status == s);
        let any = |s: Status| self.batteries.iter().any(|b| b.status == s);
        if self.batteries.is_empty() {
            Status::Unknown
        } else if any(Status::Charging) {
            Status::Charging
        } else if any(Status::Discharging) {
            Status::Discharging
        } else if all(Status::Full) {
            Status::Full
        } else if any(Status::NotCharging) || self.plugged == Some(true) {
            // Some drivers say Unknown while holding at a limit; plugged in,
            // that is what it means.
            if self.level().is_some_and(|l| l >= 98) {
                Status::Full
            } else {
                Status::NotCharging
            }
        } else {
            Status::Unknown
        }
    }

    /// Whether the machine is running on its charger.
    #[must_use]
    pub fn on_mains(&self) -> bool {
        match self.plugged {
            Some(p) => p,
            None => !matches!(self.status(), Status::Discharging),
        }
    }

    /// Power flowing in or out of all batteries, W.
    #[must_use]
    pub fn power_w(&self) -> Option<f64> {
        self.sum(|b| b.power_w)
    }

    /// Until empty when discharging, until full when charging.
    #[must_use]
    pub fn time_left(&self) -> Option<Duration> {
        let power = self.power_w().filter(|p| *p > 0.1)?;
        let now = self.sum(|b| b.energy_wh)?;
        let hours = match self.status() {
            Status::Discharging => now / power,
            Status::Charging => (self.sum(|b| b.full_wh)? - now).max(0.0) / power,
            _ => return None,
        };
        // Anything over two days is a reading taken at idle, not a forecast.
        (hours.is_finite() && hours < 48.0).then(|| Duration::from_secs_f64(hours * 3600.0))
    }
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn number(dir: &Path, name: &str) -> Option<f64> {
    read(dir, name)?.parse().ok()
}

/// A reading in millionths.
fn micro(dir: &Path, name: &str) -> Option<f64> {
    number(dir, name).map(|v| v / 1_000_000.0)
}

fn battery(dir: &Path) -> Battery {
    let voltage = micro(dir, "voltage_now");
    let design_voltage = micro(dir, "voltage_min_design")
        .or_else(|| micro(dir, "voltage_max_design"))
        .or(voltage);
    // Charge in Ah into energy in Wh.
    let from_charge = |name: &str| Some(micro(dir, name)? * design_voltage?);
    let energy = |e: &str, c: &str| micro(dir, e).or_else(|| from_charge(c));
    let power = micro(dir, "power_now")
        .or_else(|| Some(micro(dir, "current_now")? * voltage?))
        .map(f64::abs);
    let capacity = number(dir, "capacity")
        .and_then(|c| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            u8::try_from(c.clamp(0.0, 100.0) as u32).ok()
        })
        .or_else(|| {
            // The coarse level some firmware gives instead.
            Some(match read(dir, "capacity_level")?.as_str() {
                "Critical" => 5,
                "Low" => 15,
                "Normal" => 60,
                "High" => 85,
                "Full" => 100,
                _ => return None,
            })
        });
    let mut b = Battery {
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        status: Status::parse(&read(dir, "status").unwrap_or_default()),
        capacity,
        energy_wh: energy("energy_now", "charge_now"),
        full_wh: energy("energy_full", "charge_full"),
        design_wh: energy("energy_full_design", "charge_full_design"),
        power_w: power,
        voltage_v: voltage,
        cycles: number(dir, "cycle_count").and_then(|c| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            u32::try_from(c as u64).ok()
        }),
        technology: read(dir, "technology").filter(|t| t != "Unknown"),
        manufacturer: read(dir, "manufacturer"),
        model: read(dir, "model_name"),
        // Tenths of a degree.
        temperature_c: number(dir, "temp").map(|t| t / 10.0),
        charge_limit: number(dir, "charge_control_end_threshold").and_then(|c| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            u8::try_from(c as u32).ok()
        }),
    };
    // A cycle count of zero is what many drivers say when they do not count.
    if b.cycles == Some(0) {
        b.cycles = None;
    }
    b
}

/// A duration as people say it: `3 h 12 min`, `45 min`, `under a minute`.
#[must_use]
pub fn spoken(d: Duration) -> String {
    let minutes = d.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "under a minute".into(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// A duration for the bar: `3:12`.
#[must_use]
pub fn clock(d: Duration) -> String {
    let minutes = d.as_secs() / 60;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// A laptop's battery, discharging, for tests and previews.
#[must_use]
pub fn sample() -> Power {
    Power {
        batteries: vec![Battery {
            name: "BAT0".into(),
            status: Status::Discharging,
            capacity: Some(78),
            energy_wh: Some(39.2),
            full_wh: Some(50.3),
            design_wh: Some(57.0),
            power_w: Some(7.4),
            voltage_v: Some(11.84),
            cycles: Some(213),
            technology: Some("Li-ion".into()),
            manufacturer: Some("ASUSTeK".into()),
            model: Some("ASUS Battery".into()),
            temperature_c: None,
            charge_limit: Some(80),
        }],
        plugged: Some(false),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Power, Status, clock, spoken};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// A `power_supply` directory to read from.
    pub(crate) fn fake(name: &str, supplies: &[(&str, &[(&str, &str)])]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("alpymist-power-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (supply, files) in supplies {
            let dir = root.join(supply);
            std::fs::create_dir_all(&dir).unwrap();
            for (file, value) in *files {
                std::fs::write(dir.join(file), format!("{value}\n")).unwrap();
            }
        }
        root
    }

    #[test]
    fn an_energy_reporting_battery_reads_as_watt_hours() {
        let root = fake(
            "energy",
            &[
                (
                    "BAT0",
                    &[
                        ("type", "Battery"),
                        ("status", "Discharging"),
                        ("capacity", "50"),
                        ("energy_now", "20000000"),
                        ("energy_full", "40000000"),
                        ("energy_full_design", "50000000"),
                        ("power_now", "10000000"),
                        ("cycle_count", "0"),
                    ],
                ),
                ("AC", &[("type", "Mains"), ("online", "0")]),
                (
                    "hidpp_battery_0",
                    &[("type", "Battery"), ("scope", "Device"), ("capacity", "5")],
                ),
            ],
        );
        let p = Power::read(&root);
        assert_eq!(p.batteries.len(), 1, "the mouse is not the machine");
        let b = p.main().unwrap();
        assert_eq!(b.health(), Some(80));
        assert_eq!(b.cycles, None, "zero cycles means not counted");
        assert_eq!(p.level(), Some(50));
        assert_eq!(p.plugged, Some(false));
        assert_eq!(p.time_left(), Some(Duration::from_hours(2)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_charge_reporting_battery_is_converted_with_its_design_voltage() {
        let root = fake(
            "charge",
            &[(
                "BAT1",
                &[
                    ("type", "Battery"),
                    ("status", "Charging"),
                    ("charge_now", "2000000"),
                    ("charge_full", "4000000"),
                    ("voltage_min_design", "3800000"),
                    ("voltage_now", "4000000"),
                    ("current_now", "-1000000"),
                    ("temp", "312"),
                ],
            )],
        );
        let p = Power::read(Path::new(&root));
        let b = p.main().unwrap();
        assert!((b.energy_wh.unwrap() - 7.6).abs() < 1e-9);
        assert!((b.power_w.unwrap() - 4.0).abs() < 1e-9, "current is signed");
        assert_eq!(b.temperature_c, Some(31.2));
        assert_eq!(p.status(), Status::Charging);
        // 7.6 Wh to go at 4 W.
        assert_eq!(p.time_left().unwrap().as_secs(), 6840);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugged_in_at_a_limit_is_holding_not_unknown() {
        let mut p = super::sample();
        p.plugged = Some(true);
        p.batteries[0].status = Status::Unknown;
        assert_eq!(p.status(), Status::NotCharging);
        assert!(p.on_mains());
        assert_eq!(p.time_left(), None);
    }

    #[test]
    fn no_supplies_is_no_battery() {
        let p = Power::read(Path::new("/nonexistent"));
        assert!(!p.has_battery());
        assert_eq!(p.level(), None);
        assert!(p.on_mains());
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(spoken(Duration::from_mins(192)), "3 h 12 min");
        assert_eq!(spoken(Duration::from_hours(2)), "2 h");
        assert_eq!(spoken(Duration::from_secs(30)), "under a minute");
        assert_eq!(clock(Duration::from_mins(192)), "3:12");
    }
}

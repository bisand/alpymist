//! Power modes: Power saver, Balanced, Performance.
//!
//! The three GNOME and KDE offer, carried out on whatever this machine's
//! kernel exposes, in the order power-profiles-daemon prefers them:
//!
//! - the firmware's platform profile (`/sys/firmware/acpi/platform_profile`),
//!   which on most recent laptops also sets fans and power limits;
//! - the CPU's energy/performance preference (`energy_performance_preference`,
//!   `intel_pstate` and `amd_pstate`), which steers the processor itself;
//! - failing both, the cpufreq governor.
//!
//! power-profiles-daemon itself is not used: Alpine packages it without an
//! `OpenRC` service, and its permission check needs elogind's sessions, which
//! the desktop does not run. Writing these files needs root, so the plan is
//! worked out here, as data anyone can test, and carried out by
//! `alpymist-power-helper` through pkexec.

use std::path::{Path, PathBuf};

/// The system's sysfs root.
pub const SYSFS: &str = "/sys";

/// A power mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Profile {
    /// Longest on battery, slowest.
    PowerSaver,
    /// The default.
    Balanced,
    /// Fastest, warmest, loudest.
    Performance,
}

impl Profile {
    /// Every mode, in the order shown.
    pub const ALL: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    /// The name used on the command line and in files.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    /// The name shown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::PowerSaver => "Saver",
            Self::Balanced => "Balanced",
            Self::Performance => "Performance",
        }
    }

    /// Its icon, from Nerd Font's Material Design set: a leaf, scales, a
    /// rocket.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::PowerSaver => "\u{f032a}",
            Self::Balanced => "\u{f05d1}",
            Self::Performance => "\u{f14de}",
        }
    }

    /// A mode by its [`Profile::id`], or a few other spellings of it.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "power-saver" | "saver" | "powersave" | "battery" | "low-power" => {
                Some(Self::PowerSaver)
            }
            "balanced" | "balance" => Some(Self::Balanced),
            "performance" | "perf" => Some(Self::Performance),
            _ => None,
        }
    }

    /// Platform profile names for this mode, most fitting first.
    fn platform(self) -> &'static [&'static str] {
        match self {
            Self::PowerSaver => &["low-power", "quiet", "cool"],
            Self::Balanced => &["balanced"],
            Self::Performance => &["performance", "max-power"],
        }
    }

    fn epp(self) -> &'static [&'static str] {
        match self {
            Self::PowerSaver => &["power", "balance_power"],
            Self::Balanced => &["balance_performance", "default", "balance_power"],
            Self::Performance => &["performance"],
        }
    }

    fn governor(self) -> &'static [&'static str] {
        match self {
            Self::PowerSaver => &["powersave", "conservative"],
            Self::Balanced => &["schedutil", "ondemand", "conservative"],
            Self::Performance => &["performance"],
        }
    }
}

/// What this machine lets a mode change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Knobs {
    /// The platform profile's current value and choices.
    pub platform: Option<(String, Vec<String>)>,
    /// Each CPU policy directory, with its governor and governors available,
    /// and its energy/performance preference and choices where it has one.
    pub policies: Vec<Policy>,
}

/// One cpufreq policy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Policy {
    /// `devices/system/cpu/cpufreq/policy0`, under the sysfs root.
    pub dir: PathBuf,
    /// The scaling governor now.
    pub governor: Option<String>,
    /// Governors it offers.
    pub governors: Vec<String>,
    /// The energy/performance preference now.
    pub epp: Option<String>,
    /// Preferences it offers.
    pub epps: Vec<String>,
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn words(path: &Path) -> Vec<String> {
    read(path)
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn pick<'a>(wanted: &[&str], offered: &'a [String]) -> Option<&'a str> {
    wanted
        .iter()
        .find_map(|w| offered.iter().find(|o| o == w))
        .map(String::as_str)
}

impl Knobs {
    /// Read what a sysfs tree under `root` offers.
    #[must_use]
    pub fn read(root: &Path) -> Self {
        let acpi = root.join("firmware/acpi");
        let platform = read(&acpi.join("platform_profile"))
            .map(|now| (now, words(&acpi.join("platform_profile_choices"))));
        let cpufreq = root.join("devices/system/cpu/cpufreq");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&cpufreq)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("policy"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        let policies = dirs
            .into_iter()
            .map(|dir| Policy {
                governor: read(&dir.join("scaling_governor")),
                governors: words(&dir.join("scaling_available_governors")),
                epp: read(&dir.join("energy_performance_preference")),
                epps: words(&dir.join("energy_performance_available_preferences")),
                dir: dir.strip_prefix(root).unwrap_or(&dir).to_path_buf(),
            })
            .collect();
        Self { platform, policies }
    }

    /// Read the machine's.
    #[must_use]
    pub fn now() -> Self {
        Self::read(Path::new(SYSFS))
    }

    fn has_epp(&self) -> bool {
        self.policies.iter().any(|p| !p.epps.is_empty())
    }

    /// The modes this machine can be put in.
    #[must_use]
    pub fn available(&self) -> Vec<Profile> {
        Profile::ALL
            .into_iter()
            .filter(|p| !self.plan(*p).is_empty())
            .collect()
    }

    /// What mode the machine is in, as far as can be told.
    #[must_use]
    pub fn current(&self) -> Option<Profile> {
        let matches = |value: &str, pick: fn(Profile) -> &'static [&'static str]| {
            Profile::ALL.into_iter().find(|p| pick(*p).contains(&value))
        };
        if let Some((now, _)) = &self.platform {
            return matches(now, Profile::platform);
        }
        let first = self.policies.first()?;
        if let Some(epp) = &first.epp
            && !first.epps.is_empty()
        {
            return matches(epp, Profile::epp);
        }
        matches(first.governor.as_deref()?, Profile::governor)
    }

    /// The files to write, in order, to put the machine in `profile`. Empty
    /// when nothing here can express it.
    #[must_use]
    pub fn plan(&self, profile: Profile) -> Vec<(PathBuf, String)> {
        let mut writes = Vec::new();
        if let Some((_, choices)) = &self.platform
            && let Some(value) = pick(profile.platform(), choices)
        {
            writes.push((
                PathBuf::from("firmware/acpi/platform_profile"),
                value.to_owned(),
            ));
        }
        let epp = self.has_epp();
        for policy in &self.policies {
            let relative = |name: &str| policy.dir.join(name);
            if epp && !policy.epps.is_empty() {
                // A preference is only honoured under intel_pstate's and
                // amd_pstate's powersave governor; performance pins it.
                if policy.governor.as_deref() == Some("performance")
                    && policy.governors.iter().any(|g| g == "powersave")
                {
                    writes.push((relative("scaling_governor"), "powersave".into()));
                }
                if let Some(value) = pick(profile.epp(), &policy.epps) {
                    writes.push((relative("energy_performance_preference"), value.to_owned()));
                }
            } else if !epp
                && self.platform.is_none()
                && let Some(value) = pick(profile.governor(), &policy.governors)
            {
                writes.push((relative("scaling_governor"), value.to_owned()));
            }
        }
        writes
    }
}

#[cfg(test)]
mod tests {
    use super::{Knobs, Profile};
    use std::path::{Path, PathBuf};

    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("alpymist-profile-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (path, value) in files {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, format!("{value}\n")).unwrap();
        }
        root
    }

    const POLICY: &str = "devices/system/cpu/cpufreq/policy0";

    #[test]
    fn a_laptop_with_a_platform_profile_and_epp_sets_both() {
        let root = tree(
            "both",
            &[
                ("firmware/acpi/platform_profile", "balanced"),
                (
                    "firmware/acpi/platform_profile_choices",
                    "quiet balanced performance",
                ),
                (&format!("{POLICY}/scaling_governor"), "powersave"),
                (
                    &format!("{POLICY}/scaling_available_governors"),
                    "performance powersave",
                ),
                (
                    &format!("{POLICY}/energy_performance_preference"),
                    "balance_performance",
                ),
                (
                    &format!("{POLICY}/energy_performance_available_preferences"),
                    "default performance balance_performance balance_power power",
                ),
            ],
        );
        let k = Knobs::read(&root);
        assert_eq!(k.current(), Some(Profile::Balanced));
        assert_eq!(k.available(), Profile::ALL);
        let plan = k.plan(Profile::PowerSaver);
        assert_eq!(
            plan,
            [
                (
                    PathBuf::from("firmware/acpi/platform_profile"),
                    "quiet".into()
                ),
                (
                    Path::new(POLICY).join("energy_performance_preference"),
                    "power".into()
                ),
            ]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn without_either_the_governor_carries_the_mode() {
        let root = tree(
            "governor",
            &[
                (&format!("{POLICY}/scaling_governor"), "ondemand"),
                (
                    &format!("{POLICY}/scaling_available_governors"),
                    "conservative ondemand userspace powersave performance schedutil",
                ),
            ],
        );
        let k = Knobs::read(&root);
        assert_eq!(k.current(), Some(Profile::Balanced));
        assert_eq!(
            k.plan(Profile::Performance),
            [(
                Path::new(POLICY).join("scaling_governor"),
                "performance".into()
            )]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_pinned_preference_is_unpinned_first() {
        let root = tree(
            "pinned",
            &[
                (&format!("{POLICY}/scaling_governor"), "performance"),
                (
                    &format!("{POLICY}/scaling_available_governors"),
                    "performance powersave",
                ),
                (
                    &format!("{POLICY}/energy_performance_preference"),
                    "performance",
                ),
                (
                    &format!("{POLICY}/energy_performance_available_preferences"),
                    "performance balance_performance power",
                ),
            ],
        );
        let plan = Knobs::read(&root).plan(Profile::Balanced);
        assert_eq!(plan[0].1, "powersave");
        assert_eq!(plan[1].1, "balance_performance");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_machine_with_nothing_offers_nothing() {
        let k = Knobs::read(Path::new("/nonexistent"));
        assert!(k.available().is_empty());
        assert_eq!(k.current(), None);
        assert_eq!(Profile::parse("Power-Saver"), Some(Profile::PowerSaver));
    }
}

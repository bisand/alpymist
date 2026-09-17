//! What the About box says: read from files any account can read.
//!
//! Which Alpymist is installed comes from apk's database, not from this
//! binary, so the box reports what the system runs even when it is itself
//! older or newer than the rest. Everything is read under a root directory,
//! so tests and snapshots can hand it a made-up system.

use alpymist_core::Channel;
use std::fmt::Write as _;
use std::path::Path;

/// The package whose version is Alpymist's: the desktop every tier shares.
const DESKTOP: &str = "alpymist-desktop";

/// Everything the box shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct About {
    /// `alpymist-desktop`'s version, exactly as apk has it.
    pub version: Option<String>,
    /// The channel `/etc/apk/repositories` follows.
    pub channel: Option<Channel>,
    /// `/etc/alpine-release`.
    pub alpine: Option<String>,
    /// The running kernel.
    pub kernel: Option<String>,
    /// The desktop the session runs: Hyprland, labwc, i3.
    pub session: Option<String>,
    /// The machine's maker and model.
    pub computer: Option<String>,
    /// Memory, in words: `3.8 GiB`.
    pub memory: Option<String>,
    /// Every installed Alpymist package.
    pub packages: Vec<Package>,
}

/// An installed package, as apk's database has it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Package {
    /// The name: `alpymist-power-openrc`.
    pub name: String,
    /// The version: `0.0.6-r10`.
    pub version: String,
    /// The aport it was built from, apk's `o:`: a subpackage's origin is the
    /// package it was built beside, and everything else is its own origin.
    /// Absent — nothing but abuild leaves it out — the name stands in.
    pub origin: String,
}

/// How a version was built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Build {
    /// A released package: `0.0.6-r10`.
    Release {
        /// The version without its build number: `0.0.6`.
        version: String,
        /// The build number: the CI run that built it, `10` (ADR 0008).
        build: String,
    },
    /// A dev build from main: `0.0.6_git20260916203112-r10`.
    Dev {
        /// The release it comes after: `0.0.6`.
        after: String,
        /// The build number, as for a release.
        build: String,
        /// When the commit it was built from was made, in UTC:
        /// `2026-09-16 20:31`.
        committed: String,
    },
}

impl Build {
    /// Read an apk version. `None` when it is neither shape.
    #[must_use]
    pub fn of(version: &str) -> Option<Self> {
        let (base, build) = version.rsplit_once("-r")?;
        if let Some((after, stamp)) = base.split_once("_git") {
            let digits = stamp.as_bytes();
            if digits.len() != 14 || !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            let at = |a: usize, b: usize| &stamp[a..b];
            return Some(Self::Dev {
                after: after.to_owned(),
                build: build.to_owned(),
                committed: format!(
                    "{}-{}-{} {}:{}",
                    at(0, 4),
                    at(4, 6),
                    at(6, 8),
                    at(8, 10),
                    at(10, 12)
                ),
            });
        }
        (!base.is_empty() && !base.contains('_')).then(|| Self::Release {
            version: base.to_owned(),
            build: build.to_owned(),
        })
    }

    /// Read out for a person: the release this belongs to and the build it
    /// came from. A dev build says so in words rather than as the fourteen
    /// digits apk carries, which is what `0.0.6_git20260916203112-r10` is
    /// under the stamp.
    #[must_use]
    pub fn described(&self) -> String {
        match self {
            Self::Release { version, build } => format!("{version}, build {build}"),
            Self::Dev { after, build, .. } => format!("{after}, dev build {build}"),
        }
    }
}

impl About {
    /// Read the system under `root`, which is `/` but for tests.
    #[must_use]
    pub fn read(root: &Path) -> Self {
        let text = |path: &str| std::fs::read_to_string(root.join(path)).ok();
        let line = |path: &str| {
            text(path)
                .map(|t| t.trim().trim_end_matches('\0').to_owned())
                .filter(|t| !t.is_empty())
        };
        let packages = text("lib/apk/db/installed")
            .map(|db| alpymist_packages(&db))
            .unwrap_or_default();
        let version = packages
            .iter()
            .find(|p| p.name == DESKTOP)
            .map(|p| p.version.clone());
        let computer = {
            let vendor = line("sys/class/dmi/id/sys_vendor");
            let product = line("sys/class/dmi/id/product_name");
            match (vendor, product) {
                (Some(v), Some(p)) if p.starts_with(&v) => Some(p),
                (Some(v), Some(p)) => Some(format!("{v} {p}")),
                (v, p) => p.or(v).or_else(|| line("proc/device-tree/model")),
            }
        };
        Self {
            version,
            channel: text("etc/apk/repositories").and_then(|r| Channel::of_repositories(&r)),
            alpine: line("etc/alpine-release"),
            kernel: line("proc/sys/kernel/osrelease"),
            session: std::env::var("XDG_CURRENT_DESKTOP")
                .ok()
                .filter(|s| !s.is_empty()),
            computer,
            memory: text("proc/meminfo").and_then(|m| memory(&m)),
            packages,
        }
    }

    /// The headline: `Alpymist 0.0.1`, or `Alpymist 0.0.1, dev`.
    #[must_use]
    pub fn title(&self) -> String {
        match self.version.as_deref().and_then(Build::of) {
            Some(Build::Release { version, .. }) => format!("Alpymist {version}"),
            Some(Build::Dev { after, .. }) => format!("Alpymist {after}, dev"),
            None => "Alpymist".into(),
        }
    }

    /// The line under it: when a dev build's commit was made, or which
    /// package release this is.
    #[must_use]
    pub fn subtitle(&self) -> String {
        match (
            self.version.as_deref(),
            self.version.as_deref().and_then(Build::of),
        ) {
            (_, Some(Build::Dev { committed, .. })) => {
                format!("Built from main as of {committed} UTC")
            }
            (Some(v), _) => format!("Release {v}"),
            (None, _) => "No Alpymist desktop is installed".into(),
        }
    }

    /// The Version row: the release and the build it came from, spelled out
    /// by [`Build::described`]. The exact string apk holds is a line of the
    /// Packages list either way, and all of `text()`, so a bug report still
    /// carries it.
    #[must_use]
    pub fn version_row(&self) -> String {
        match (
            self.version.as_deref(),
            self.version.as_deref().and_then(Build::of),
        ) {
            (_, Some(build)) => build.described(),
            // A version of neither shape is shown as it is rather than hidden.
            (Some(v), None) => v.to_owned(),
            (None, None) => "unknown".to_owned(),
        }
    }

    /// Label and value rows about the system.
    #[must_use]
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let unknown = || "unknown".to_owned();
        vec![
            ("Version", self.version_row()),
            (
                "Channel",
                self.channel.map_or_else(unknown, |c| c.name().to_owned()),
            ),
            ("Alpine", self.alpine.clone().unwrap_or_else(unknown)),
            ("Kernel", self.kernel.clone().unwrap_or_else(unknown)),
            ("Desktop", self.session.clone().unwrap_or_else(unknown)),
            ("Computer", self.computer.clone().unwrap_or_else(unknown)),
            ("Memory", self.memory.clone().unwrap_or_else(unknown)),
        ]
    }

    /// The packages worth a line in the box: each one, less subpackages
    /// installed at the version of the package they were built beside
    /// (`alpymist-power-openrc` beside `alpymist-power`, the desktop's tiers
    /// beside `alpymist-desktop`), which would make the box taller than a
    /// small screen and say nothing new.
    ///
    /// Which is a subpackage is apk's `o:` and not the name, because since
    /// ADR 0008 every first-party package shares one version and one build
    /// number: by name and version alone, `alpymist-menu` looks like a
    /// subpackage of `alpymist`. `o:` also catches `alpymist-shell`, built
    /// beside `alpymist-desktop` under a name that shares no prefix with it.
    #[must_use]
    pub fn shown_packages(&self) -> Vec<&Package> {
        self.packages
            .iter()
            .filter(|p| {
                p.origin == p.name
                    // A subpackage left behind at a version of its own, by a
                    // half-applied upgrade, says something and stays.
                    || !self
                        .packages
                        .iter()
                        .any(|parent| parent.name == p.origin && parent.version == p.version)
            })
            .collect()
    }

    /// All of it as plain text: printed by `--print`, and what Copy puts on
    /// the clipboard for a bug report.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = format!("{}\n{}\n\n", self.title(), self.subtitle());
        let rows = self.rows();
        let wide = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        for (label, value) in rows {
            let _ = writeln!(out, "{label:wide$}  {value}");
        }
        if !self.packages.is_empty() {
            out.push_str("\nPackages\n");
            let wide = self
                .packages
                .iter()
                .map(|p| p.name.len())
                .max()
                .unwrap_or(0);
            for p in &self.packages {
                let _ = writeln!(out, "  {:wide$}  {}", p.name, p.version);
            }
        }
        out
    }

    /// A system to draw and test with. Every first-party package is at one
    /// version and one build number, which is what a system looks like since
    /// ADR 0008 and the case `shown_packages` has to get right.
    #[must_use]
    pub fn sample() -> Self {
        let v = "0.0.1_git20260915121112-r13";
        Self {
            version: Some(v.into()),
            channel: Some(Channel::Dev),
            alpine: Some("3.24.1".into()),
            kernel: Some("6.12.47-0-lts".into()),
            session: Some("Hyprland".into()),
            computer: Some("QEMU Virtual Machine".into()),
            memory: Some("3.8 GiB".into()),
            packages: [
                ("alpymist", v, "alpymist"),
                ("alpymist-auth", v, "alpymist-auth"),
                ("alpymist-desktop", v, "alpymist-desktop"),
                ("alpymist-desktop-full", v, "alpymist-desktop"),
                ("alpymist-keys", "2026-r1", "alpymist-keys"),
                ("alpymist-menu", v, "alpymist-menu"),
                ("alpymist-power", v, "alpymist-power"),
                ("alpymist-shell", v, "alpymist-desktop"),
                ("alpymist-store", v, "alpymist-store"),
                ("squint", "0.1.0_git20260914-r2", "squint"),
            ]
            .into_iter()
            .map(|(name, version, origin)| Package {
                name: name.to_owned(),
                version: version.to_owned(),
                origin: origin.to_owned(),
            })
            .collect(),
        }
    }
}

/// Installed Alpymist packages, sorted by name, from apk's database: records
/// separated by blank lines, `P:` the name, `V:` the version and `o:` the
/// aport it was built from.
fn alpymist_packages(db: &str) -> Vec<Package> {
    let mut out: Vec<Package> = db
        .split("\n\n")
        .filter_map(|record| {
            let field = |key: &str| record.lines().find_map(|l| l.strip_prefix(key));
            let name = field("P:")?;
            let ours = name.starts_with("alpymist") || name == "squint";
            if !ours {
                return None;
            }
            Some(Package {
                name: name.to_owned(),
                version: field("V:")?.to_owned(),
                origin: field("o:").unwrap_or(name).to_owned(),
            })
        })
        .collect();
    out.sort();
    out
}

/// `MemTotal` from `/proc/meminfo`, in GiB to one decimal.
fn memory(meminfo: &str) -> Option<String> {
    let kib: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    let tenths = kib * 10 / (1024 * 1024);
    Some(format!("{}.{} GiB", tenths / 10, tenths % 10))
}

#[cfg(test)]
mod tests {
    use super::{About, Build, Package, alpymist_packages, memory};
    use alpymist_core::Channel;

    #[test]
    fn a_dev_version_says_when_its_commit_was_made() {
        assert_eq!(
            Build::of("0.0.1_git20260915121112-r13"),
            Some(Build::Dev {
                after: "0.0.1".into(),
                build: "13".into(),
                committed: "2026-09-15 12:11".into()
            })
        );
    }

    #[test]
    fn a_release_is_its_version() {
        assert_eq!(
            Build::of("0.0.1-r13"),
            Some(Build::Release {
                version: "0.0.1".into(),
                build: "13".into()
            })
        );
        // An upstream snapshot is neither.
        assert_eq!(Build::of("0.1.0_git20260914-r2"), None);
        assert_eq!(Build::of("garbage"), None);
    }

    #[test]
    fn the_version_row_never_shows_the_dev_stamps_digits() {
        let row = |v: &str| {
            About {
                version: Some(v.to_owned()),
                ..About::default()
            }
            .version_row()
        };
        assert_eq!(row("0.0.6_git20260916203112-r10"), "0.0.6, dev build 10");
        assert_eq!(row("0.0.6-r10"), "0.0.6, build 10");
        // Neither shape, so it is passed through rather than lost.
        assert_eq!(row("2026-r1"), "2026, build 1");
        assert_eq!(row("garbage"), "garbage");
        assert_eq!(About::default().version_row(), "unknown");
    }

    #[test]
    fn only_alpymist_packages_are_listed() {
        let db = "C:Q1a=\nP:musl\nV:1.2.5-r10\no:musl\n\n\
                  C:Q1b=\nP:alpymist-menu\nV:0.0.1-r7\nA:aarch64\no:alpymist-menu\n\n\
                  C:Q1c=\nP:squint\nV:0.1.0_git20260914-r2\n\n\
                  C:Q1d=\nP:alpymist-shell\nV:0.0.1-r13\no:alpymist-desktop\n";
        let named = |name: &str, version: &str, origin: &str| Package {
            name: name.to_owned(),
            version: version.to_owned(),
            origin: origin.to_owned(),
        };
        assert_eq!(
            alpymist_packages(db),
            [
                named("alpymist-menu", "0.0.1-r7", "alpymist-menu"),
                // Built beside alpymist-desktop under a name that says nothing
                // about it.
                named("alpymist-shell", "0.0.1-r13", "alpymist-desktop"),
                // No `o:`, so it is its own origin.
                named("squint", "0.1.0_git20260914-r2", "squint"),
            ]
        );
    }

    #[test]
    fn memory_is_in_gibibytes() {
        assert_eq!(
            memory("MemTotal:        4010532 kB\nMemFree: 1 kB\n").as_deref(),
            Some("3.8 GiB")
        );
        assert_eq!(memory("nothing"), None);
    }

    #[test]
    fn a_whole_system_is_read_from_its_files() {
        let root = std::env::temp_dir().join(format!("alpymist-about-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        write(
            "lib/apk/db/installed",
            "P:alpymist-desktop\nV:0.0.1-r13\no:alpymist-desktop\n\n\
             P:alpymist\nV:0.0.1-r1\no:alpymist\n",
        );
        write("etc/apk/repositories", Channel::Stable.repository());
        write("etc/alpine-release", "3.24.1\n");
        write("proc/sys/kernel/osrelease", "6.12.47-0-lts\n");
        write("sys/class/dmi/id/sys_vendor", "LENOVO\n");
        write("sys/class/dmi/id/product_name", "ThinkPad X220\n");
        let about = About::read(&root);
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(about.title(), "Alpymist 0.0.1");
        assert_eq!(about.subtitle(), "Release 0.0.1-r13");
        assert_eq!(about.channel, Some(Channel::Stable));
        assert_eq!(about.computer.as_deref(), Some("LENOVO ThinkPad X220"));
        assert_eq!(about.packages.len(), 2);
        let text = about.text();
        assert!(text.contains("Channel   stable\n"), "{text}");
        assert!(text.contains("  alpymist          0.0.1-r1\n"), "{text}");
    }

    #[test]
    fn subpackages_at_their_parents_version_are_left_out_of_the_box() {
        let mut about = About::sample();
        about.packages.push(Package {
            name: "alpymist-power-openrc".into(),
            version: "0.0.1-r0".into(),
            origin: "alpymist-power".into(),
        });
        let shown: Vec<&str> = about
            .shown_packages()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert!(!shown.contains(&"alpymist-desktop-full"));
        // Built beside the desktop under an unrelated name: still a
        // subpackage, and the name alone would never have said so.
        assert!(!shown.contains(&"alpymist-shell"));
        assert!(shown.contains(&"alpymist-desktop"));
        // At another version it says something, so it stays.
        assert!(shown.contains(&"alpymist-power-openrc"));
        assert!(about.text().contains("alpymist-desktop-full"));
    }

    #[test]
    fn one_version_for_every_package_does_not_empty_the_box() {
        // Since ADR 0008 a system carries one version and one build number
        // across every first-party package, so each of these is a prefix of
        // the next at the very same version. None of them is a subpackage.
        let about = About::sample();
        let shown: Vec<&str> = about
            .shown_packages()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        for name in [
            "alpymist",
            "alpymist-auth",
            "alpymist-desktop",
            "alpymist-menu",
            "alpymist-power",
            "alpymist-store",
        ] {
            assert!(shown.contains(&name), "{name} is missing from {shown:?}");
        }
    }

    #[test]
    fn a_dev_sample_reads_as_dev() {
        let about = About::sample();
        assert_eq!(about.title(), "Alpymist 0.0.1, dev");
        assert_eq!(
            about.subtitle(),
            "Built from main as of 2026-09-15 12:11 UTC"
        );
    }
}

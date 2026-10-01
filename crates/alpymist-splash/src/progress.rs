//! How far the boot has got.
//!
//! The services the boot will start are the runlevels' — `sysinit`, `boot`
//! and `default`, and any runlevel stacked in one — read once, when the
//! splash starts. How many of those `OpenRC` has marked started is how far it
//! has got. Services started only as something's dependency are not counted
//! either way, so the fraction is of a known whole.
//!
//! The handover is near the end of `default`, so the services that come with
//! or after it are left out of the whole: the bar reads nearly full when the
//! next screen takes over rather than jumping from three quarters. It never
//! goes back, and never shows full until the splash is leaving.

use crate::handover::SUCCESSORS;
use crate::scene::Shown;
use std::collections::BTreeSet;
use std::path::Path;

/// The runlevels a boot goes through, in order.
pub const RUNLEVELS: [&str; 3] = ["sysinit", "boot", "default"];

/// Where `OpenRC` keeps them.
pub const RUNLEVEL_DIR: &str = "/etc/runlevels";

/// The service that takes the splash down, which starts with the handover.
const DONE: &str = "alpymist-splash-done";

/// The most the bar shows before the splash is leaving.
const MOST: f32 = 0.97;

/// The services in `RUNLEVELS` under `dir`, following stacked runlevels: a
/// directory inside a runlevel is another runlevel, whose services count too.
#[must_use]
pub fn expected(dir: &Path) -> BTreeSet<String> {
    let mut services = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut levels: Vec<String> = RUNLEVELS.iter().map(|l| (*l).to_owned()).collect();
    while let Some(level) = levels.pop() {
        if !seen.insert(level.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir.join(&level)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if entry.path().is_dir() {
                levels.push(name);
            } else {
                services.insert(name);
            }
        }
    }
    services
}

/// The services `OpenRC` has marked started, under `run` (`/run/openrc`).
#[must_use]
pub fn started(run: &Path) -> Vec<String> {
    std::fs::read_dir(run.join("started"))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The bar's state through a boot.
#[derive(Debug, Clone)]
pub struct Progress {
    /// What counts: the runlevels' services, less those of the handover.
    counted: BTreeSet<String>,
    /// The most shown so far.
    shown: f32,
    /// Steps of the waiting animation, when nothing is counted.
    step: u32,
}

impl Progress {
    /// Progress through a boot that starts `expected`.
    #[must_use]
    pub fn new(mut expected: BTreeSet<String>) -> Self {
        for late in SUCCESSORS.iter().chain([&DONE]) {
            expected.remove(*late);
        }
        Self {
            counted: expected,
            shown: 0.0,
            step: 0,
        }
    }

    /// What to show, now that `started` have started.
    pub fn update(&mut self, started: &[String]) -> Shown {
        if self.counted.is_empty() {
            self.step = self.step.wrapping_add(1);
            return Shown::Waiting(self.step);
        }
        let done = started.iter().filter(|s| self.counted.contains(*s)).count();
        // Counts of services, far inside f32's exact integers.
        #[allow(clippy::cast_precision_loss)]
        let fraction = (done as f32 / self.counted.len() as f32).min(MOST);
        self.shown = self.shown.max(fraction);
        Shown::Fraction(self.shown)
    }
}

#[cfg(test)]
mod tests {
    use super::{Progress, expected};
    use crate::scene::Shown;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_runlevels_are_read_with_the_ones_stacked_in_them() {
        let d = std::env::temp_dir().join(format!("alpymist-runlevels-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        for (level, service) in [
            ("sysinit", "devfs"),
            ("sysinit", "alpymist-splash"),
            ("boot", "localmount"),
            ("default", "greetd"),
            ("default", "sshd"),
            ("extra", "tailscale"),
            ("unused", "nothing"),
        ] {
            std::fs::create_dir_all(d.join(level)).unwrap();
            std::fs::write(d.join(level).join(service), "").unwrap();
        }
        // `default` stacks `extra`, which stacks `default` again.
        std::fs::create_dir_all(d.join("default/extra")).unwrap();
        std::fs::create_dir_all(d.join("extra/default")).unwrap();
        let found = expected(&d);
        let found: Vec<&str> = found.iter().map(String::as_str).collect();
        assert_eq!(
            found,
            [
                "alpymist-splash",
                "devfs",
                "greetd",
                "localmount",
                "sshd",
                "tailscale"
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_bar_only_moves_forward_and_never_fills_before_the_handover() {
        let mut p = Progress::new(
            names(&[
                "devfs",
                "localmount",
                "sshd",
                "iwd",
                "greetd",
                "alpymist-splash-done",
            ])
            .into_iter()
            .collect(),
        );
        assert_eq!(p.update(&names(&["devfs"])), Shown::Fraction(0.25));
        // A dependency nobody listed counts for nothing.
        assert_eq!(
            p.update(&names(&["devfs", "localmount", "udev"])),
            Shown::Fraction(0.5)
        );
        // A service that stops does not take the bar back.
        assert_eq!(p.update(&names(&["localmount"])), Shown::Fraction(0.5));
        let all = names(&["devfs", "localmount", "sshd", "iwd", "greetd"]);
        assert_eq!(p.update(&all), Shown::Fraction(0.97));
    }

    #[test]
    fn with_nothing_to_count_it_waits_visibly() {
        let mut p = Progress::new(std::collections::BTreeSet::new());
        assert_eq!(p.update(&[]), Shown::Waiting(1));
        assert_eq!(p.update(&[]), Shown::Waiting(2));
    }
}

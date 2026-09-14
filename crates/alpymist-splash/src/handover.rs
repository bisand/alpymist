//! When the splash should get out of the way.
//!
//! The splash starts early in boot and holds the display; whatever comes next
//! needs it. Nothing tells the splash that moment has come, so it watches for
//! it: `OpenRC` marks a service as starting in `/run/openrc/starting` before it
//! runs the service's script, and the installer and greetd are the two things
//! that take the screen over. When neither ever comes — a system whose desktop
//! did not install — the gettys that init starts once booting has finished are
//! the sign, and a time limit is the last resort.
//!
//! Pure over what it is shown, so every case is testable without booting.

use std::path::Path;
use std::time::Duration;

/// The services that draw on the screen next.
pub const SUCCESSORS: [&str; 2] = ["alpymist-install", "greetd"];

/// How long the splash stays at most, whatever happens.
pub const LIMIT: Duration = Duration::from_secs(90);

/// Why the splash is leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Something that draws on the screen is starting.
    Successor,
    /// Booting finished and a login prompt is up, with nothing drawing over it.
    Getty,
    /// It has been up too long.
    Limit,
    /// It was asked to stop.
    Signal,
}

impl Reason {
    /// Whether to blank the console on the way out.
    ///
    /// Blank for a successor, so the moment between the splash going and the
    /// next screen arriving shows black, not the boot messages the splash was
    /// hiding. Never for a getty: blanking would erase the login prompt it just
    /// printed and leave a black screen waiting for a key.
    #[must_use]
    pub fn blanks_console(self) -> bool {
        matches!(self, Self::Successor | Self::Signal)
    }
}

/// Whether to leave, given which services `OpenRC` has marked starting or
/// started, the names of the running processes, and how long it has been up.
#[must_use]
pub fn decide<'a>(
    services: &[String],
    processes: impl IntoIterator<Item = &'a str>,
    up: Duration,
) -> Option<Reason> {
    if services.iter().any(|s| SUCCESSORS.contains(&s.as_str())) {
        return Some(Reason::Successor);
    }
    if processes
        .into_iter()
        .any(|name| name == "getty" || name == "agetty")
    {
        return Some(Reason::Getty);
    }
    (up >= LIMIT).then_some(Reason::Limit)
}

/// The services `OpenRC` has marked starting or started.
#[must_use]
pub fn services(run: &Path) -> Vec<String> {
    ["starting", "started"]
        .iter()
        .filter_map(|state| std::fs::read_dir(run.join(state)).ok())
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// The names of the running processes.
#[must_use]
pub fn processes() -> Vec<String> {
    std::fs::read_dir("/proc")
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit()))
                })
                .filter_map(|entry| std::fs::read_to_string(entry.path().join("comm")).ok())
                .map(|comm| comm.trim().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{LIMIT, Reason, decide, services};
    use std::time::Duration;

    const EARLY: Duration = Duration::from_secs(3);

    #[test]
    fn the_splash_stays_while_the_machine_is_still_booting() {
        let services = vec!["udev".to_string(), "modloop".to_string()];
        assert_eq!(decide(&services, ["init", "udevd", "openrc"], EARLY), None);
    }

    #[test]
    fn the_installer_or_the_login_screen_starting_takes_over() {
        for successor in ["alpymist-install", "greetd"] {
            let services = vec!["dbus".to_string(), successor.to_string()];
            assert_eq!(
                decide(&services, ["openrc-run.sh"], EARLY),
                Some(Reason::Successor),
                "{successor}"
            );
        }
    }

    #[test]
    fn a_login_prompt_means_booting_finished_with_nothing_to_hand_over_to() {
        assert_eq!(
            decide(&[], ["init", "getty", "getty"], EARLY),
            Some(Reason::Getty)
        );
        assert!(
            !Reason::Getty.blanks_console(),
            "blanking would erase the prompt"
        );
        assert!(Reason::Successor.blanks_console());
    }

    #[test]
    fn it_never_stays_up_forever() {
        assert_eq!(decide(&[], ["init"], LIMIT), Some(Reason::Limit));
    }

    #[test]
    fn openrc_state_is_read_from_both_starting_and_started() {
        let run = std::env::temp_dir().join(format!("alpymist-splash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&run);
        std::fs::create_dir_all(run.join("starting")).unwrap();
        std::fs::create_dir_all(run.join("started")).unwrap();
        std::fs::write(run.join("starting").join("greetd"), "").unwrap();
        std::fs::write(run.join("started").join("udev"), "").unwrap();
        let mut found = services(&run);
        found.sort();
        assert_eq!(found, ["greetd", "udev"]);
        assert!(services(&run.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&run);
    }
}

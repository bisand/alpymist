//! Where software comes from.
//!
//! A [`Source`] reads its catalogue from what is already on disk, says what
//! is installed, and carries out installs, removals, updates and refreshes.
//! The store knows nothing of Flatpak or apk beyond this trait, so a new kind
//! of source is a new implementation and a new `kind` in the configuration.
//!
//! Everything here blocks: the store calls it from worker threads, one per
//! source, so a slow `apk update` never holds up a search in Flathub.

pub mod apk;
pub mod flatpak;

use crate::catalog::{Entry, Installed};
use crate::config::{self, Kind};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

/// Something to do to a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Install an entry.
    Install(String),
    /// Remove an entry.
    Remove(String),
    /// Update one entry, or everything.
    Update(Option<String>),
    /// Fetch the catalogue again.
    Refresh,
}

impl Op {
    /// What the status line says while it runs: "Installing GIMP".
    #[must_use]
    pub fn doing(&self, name: &str, source: &str) -> String {
        match self {
            Self::Install(_) => format!("Installing {name}"),
            Self::Remove(_) => format!("Removing {name}"),
            Self::Update(Some(_)) => format!("Updating {name}"),
            Self::Update(None) => format!("Updating everything from {source}"),
            Self::Refresh => format!("Refreshing {source}"),
        }
    }

    /// What the status line says once it is done: "GIMP installed".
    #[must_use]
    pub fn done(&self, name: &str, source: &str) -> String {
        match self {
            Self::Install(_) => format!("{name} installed"),
            Self::Remove(_) => format!("{name} removed"),
            Self::Update(Some(_)) => format!("{name} updated"),
            Self::Update(None) => format!("Everything from {source} is up to date"),
            Self::Refresh => format!("{source} refreshed"),
        }
    }

    /// The entry it concerns, if one.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Install(id) | Self::Remove(id) | Self::Update(Some(id)) => Some(id),
            Self::Update(None) | Self::Refresh => None,
        }
    }
}

/// A place software comes from.
pub trait Source: Send + Sync {
    /// Get ready to be read: add a missing remote. May use the network.
    ///
    /// # Errors
    /// What went wrong, for the status line.
    fn prepare(&self) -> Result<(), String> {
        Ok(())
    }

    /// Whether [`Source::load`] would find a catalogue on disk. A source
    /// without one is refreshed before it is loaded.
    fn has_catalog(&self) -> bool {
        true
    }

    /// Read the catalogue from disk. No network.
    ///
    /// # Errors
    /// What went wrong, for the status line.
    fn load(&self) -> Result<Vec<Entry>, String>;

    /// What is installed, with updates where the source knows of them.
    ///
    /// # Errors
    /// What went wrong, for the status line.
    fn installed(&self) -> Result<Vec<Installed>, String>;

    /// Carry out `op`, telling `progress` each step as it goes.
    ///
    /// # Errors
    /// What went wrong, for the status line.
    fn run(&self, op: &Op, progress: &mut dyn FnMut(&str)) -> Result<(), String>;

    /// The command that starts an installed entry, if it can be started.
    fn launch(&self, id: &str) -> Option<Command> {
        let _ = id;
        None
    }
}

/// The source a configuration describes.
#[must_use]
pub fn open(source: &config::Source) -> Box<dyn Source> {
    match &source.kind {
        Kind::Flatpak(settings) => Box::new(flatpak::Flatpak::new(settings.clone())),
        Kind::Apk(settings) => Box::new(apk::Apk::new(settings.clone())),
    }
}

/// Run a command to its end, passing each line it prints to `progress`.
///
/// # Errors
/// The command could not be started, or failed; the message is the last
/// thing it said on standard error, or its exit status.
pub fn run_command(mut command: Command, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C")
        .spawn()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    let stderr = child.stderr.take();
    // Standard error is read beside standard output, so neither pipe can
    // fill and stall the other; its last line is the error message.
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            // Progress bars redraw with carriage returns; the last frame is
            // the one to show.
            let line = line.rsplit('\r').next().unwrap_or("").trim();
            if !line.is_empty() {
                progress(line);
            }
        }
    }
    let status = child.wait().map_err(|e| format!("{program}: {e}"))?;
    let errors = errors.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let last = errors
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or("").trim())
        .rfind(|l| !l.is_empty())
        .map(str::to_owned);
    Err(last.unwrap_or_else(|| format!("{program} failed ({status})")))
}

/// Run a command and return what it printed.
///
/// # Errors
/// As [`run_command`].
pub fn output(mut command: Command) -> Result<String, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let out = command
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let errors = String::from_utf8_lossy(&out.stderr);
        Err(errors.lines().rfind(|l| !l.trim().is_empty()).map_or_else(
            || format!("{program} failed ({})", out.status),
            str::to_owned,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Op, output, run_command};
    use std::process::Command;

    #[test]
    fn progress_comes_a_line_at_a_time_and_errors_carry_the_last_word() {
        let mut command = Command::new("sh");
        command.args(["-c", "echo one; printf 'a\\rtwo\\n'; echo oops >&2; exit 3"]);
        let mut lines = Vec::new();
        let result = run_command(command, &mut |l| lines.push(l.to_owned()));
        assert_eq!(lines, ["one", "two"]);
        assert_eq!(result.unwrap_err(), "oops");
        let mut ok = Command::new("sh");
        ok.args(["-c", "echo fine"]);
        assert_eq!(output(ok).unwrap(), "fine\n");
    }

    #[test]
    fn operations_say_what_they_do() {
        let op = Op::Install("org.gimp.GIMP".into());
        assert_eq!(op.doing("GIMP", "Flathub"), "Installing GIMP");
        assert_eq!(op.done("GIMP", "Flathub"), "GIMP installed");
        assert_eq!(op.target(), Some("org.gimp.GIMP"));
        assert_eq!(Op::Refresh.target(), None);
    }
}

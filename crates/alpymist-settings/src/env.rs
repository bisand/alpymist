//! Where the settings are read and written, and how programs are run.
//!
//! Everything goes through an [`Env`], so tests point it at a directory and
//! record the commands instead of running them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Runs a program and returns what it printed.
pub type Runner = dyn Fn(&[&str]) -> Result<String, String> + Send + Sync;

/// The system and account a setting is read and written for.
pub struct Env {
    /// The filesystem root: `/`, or a test directory.
    pub root: PathBuf,
    /// The account's configuration directory: `~/.config`.
    pub config: PathBuf,
    /// Whether this process is root.
    pub is_root: bool,
    /// Whether a Hyprland session can be spoken to.
    pub hyprland: bool,
    run: Box<Runner>,
}

impl Env {
    /// The real system, for this process's account.
    #[must_use]
    pub fn detect() -> Self {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("/nonexistent/.config"));
        Self {
            root: PathBuf::from("/"),
            config,
            is_root: uid() == Some(0),
            hyprland: std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
                .is_some_and(|s| !s.is_empty()),
            run: Box::new(run),
        }
    }

    /// A test environment under `dir`: its `/` at `dir/root`, the account's
    /// configuration at `dir/config`, and commands recorded in `ran` rather
    /// than run.
    #[must_use]
    pub fn test(dir: &Path, is_root: bool, ran: &'static Mutex<Vec<String>>) -> Self {
        Self {
            root: dir.join("root"),
            config: dir.join("config"),
            is_root,
            hyprland: true,
            run: Box::new(move |argv| {
                ran.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(argv.join(" "));
                Ok(String::new())
            }),
        }
    }

    /// A path under the system root: `etc/alpymist/settings.toml`.
    #[must_use]
    pub fn system(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// A path under the account's configuration: `alpymist/settings.toml`.
    #[must_use]
    pub fn account(&self, relative: &str) -> PathBuf {
        self.config.join(relative)
    }

    /// Run a program.
    ///
    /// # Errors
    /// It could not be started or said it failed; its message.
    pub fn run(&self, argv: &[&str]) -> Result<String, String> {
        (self.run)(argv)
    }
}

/// This process's user id, from `/proc`.
fn uid() -> Option<u32> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn run(argv: &[&str]) -> Result<String, String> {
    let (program, rest) = argv.split_first().ok_or("nothing to run")?;
    // alpymist-power's helper is reached through pkexec, or its password
    // dialog, exactly as the power popup reaches it; root runs it directly.
    if *program == alpymist_power::actions::HELPER && uid() != Some(0) {
        return alpymist_power::actions::helper(rest).map(|()| String::new());
    }
    let output = Command::new(program)
        .args(rest)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        Err(if said.is_empty() {
            format!("{program} failed ({})", output.status)
        } else {
            format!("{program}: {said}")
        })
    }
}

//! Running what was chosen.
//!
//! The menu is gone the moment something is chosen, so whatever it starts
//! must not depend on it: no inherited stdin or stdout, and a process group of
//! its own, so neither a terminal's hangup nor the menu's exit reaches it.
//! Nothing is waited for. The kernel reparents the child to init when the menu
//! exits, and init reaps it.

use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Something to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A line of shell, from the menu's configuration.
    Shell {
        /// Handed to `/bin/sh -c`.
        command: String,
        /// Run inside a terminal.
        terminal: bool,
        /// Inside a terminal, wait for Enter before it closes.
        hold: bool,
    },
    /// An argument vector, from a desktop entry. No shell sees it.
    Argv {
        /// Program and arguments.
        argv: Vec<String>,
        /// Run inside a terminal.
        terminal: bool,
    },
}

/// What a held terminal prints before it waits.
const HOLD: &str = "; printf '\\n\\033[2mPress Enter to close\\033[0m'; read -r _";

impl Launch {
    /// The full argument vector, with the terminal in front where one is wanted.
    #[must_use]
    pub fn argv(&self, terminal: &[String]) -> Vec<String> {
        match self {
            Self::Shell {
                command,
                terminal: false,
                ..
            } => vec!["/bin/sh".into(), "-c".into(), command.clone()],
            Self::Shell {
                command,
                terminal: true,
                hold,
            } => {
                let script = if *hold {
                    format!("{command}{HOLD}")
                } else {
                    command.clone()
                };
                let mut argv = terminal.to_vec();
                argv.extend(["/bin/sh".into(), "-c".into(), script]);
                argv
            }
            Self::Argv {
                argv,
                terminal: false,
            } => argv.clone(),
            Self::Argv {
                argv: inner,
                terminal: true,
            } => {
                let mut argv = terminal.to_vec();
                argv.extend(inner.iter().cloned());
                argv
            }
        }
    }

    /// Start it, and do not wait.
    ///
    /// # Errors
    /// When the program could not be started at all — missing, not executable.
    /// What it does once started is its own business.
    pub fn spawn(&self, terminal: &[String]) -> io::Result<()> {
        let argv = self.argv(terminal);
        let (program, rest) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "nothing to run"))?;
        let mut command = Command::new(program);
        command
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        if let Some(home) = std::env::var_os("HOME") {
            command.current_dir(home);
        }
        command.spawn().map(drop)
    }
}

/// How to run a command in a terminal, when the configuration does not say.
///
/// `$TERMINAL` if set, then Ghostty, then foot. Ghostty wants `-e` before the
/// command; foot takes the command as its trailing arguments. Anything else in
/// `$TERMINAL` is assumed to follow the xterm convention, which almost all do.
#[must_use]
pub fn default_terminal() -> Vec<String> {
    let path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let on_path = |name: &str| path.iter().any(|d| d.join(name).is_file());
    let chosen = std::env::var("TERMINAL")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            ["ghostty", "foot"]
                .into_iter()
                .find(|t| on_path(t))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "foot".into());
    terminal_for(&chosen)
}

/// The argument prefix for a named terminal.
fn terminal_for(name: &str) -> Vec<String> {
    let base = name.rsplit('/').next().unwrap_or(name);
    match base {
        "foot" | "footclient" => vec![name.into()],
        _ => vec![name.into(), "-e".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::{Launch, terminal_for};

    fn term() -> Vec<String> {
        vec!["ghostty".into(), "-e".into()]
    }

    #[test]
    fn a_shell_line_goes_through_sh() {
        let l = Launch::Shell {
            command: "echo $HOME | wc".into(),
            terminal: false,
            hold: false,
        };
        assert_eq!(l.argv(&term()), ["/bin/sh", "-c", "echo $HOME | wc"]);
    }

    #[test]
    fn a_terminal_command_is_wrapped_and_can_hold() {
        let l = Launch::Shell {
            command: "apk upgrade".into(),
            terminal: true,
            hold: true,
        };
        let argv = l.argv(&term());
        assert_eq!(&argv[..4], ["ghostty", "-e", "/bin/sh", "-c"]);
        assert!(argv[4].starts_with("apk upgrade; "));
        assert!(argv[4].ends_with("read -r _"));
    }

    #[test]
    fn a_desktop_entry_is_run_without_a_shell() {
        let l = Launch::Argv {
            argv: vec!["btop".into()],
            terminal: true,
        };
        assert_eq!(l.argv(&term()), ["ghostty", "-e", "btop"]);
        let l = Launch::Argv {
            argv: vec!["librewolf".into(), "--new-window".into()],
            terminal: false,
        };
        assert_eq!(l.argv(&term()), ["librewolf", "--new-window"]);
    }

    #[test]
    fn foot_takes_its_command_without_a_flag() {
        assert_eq!(terminal_for("foot"), ["foot"]);
        assert_eq!(terminal_for("/usr/bin/foot"), ["/usr/bin/foot"]);
        assert_eq!(terminal_for("kitty"), ["kitty", "-e"]);
    }

    #[test]
    fn spawning_something_missing_is_an_error_not_a_panic() {
        let l = Launch::Argv {
            argv: vec!["/no/such/program".into()],
            terminal: false,
        };
        assert!(l.spawn(&[]).is_err());
    }
}

//! `alpymist` — the single entry point for configuring an Alpymist system.
//!
//! Formerly `alpymistctl` (ADR 0007). The package provides that name, so a
//! system that installed it by the old name upgrades to this one.

#![forbid(unsafe_code)]

mod channel;
mod firmware;
mod root;
mod settings;

use alpymist_core::Channel;
use alpymist_settings::{Env, Settings};
use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "alpymist",
    version,
    about = "Configure and inspect an Alpymist system",
    after_help = "Every setting the Settings app shows is here too: `alpymist list` to see them."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List settings, their values and titles; with a setting's id, its
    /// choices too.
    List {
        /// An area (`touchpad`), a setting (`touchpad.natural-scroll`), or
        /// nothing for every setting.
        filter: Option<String>,
        /// Print JSON: ids, titles, kinds, choices, scopes and values.
        #[arg(long)]
        json: bool,
    },
    /// Print a setting's value.
    Get {
        /// The setting, such as `keyboard.layout`.
        id: String,
        /// Print it as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Change a setting. A system setting asks for an administrator's
    /// password.
    Set {
        /// The setting, such as `touchpad.natural-scroll`.
        id: String,
        /// Its new value: `on`, `off`, a number, or one of its choices.
        ///
        /// Left out for a setting that is a thing to do rather than a thing to
        /// be — `alpymist set screensaver-mountains.preview` — which has no
        /// value to give it.
        #[arg(allow_hyphen_values = true)]
        value: Option<String>,
        /// Replace a generated file even if it was edited by hand.
        #[arg(long)]
        force: bool,
        /// Only write the files; do not tell the running session.
        #[arg(long)]
        no_live: bool,
    },
    /// Put a setting back to its default.
    Reset {
        /// The setting.
        id: String,
        /// Replace a generated file even if it was edited by hand.
        #[arg(long)]
        force: bool,
        /// Only write the files; do not tell the running session.
        #[arg(long)]
        no_live: bool,
    },
    /// Show or change the release channel: stable, or dev for every push to
    /// main. Changing it asks for a password and upgrades to the new channel.
    Channel {
        /// The channel to follow. Without it, print the current one.
        channel: Option<Channel>,
        /// Only switch; leave the upgrade for later.
        #[arg(long)]
        no_upgrade: bool,
    },
    /// Inspect the machine and report which desktop tier it can run.
    Probe {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
    /// Start a desktop session: prepare the files it reads, then run it.
    /// greetd's configuration runs this.
    Session {
        /// hyprland or labwc.
        desktop: Desktop,
        /// Passed on to the compositor.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Put the chosen wallpaper on the screen, replacing whatever is there.
    /// Each desktop runs this at login, and Settings after a change;
    /// `alpymist set appearance.wallpaper` chooses the picture.
    Wallpaper,
    /// Install firmware that a driver asked for and did not find. As root;
    /// the alpymist-firmware service runs `watch` at boot.
    Firmware {
        #[command(subcommand)]
        action: FirmwareAction,
    },
    /// Print the menu fragment for every setting, for packaging.
    #[command(hide = true)]
    MenuFragment,
}

#[derive(Subcommand)]
enum FirmwareAction {
    /// Look through the kernel log so far, install what is missing, and stop.
    Check,
    /// Follow the kernel log, installing what is missing as drivers ask.
    Watch,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Human-readable summary.
    Human,
    /// Machine-readable JSON, for the installer and first-boot service.
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum Desktop {
    /// Hyprland, through its `start-hyprland` launcher.
    Hyprland,
    /// labwc.
    Labwc,
    /// Prepare the files and start nothing: for package scripts, as the
    /// account whose files they are.
    Prepare,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let env = Env::detect();
    let all = Settings::new();
    match &cli.command {
        Command::List { filter, json } => settings::list(&all, &env, filter.as_deref(), *json),
        Command::Get { id, json } => settings::get(&all, &env, id, *json),
        Command::Set {
            id,
            value,
            force,
            no_live,
        } => match value {
            Some(value) => settings::change(&all, &env, id, Some(value), *force, !no_live),
            None => settings::act(&all, &env, id, *force, !no_live),
        },
        Command::Reset { id, force, no_live } => {
            settings::change(&all, &env, id, None, *force, !no_live)
        }
        Command::Channel {
            channel: None,
            no_upgrade: _,
        } => channel::show(&all, &env),
        Command::Channel {
            channel: Some(to),
            no_upgrade,
        } => channel::switch(&all, &env, *to, !no_upgrade),
        Command::Probe { format } => Ok(probe(*format)?),
        Command::Session { desktop, args } => session(&all, &env, *desktop, args),
        Command::Wallpaper => Ok(alpymist_settings::wallpaper::show(&env)?),
        Command::Firmware {
            action: FirmwareAction::Check,
        } => firmware::check(),
        Command::Firmware {
            action: FirmwareAction::Watch,
        } => firmware::watch(),
        Command::MenuFragment => {
            print!("{}", alpymist_settings::menu::fragment(&all));
            Ok(())
        }
    }
}

/// Prepare what the compositor reads, then become it. Whatever goes wrong
/// preparing, the session still starts: a login that never arrives is worse
/// than a setting not applied.
fn session(
    all: &Settings,
    env: &Env,
    desktop: Desktop,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = all.prepare_session(env) {
        eprintln!("alpymist session: {e}");
    }
    let program = match desktop {
        Desktop::Hyprland => "start-hyprland",
        Desktop::Labwc => "labwc",
        Desktop::Prepare => return Ok(()),
    };
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(log) = session_log()
        && let Ok(err) = log.try_clone()
    {
        command.stdout(log).stderr(err);
    }
    let e = command.exec();
    Err(format!("{program}: {e}").into())
}

/// Where the compositor's own output goes: `session.log` in
/// `$XDG_STATE_HOME/alpymist`, or `~/.local/state/alpymist`.
fn session_log_path(state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let state = state_home
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("alpymist/session.log"))
}

/// The log of this login's desktop, the previous one kept beside it as
/// `session.log.old`. A compositor that cannot start says why on its output
/// and exits, and without this that went to the console behind the login
/// screen, where it was gone before anyone could read it; the login screen
/// now says the desktop stopped, and a text console login shows the end of
/// this. `None` if it cannot be made, and the desktop starts anyway.
fn session_log() -> Option<std::fs::File> {
    let path = session_log_path(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let _ = std::fs::rename(&path, path.with_extension("log.old"));
    std::fs::File::create(&path).ok()
}

fn probe(format: Format) -> alpymist_core::Result<()> {
    let caps = alpymist_hwprobe::probe()?;
    let rationale = alpymist_core::select_tier(&caps);

    match format {
        Format::Json => {
            let doc = serde_json::json!({
                "capabilities": caps,
                "tier": rationale.tier,
                "backend": rationale.tier.backend(),
                "metapackage": rationale.tier.metapackage(),
                "reasons": rationale.reasons,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&doc).expect("serialisable")
            );
        }
        Format::Human => {
            println!("tier:        {:?}", rationale.tier);
            println!("backend:     {:?}", rationale.tier.backend());
            println!("metapackage: {}", rationale.tier.metapackage());
            println!("memory:      {} MiB", caps.memory_mib);
            println!("cpus:        {}", caps.cpus);
            match &caps.gles {
                Some(g) => println!(
                    "gles:        {}.{} — {} ({}){}",
                    g.version.0,
                    g.version.1,
                    g.renderer,
                    g.vendor,
                    if g.is_software() { " [software]" } else { "" }
                ),
                None => println!(
                    "gles:        not detected ({})",
                    caps.gles_error.as_deref().unwrap_or("not probed")
                ),
            }
            println!("virt:        {:?}", caps.virtualisation);
            println!("why:");
            for reason in &rationale.reasons {
                println!("  - {reason}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::session_log_path;
    use std::path::PathBuf;

    #[test]
    fn the_session_log_is_in_the_accounts_state_directory() {
        assert_eq!(
            session_log_path(None, Some("/home/andre".into())),
            Some(PathBuf::from(
                "/home/andre/.local/state/alpymist/session.log"
            ))
        );
        assert_eq!(
            session_log_path(Some("/srv/state".into()), Some("/home/andre".into())),
            Some(PathBuf::from("/srv/state/alpymist/session.log"))
        );
        assert_eq!(
            session_log_path(Some("".into()), Some("/home/andre".into())),
            Some(PathBuf::from(
                "/home/andre/.local/state/alpymist/session.log"
            )),
            "an empty XDG_STATE_HOME is unset"
        );
        assert_eq!(session_log_path(None, None), None);
    }
}

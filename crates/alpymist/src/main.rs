//! `alpymist` — the single entry point for configuring an Alpymist system.
//!
//! Formerly `alpymistctl`, which the transitional package of that name links
//! here for one release (ADR 0007).

#![forbid(unsafe_code)]

mod channel;
mod root;
mod settings;

use alpymist_core::Channel;
use alpymist_settings::{Env, Settings};
use clap::{Parser, Subcommand, ValueEnum};
use std::os::unix::process::CommandExt as _;

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
        value: String,
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
    /// Print the menu fragment for every setting, for packaging.
    #[command(hide = true)]
    MenuFragment,
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
        } => settings::change(&all, &env, id, Some(value), *force, !no_live),
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
    let e = std::process::Command::new(program).args(args).exec();
    Err(format!("{program}: {e}").into())
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

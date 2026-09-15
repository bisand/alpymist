//! `alpymist` — the single entry point for configuring an Alpymist system.
//!
//! Formerly `alpymistctl`, which the transitional package of that name links
//! here for one release (ADR 0007).

#![forbid(unsafe_code)]

mod channel;

use alpymist_core::Channel;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "alpymist",
    version,
    about = "Configure and inspect an Alpymist system"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect the machine and report which desktop tier it can run.
    Probe {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
    /// Show or change the release channel: stable, or dev for every push to
    /// main. Changing it needs root, and upgrades to the new channel.
    Channel {
        /// The channel to follow. Without it, print the current one.
        channel: Option<Channel>,
        /// Only switch; leave the upgrade for later.
        #[arg(long)]
        no_upgrade: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Human-readable summary.
    Human,
    /// Machine-readable JSON, for the installer and first-boot service.
    Json,
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
    match cli.command {
        Command::Probe { format } => Ok(probe(format)?),
        Command::Channel {
            channel: None,
            no_upgrade: _,
        } => channel::show(),
        Command::Channel {
            channel: Some(to),
            no_upgrade,
        } => channel::switch(to, !no_upgrade),
    }
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

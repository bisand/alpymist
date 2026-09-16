//! Build and test orchestration for Alpymist.
//!
//! Everything that would otherwise be a shell script lives here. The two things
//! that stay shell are the `APKBUILD` and the `mkimage` profile, because
//! Alpine's build system sources them as shell and offers no other interface.

#![forbid(unsafe_code)]

mod installer_data;
mod publish;
mod qemu;
mod serial;
mod version;

use alpymist_core::Channel;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "xtask", about = "Build and test orchestration for Alpymist")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Boot an image in QEMU and assert that it reports a desktop tier.
    Smoke {
        /// Path to the ISO to boot.
        #[arg(long)]
        iso: PathBuf,
        /// Architecture the ISO was built for.
        #[arg(long, value_enum, default_value_t = Arch::Aarch64)]
        arch: Arch,
        /// Give up after this many seconds.
        #[arg(long, default_value_t = 180)]
        timeout: u64,
        /// Fail unless the image reports this tier. Use in CI to catch a
        /// regression that silently downgrades every machine.
        #[arg(long)]
        expect_tier: Option<String>,
        /// Print the whole serial log, not just the probe report.
        #[arg(long)]
        verbose: bool,
    },
    /// Sign and publish a channel's packages: stable to pkgs.alpymist.org,
    /// dev to dev.pkgs.alpymist.org.
    ///
    /// Signs the repository index with the channel's key, verifies it, and
    /// with --push replaces the Pages site. Stable's key stays on a
    /// maintainer's machine; dev's is CI's (ADR 0006).
    Publish {
        /// The channel to publish to.
        #[arg(long, default_value = "stable", value_parser = parse_channel)]
        channel: Channel,
        /// The workflow run to publish: Release for stable, Dev for dev
        /// (`gh run list -w Release`).
        #[arg(long, required_unless_present = "packages")]
        run: Option<String>,
        /// Packages already downloaded, one packages-<arch> directory per
        /// architecture, instead of a run's. Dev only; CI publishes this way.
        #[arg(long, conflicts_with = "run", requires = "commit")]
        packages: Option<PathBuf>,
        /// The commit of main those packages were built from.
        #[arg(long, requires = "packages")]
        commit: Option<String>,
        /// The channel's signing key. By default,
        /// ~/.config/alpymist/keys/<its name>.
        #[arg(long)]
        key: Option<PathBuf>,
        /// Push the signed site. Without it, stop after verifying.
        #[arg(long)]
        push: bool,
    },
    /// Set, or check, the version every first-party package is built with.
    ///
    /// Given a version, it writes that one everywhere and starts pkgrel again
    /// at 0. Given none, it checks: Cargo.toml's workspace version and every
    /// aports/*/APKBUILD pkgver have to agree, which is what CI runs
    /// (ADR 0008).
    Version {
        /// Write this version everywhere, as `0.0.5`.
        #[arg(value_name = "VERSION")]
        set: Option<String>,
        /// Also require the workspace version to be this release tag, with or
        /// without its leading `v`. What a Release run checks before building.
        #[arg(long, value_name = "TAG")]
        expect: Option<String>,
        /// The workspace to work on.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Regenerate the installer's keyboard layout and time zone lists.
    ///
    /// Run inside the Alpine builder with kbd-bkeymaps and tzdata installed.
    InstallerData {
        /// Where kbd-bkeymaps keeps its keymaps.
        #[arg(long, default_value = "/usr/share/bkeymaps")]
        bkeymaps: PathBuf,
        /// Where tzdata keeps zone.tab and iso3166.tab.
        #[arg(long, default_value = "/usr/share/zoneinfo")]
        zoneinfo: PathBuf,
        /// Directory to write keymaps.tsv and timezones.tsv into.
        #[arg(long, default_value = "crates/alpymist-core/data")]
        out: PathBuf,
    },
}

/// Architectures we build images for.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Arch {
    /// 64-bit ARM.
    #[value(name = "aarch64")]
    Aarch64,
    /// 64-bit x86.
    #[value(name = "x86_64")]
    X86_64,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Smoke {
            iso,
            arch,
            timeout,
            expect_tier,
            verbose,
        } => smoke(
            &iso,
            arch,
            Duration::from_secs(timeout),
            expect_tier.as_deref(),
            verbose,
        ),
        Command::Publish {
            channel,
            run,
            packages,
            commit,
            key,
            push,
        } => {
            let packages = match (run, packages, commit) {
                (Some(run), _, _) => publish::Packages::Run(run),
                (None, Some(dir), Some(commit)) => publish::Packages::Downloaded { dir, commit },
                _ => bail!("give --run, or --packages with --commit"),
            };
            let key = key.unwrap_or_else(|| publish::default_key(channel));
            publish::publish(channel, &packages, &key, push)
        }
        Command::Version { set, expect, root } => {
            version::version(&root, set.as_deref(), expect.as_deref())
        }
        Command::InstallerData {
            bkeymaps,
            zoneinfo,
            out,
        } => installer_data::generate(&bkeymaps, &zoneinfo, &out),
    }
}

fn parse_channel(s: &str) -> Result<Channel, String> {
    s.parse()
}

/// Boot `iso` and check that the first-boot probe reported a tier.
fn smoke(
    iso: &std::path::Path,
    arch: Arch,
    timeout: Duration,
    expect_tier: Option<&str>,
    verbose: bool,
) -> Result<()> {
    if !iso.exists() {
        bail!("no such image: {}", iso.display());
    }
    println!(
        "booting {} on {arch:?}, timeout {}s",
        iso.display(),
        timeout.as_secs()
    );

    let log = qemu::boot_and_capture(iso, arch, timeout, serial::END).context("running QEMU")?;

    if verbose {
        for line in &log.lines {
            println!("| {line}");
        }
    }

    let Some(report) = serial::extract_report(&log.lines) else {
        // Show the tail regardless: when a boot fails this is the only evidence.
        eprintln!("--- last 40 lines of serial output ---");
        for line in log.lines.iter().rev().take(40).rev() {
            eprintln!("| {line}");
        }
        bail!(
            "image booted for {}s but never printed a probe report{}; full log: {}",
            log.elapsed.as_secs(),
            if log.timed_out { " (timed out)" } else { "" },
            log.log_path.display()
        );
    };

    println!("--- probe report from the booted image ---");
    for line in &report {
        println!("| {line}");
    }

    if let Err(missing) = serial::validate_report(&report) {
        bail!("probe report is missing: {}", missing.join(", "));
    }

    if let Some(expected) = expect_tier {
        let actual = serial::reported_tier(&report).unwrap_or_default();
        if actual != expected {
            bail!("expected tier {expected}, image reported {actual}");
        }
        println!("tier is {actual}, as expected");
    }

    println!("\nsmoke test passed in {}s", log.elapsed.as_secs());
    Ok(())
}

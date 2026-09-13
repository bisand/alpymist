//! Build and test orchestration for Alpymist.
//!
//! Everything that would otherwise be a shell script lives here. The two things
//! that stay shell are the `APKBUILD` and the `mkimage` profile, because
//! Alpine's build system sources them as shell and offers no other interface.

#![forbid(unsafe_code)]

mod ghostty;
mod installer_data;
mod qemu;
mod serial;

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
    /// Check the ghostty backport against Alpine's aport.
    ///
    /// Ghostty is not in a stable Alpine branch, so we carry a copy of their
    /// testing aport. This says when theirs has moved.
    GhosttyCheck {
        /// Our copy.
        #[arg(long, default_value = "aports/ghostty/APKBUILD")]
        apkbuild: PathBuf,
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
        #[arg(long, default_value = "crates/alpymist-install/data")]
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
        Command::GhosttyCheck { apkbuild } => ghostty::check(&apkbuild),
        Command::InstallerData {
            bkeymaps,
            zoneinfo,
            out,
        } => installer_data::generate(&bkeymaps, &zoneinfo, &out),
    }
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

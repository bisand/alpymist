//! Watching the Ghostty backport for drift.
//!
//! Ghostty is packaged in Alpine's edge/testing but not in a stable branch, so
//! `aports/ghostty/APKBUILD` is a copy of theirs, pinned to an upstream commit
//! and checksum. A copy nobody looks at goes stale, and a stale terminal is a
//! terminal with unfixed bugs in it — so the nightly image build runs this,
//! and fails when Alpine's version has moved past ours.
//!
//! What it does not do is update anything. Pulling a new commit means building
//! and booting it; that is a decision, not a cron job.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Alpine's own aport, the one ours is copied from.
const UPSTREAM: &str =
    "https://raw.githubusercontent.com/alpinelinux/aports/master/testing/ghostty/APKBUILD";

/// The fields that decide what gets built.
#[derive(Debug, PartialEq, Eq)]
struct Pin {
    version: String,
    commit: String,
    checksum: String,
}

/// Compare our backport against Alpine's.
///
/// # Errors
/// Fails when the two differ, or when either cannot be read.
pub fn check(ours: &Path) -> Result<()> {
    let mine =
        pin(&std::fs::read_to_string(ours)
            .with_context(|| format!("reading {}", ours.display()))?)?;
    let theirs = pin(&fetch(UPSTREAM)?)?;

    if mine == theirs {
        println!("ghostty {} matches Alpine's aport", mine.version);
        return Ok(());
    }
    bail!(
        "Alpine's ghostty aport has moved on:\n  \
         ours:   {} (commit {}…)\n  \
         theirs: {} (commit {}…)\n\n\
         Copy https://gitlab.alpinelinux.org/alpine/aports/-/blob/master/testing/ghostty/APKBUILD \
         into aports/ghostty/APKBUILD, keeping our header, then build and boot it before pushing.\n\
         When ghostty reaches a stable Alpine branch, delete the backport and depend on the \
         package instead.",
        mine.version,
        short(&mine.commit),
        theirs.version,
        short(&theirs.commit),
    );
}

fn short(commit: &str) -> String {
    commit.chars().take(12).collect()
}

/// Pull the version, commit and checksum out of an APKBUILD.
fn pin(text: &str) -> Result<Pin> {
    let field = |name: &str| -> Option<String> {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(&format!("{name}=")))
            .map(|v| v.trim_matches(['"', '\'']).to_string())
    };
    let version = field("pkgver").context("no pkgver")?;
    let commit = field("_commit").context("no _commit")?;
    // The checksum block is `sha512sums="<hash>  <file>"` across lines.
    let checksum = text
        .split_once("sha512sums=")
        .and_then(|(_, rest)| rest.split('"').nth(1))
        .map(|block| {
            block
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .filter(|c| !c.is_empty())
        .context("no sha512sums")?;
    Ok(Pin {
        version,
        commit,
        checksum,
    })
}

/// Fetch a URL as text.
///
/// Through curl rather than an HTTP crate: this runs in CI next to a dozen
/// other command-line tools, and one fewer dependency tree to audit is worth
/// more here than the tidiness of doing it in-process.
fn fetch(url: &str) -> Result<String> {
    let out = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", url])
        .output()
        .with_context(|| format!("running curl for {url}"))?;
    if !out.status.success() {
        bail!(
            "fetching {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::pin;

    const SAMPLE: &str = r#"
pkgname=ghostty
pkgver=1.3.1_git20260908
_commit=b0c421fcd2e290629d4285c181b52fe2f2095f06
source="https://example.invalid/ghostty-$_commit.tar.gz"
sha512sums="
50aefee3758d5601  ghostty-b0c421.tar.gz
"
"#;

    #[test]
    fn it_reads_what_decides_the_build() {
        let p = pin(SAMPLE).unwrap();
        assert_eq!(p.version, "1.3.1_git20260908");
        assert_eq!(p.commit, "b0c421fcd2e290629d4285c181b52fe2f2095f06");
        assert_eq!(p.checksum, "50aefee3758d5601");
    }

    #[test]
    fn a_different_commit_is_a_different_pin() {
        let other = SAMPLE.replace("b0c421fcd2e290629d4285c181b52fe2f2095f06", "deadbeef");
        assert_ne!(pin(SAMPLE).unwrap(), pin(&other).unwrap());
    }

    #[test]
    fn an_apkbuild_without_the_fields_is_an_error() {
        assert!(pin("pkgname=ghostty\n").is_err());
    }
}

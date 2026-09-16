//! The one version every first-party package is built with.
//!
//! `Cargo.toml`'s `[workspace.package] version` is where it is written down;
//! each `aports/*/APKBUILD` repeats it as `pkgver`, because abuild sources the
//! APKBUILD as shell and has nowhere to read it from. This keeps the copies
//! honest: `version 0.0.5` writes them all at once, and `version` on its own
//! fails when any of them has drifted (ADR 0008).
//!
//! `pkgrel` is not a copy of anything. CI sets it to the run number of the
//! build that produced the package, so this resets it to 0 on a bump and
//! leaves it alone otherwise.

use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Packages versioned by hand, on a schedule of their own: built from a pinned
/// upstream commit or from a key file, not from this workspace. Every other
/// directory in `aports/` carries the workspace version, so a new aport is
/// covered the day it is added — and `ci/build-packages.sh` leaves exactly
/// these two unstamped.
const INDEPENDENT: [&str; 2] = ["alpymist-keys", "squint"];

/// Set every version to `set`, or, without one, check that they already agree.
///
/// `expect` is the release tag a Release run is building, with or without its
/// leading `v`: the workspace version has to be it, or the run is building
/// something other than what the tag says.
pub fn version(root: &Path, set: Option<&str>, expect: Option<&str>) -> Result<()> {
    if let Some(new) = set {
        check_version(new)?;
        let mut changed = false;
        for (what, path) in files(root)? {
            let from = read(&path, &what)?;
            if from == new {
                println!("  {what:<32} {new} (unchanged)");
                continue;
            }
            write(&path, &what, new)?;
            println!("  {what:<32} {from} -> {new}");
            changed = true;
        }
        if changed {
            println!("\nnow refresh Cargo.lock:\n  cargo update --workspace");
        }
        return Ok(());
    }

    let found: BTreeMap<String, String> = files(root)?
        .into_iter()
        .map(|(what, path)| Ok((what.clone(), read(&path, &what)?)))
        .collect::<Result<_>>()?;
    let workspace = found
        .get("Cargo.toml")
        .expect("files() always yields Cargo.toml")
        .clone();
    let drifted: Vec<_> = found
        .iter()
        .filter(|(_, v)| **v != workspace)
        .map(|(what, v)| format!("{what} is {v}"))
        .collect();
    ensure!(
        drifted.is_empty(),
        "the workspace is {workspace}, but:\n  {}\nrun `cargo xtask version {workspace}`",
        drifted.join("\n  ")
    );

    if let Some(tag) = expect {
        let tag = tag.strip_prefix('v').unwrap_or(tag);
        ensure!(
            tag == workspace,
            "this build is tagged {tag} but the workspace is {workspace}; \
             run `cargo xtask version {tag}` and release the commit that has it"
        );
    }

    println!(
        "{} packages, all at {workspace}",
        found.len().saturating_sub(1)
    );
    Ok(())
}

/// Every file holding a copy of the version, as (how to name it, where it is).
/// `Cargo.toml` comes first; it is the one the others are checked against.
fn files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = vec![("Cargo.toml".to_owned(), root.join("Cargo.toml"))];
    let aports = root.join("aports");
    let mut ports = Vec::new();
    for entry in
        std::fs::read_dir(&aports).with_context(|| format!("reading {}", aports.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if INDEPENDENT.contains(&name.as_str()) || !entry.path().join("APKBUILD").is_file() {
            continue;
        }
        ports.push(name);
    }
    ensure!(!ports.is_empty(), "no aports under {}", aports.display());
    ports.sort();
    out.extend(ports.into_iter().map(|name| {
        (
            format!("aports/{name}"),
            aports.join(&name).join("APKBUILD"),
        )
    }));
    Ok(out)
}

/// The version `path` carries: `Cargo.toml`'s `[workspace.package]` version, or
/// an APKBUILD's `pkgver`.
fn read(path: &Path, what: &str) -> Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {what}"))?;
    let found = if path.ends_with("Cargo.toml") {
        workspace_version(&text)
    } else {
        text.lines()
            .find_map(|line| line.strip_prefix("pkgver="))
            .map(str::to_owned)
    };
    found.with_context(|| format!("{what} has no version in it"))
}

/// Replace that version with `new`, leaving everything else byte for byte.
fn write(path: &Path, what: &str, new: &str) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {what}"))?;
    let out = if path.ends_with("Cargo.toml") {
        set_workspace_version(&text, new)
            .with_context(|| format!("{what} has no [workspace.package] version"))?
    } else {
        set_pkgver(&text, new)
    };
    std::fs::write(path, out).with_context(|| format!("writing {what}"))
}

/// Cargo.toml carries `version = ` under `[workspace.package]` and again under
/// `[workspace.dependencies]`; only the one in its own table is ours.
fn set_workspace_version(text: &str, new: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut table = "";
    let mut done = false;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name;
        }
        if !done && table == "workspace.package" && line.starts_with("version = ") {
            let _ = writeln!(out, "version = \"{new}\"");
            done = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    done.then_some(out)
}

/// An APKBUILD has one `pkgver` and one `pkgrel`, both on lines of their own.
/// A new version starts again at `pkgrel=0`; on a CI build that is overwritten
/// with the build number, and a local build gets the 0.
fn set_pkgver(text: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if line.starts_with("pkgver=") {
            let _ = writeln!(out, "pkgver={new}");
        } else if line.starts_with("pkgrel=") {
            out.push_str("pkgrel=0\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The `version = "..."` under `[workspace.package]`.
fn workspace_version(text: &str) -> Option<String> {
    let mut table = "";
    for line in text.lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name;
        }
        if table == "workspace.package"
            && let Some(rest) = line.strip_prefix("version = ")
        {
            return Some(rest.trim().trim_matches('"').to_owned());
        }
    }
    None
}

/// `0.0.4`, and nothing apk would sort in a way we did not mean.
fn check_version(v: &str) -> Result<()> {
    let parts: Vec<_> = v.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("{v} is not a version like 0.0.4");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_version, set_pkgver, set_workspace_version, workspace_version};

    const CARGO: &str = "[workspace]\nresolver = \"3\"\n\n\
                         [workspace.package]\nversion = \"0.0.4\"\nedition = \"2024\"\n\n\
                         [workspace.dependencies]\nserde = { version = \"1\" }\n";

    #[test]
    fn the_workspace_version_is_the_one_under_its_own_table() {
        assert_eq!(workspace_version(CARGO).as_deref(), Some("0.0.4"));
    }

    #[test]
    fn a_dependency_version_is_not_mistaken_for_it() {
        let no_package = CARGO.replace("[workspace.package]\nversion = \"0.0.4\"\n", "");
        assert_eq!(workspace_version(&no_package), None);
    }

    #[test]
    fn setting_it_leaves_a_dependency_version_alone() {
        let out = set_workspace_version(CARGO, "0.0.5").expect("the table is there");
        assert!(out.contains("\nversion = \"0.0.5\"\n"), "{out}");
        assert!(out.contains("serde = { version = \"1\" }"), "{out}");
    }

    const APKBUILD: &str = "pkgname=alpymist\npkgver=0.0.4\npkgrel=7\n\
                            pkgdesc=\"pkgver= is not a field here\"\n";

    #[test]
    fn a_bump_starts_the_package_release_again() {
        let out = set_pkgver(APKBUILD, "0.0.5");
        assert_eq!(
            out,
            "pkgname=alpymist\npkgver=0.0.5\npkgrel=0\n\
             pkgdesc=\"pkgver= is not a field here\"\n"
        );
    }

    #[test]
    fn only_a_three_part_number_is_accepted() {
        assert!(check_version("0.0.4").is_ok());
        assert!(check_version("10.2.30").is_ok());
        assert!(check_version("v0.0.4").is_err());
        assert!(check_version("0.0.4-rc1").is_err());
        assert!(check_version("0.0").is_err());
        assert!(check_version("0.0.").is_err());
    }
}

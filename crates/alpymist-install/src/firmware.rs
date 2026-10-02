//! Firmware for the installed system.
//!
//! The live system has every firmware file there is, in the kernel's modloop.
//! The installed system has only the packages it is given, and `setup-disk`
//! chooses them from the install medium alone — silently dropping any it does
//! not find there. The medium carries a few common ones, so a laptop whose
//! Wi-Fi card needed anything else installed "successfully" and then had no
//! Wi-Fi at all.
//!
//! So once the new system has its online repositories, the installer asks the
//! same question `setup-disk` does — which firmware do the loaded drivers name —
//! and installs the answer from both the medium and the network.
//!
//! The same rule as `setup-disk`, deliberately: see
//! [`alpymist_core::firmware::package_for`], which `alpymist firmware watch`
//! also uses for whatever a driver asks for after installation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The packages holding these firmware paths, as `modinfo -F firmware` prints
/// them, and the regulatory database if there is a wireless interface.
#[must_use]
pub fn packages(firmware: &str, wireless: bool) -> Vec<String> {
    let mut wanted: BTreeSet<String> = firmware
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(alpymist_core::firmware::package_for)
        .collect();
    if wireless {
        wanted.insert("wireless-regdb".into());
    }
    wanted.into_iter().collect()
}

/// The repositories a system names, without comments or blank lines.
#[must_use]
pub fn repositories(file: &str) -> Vec<String> {
    file.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether this machine has a wireless network interface.
#[must_use]
pub fn has_wireless() -> bool {
    std::fs::read_dir("/sys/class/net").is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().join("wireless").is_dir())
    })
}

/// The tops of the install media among `repos`: a repository that is a
/// directory, `/media/usb/apks`, is on a medium whose top is its parent.
#[must_use]
pub fn media(repos: &[String]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for repo in repos {
        if let Some(top) = repo
            .strip_prefix('/')
            .and_then(|_| Path::new(repo).parent())
            && !found.iter().any(|f| f == top)
        {
            found.push(top.to_path_buf());
        }
    }
    found
}

/// Copy firmware a stick carries (`alpymist firmware broadcom --to`) into
/// the system at `root`, and say so. Nothing carried is nothing said.
#[must_use]
pub fn carried(root: &str, media: &[PathBuf]) -> Vec<String> {
    let into = Path::new(root).join("lib/firmware");
    let mut report = Vec::new();
    for top in media {
        let from = top.join(alpymist_core::firmware::CARRIED);
        match alpymist_core::firmware::take_carried(&from, &into) {
            Ok(0) => {}
            Ok(copied) => report.push(format!(
                "Copied {copied} firmware files carried in {}",
                from.display()
            )),
            Err(e) => report.push(format!(
                "Could not copy firmware from {}: {e}",
                from.display()
            )),
        }
    }
    report
}

/// Install the firmware this machine's drivers need into the system at `root`.
///
/// Run as `alpymist-install firmware /mnt`, as a step of the install plan.
///
/// # Errors
/// What went wrong, for the install log. The system boots without it.
pub fn install(root: &str) -> Result<Vec<String>, String> {
    // What a stick carries first, whatever becomes of the packages: it is
    // there for the machine with no network to fetch them over.
    let live = std::fs::read_to_string("/etc/apk/repositories").unwrap_or_default();
    let brought = carried(root, &media(&repositories(&live)));
    match packaged(root) {
        Ok(mut report) => {
            if brought.is_empty() && report.iter().any(|l| l.contains("linux-firmware-b43")) {
                report.push(
                    "Broadcom Wi-Fi: its firmware is in no package. Once installed, with a \
                     network: doas alpymist firmware broadcom"
                        .into(),
                );
            }
            Ok(brought.into_iter().chain(report).collect())
        }
        Err(why) => Err(brought
            .into_iter()
            .chain(std::iter::once(why))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

/// The firmware packages the loaded drivers name, from the medium and the
/// network.
fn packaged(root: &str) -> Result<Vec<String>, String> {
    let modules: Vec<String> = std::fs::read_dir("/sys/module")
        .map_err(|e| format!("could not list the loaded drivers: {e}"))?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let output = Command::new("modinfo")
        .args(["-F", "firmware"])
        .args(&modules)
        .output()
        .map_err(|e| format!("could not run modinfo: {e}"))?;
    let wanted = packages(&String::from_utf8_lossy(&output.stdout), has_wireless());
    if wanted.is_empty() {
        return Ok(vec!["No firmware is needed.".into()]);
    }

    // The new system's own repositories, which are online, and the live
    // system's, which include the install medium: whichever has a package.
    let mut repos = Vec::new();
    for file in [
        format!("{}/etc/apk/repositories", root.trim_end_matches('/')),
        "/etc/apk/repositories".to_string(),
    ] {
        if let Ok(text) = std::fs::read_to_string(&file) {
            for repo in repositories(&text) {
                if !repos.contains(&repo) {
                    repos.push(repo);
                }
            }
        }
    }
    let apk = |args: &[&str]| {
        let mut command = Command::new("apk");
        command.args(["--root", root, "--repositories-file", "/dev/null"]);
        for repo in &repos {
            command.args(["--repository", repo]);
        }
        command.args(args).output()
    };

    // Names the drivers mention but nobody packages — linux-firmware-b43, for
    // one — would fail the whole add, so ask which exist first. With no
    // network the online indexes cannot be fetched; what is on the medium is
    // still found.
    let mut search = vec!["search", "--quiet", "--exact", "--update-cache"];
    search.extend(wanted.iter().map(String::as_str));
    let found = apk(&search).map_err(|e| format!("could not run apk: {e}"))?;
    // `--quiet` prints bare names, one per line.
    let listed = String::from_utf8_lossy(&found.stdout);
    let entries: Vec<&str> = listed.split_whitespace().collect();
    let available: Vec<String> = wanted
        .iter()
        .filter(|name| entries.contains(&name.as_str()))
        .cloned()
        .collect();

    let mut report = vec![format!("Drivers ask for: {}", wanted.join(" "))];
    let missing: Vec<&String> = wanted.iter().filter(|w| !available.contains(w)).collect();
    if !missing.is_empty() {
        report.push(format!(
            "Not available here: {}",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if available.is_empty() {
        return Ok(report);
    }

    let mut add = vec!["add", "--no-progress"];
    add.extend(available.iter().map(String::as_str));
    let added = apk(&add).map_err(|e| format!("could not run apk: {e}"))?;
    report.extend(
        String::from_utf8_lossy(&added.stdout)
            .lines()
            .chain(String::from_utf8_lossy(&added.stderr).lines())
            .map(str::to_string),
    );
    if added.status.success() {
        report.push(format!("Installed: {}", available.join(" ")));
        Ok(report)
    } else {
        Err(report.join("\n"))
    }
}

/// Whether `path` could be a root to install firmware into.
#[must_use]
pub fn is_root(path: &str) -> bool {
    Path::new(path).join("etc/apk").is_dir()
}

#[cfg(test)]
mod tests {
    use super::{carried, media, packages, repositories};
    use std::path::PathBuf;

    #[test]
    fn a_stick_is_found_from_its_repository_and_what_it_carries_is_copied() {
        let repos = [
            "/media/usb/apks".to_string(),
            "https://dl-cdn.alpinelinux.org/alpine/v3.24/main".to_string(),
            "/media/usb/apks".to_string(),
        ];
        assert_eq!(media(&repos), [PathBuf::from("/media/usb")]);

        let d = std::env::temp_dir().join(format!("alpymist-carried-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let stick = d.join("usb");
        let root = d.join("mnt");
        std::fs::create_dir_all(stick.join("alpymist-firmware/b43")).unwrap();
        std::fs::write(stick.join("alpymist-firmware/b43/ucode16_mimo.fw"), "u").unwrap();
        let said = carried(root.to_str().unwrap(), &[stick.clone(), d.join("cdrom")]);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("Copied 1 firmware files"));
        assert!(root.join("lib/firmware/b43/ucode16_mimo.fw").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn firmware_is_found_in_its_vendors_package() {
        let modinfo = "rtlwifi/rtl8723bs_nic.bin\nrtlwifi/rtl8723bs_wowlan.bin\n\
                       i915/kbl_dmc_ver1_04.bin\nregulatory.db\n\n";
        assert_eq!(
            packages(modinfo, false),
            [
                "linux-firmware-i915",
                "linux-firmware-other",
                "linux-firmware-rtlwifi",
            ]
        );
    }

    #[test]
    fn a_wireless_machine_also_gets_the_regulatory_database() {
        assert_eq!(packages("", true), ["wireless-regdb"]);
        assert!(packages("", false).is_empty());
    }

    #[test]
    fn repositories_ignore_comments_and_blank_lines() {
        let file = "/media/cdrom/apks\n#/media/usb/apks\n\n  https://x/main  # online\n";
        assert_eq!(repositories(file), ["/media/cdrom/apks", "https://x/main"]);
    }
}

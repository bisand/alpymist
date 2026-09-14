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
//! The same rule as `setup-disk`, deliberately: firmware under `vendor/` is in
//! `linux-firmware-vendor`, and a file at the top level is in
//! `linux-firmware-other`.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The packages holding these firmware paths, as `modinfo -F firmware` prints
/// them, and the regulatory database if there is a wireless interface.
#[must_use]
pub fn packages(firmware: &str, wireless: bool) -> Vec<String> {
    let mut wanted: BTreeSet<String> = firmware
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|path| match path.split_once('/') {
            Some((vendor, _)) if !vendor.is_empty() => format!("linux-firmware-{vendor}"),
            _ => "linux-firmware-other".to_string(),
        })
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

/// Install the firmware this machine's drivers need into the system at `root`.
///
/// Run as `alpymist-install firmware /mnt`, as a step of the install plan.
///
/// # Errors
/// What went wrong, for the install log. The system boots without it.
pub fn install(root: &str) -> Result<Vec<String>, String> {
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
    use super::{packages, repositories};

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

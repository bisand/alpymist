//! Which Alpine package holds a firmware file.
//!
//! Alpine splits linux-firmware by directory: a file under `vendor/` is in
//! `linux-firmware-vendor`, and a file at the top level is in
//! `linux-firmware-other`. Its `setup-disk` picks firmware by that rule, and so
//! does Alpymist, twice: the installer for the drivers loaded while it runs,
//! and `alpymist firmware watch` for whatever a driver asks for later.
//!
//! Some firmware is in no package, because its maker lets nobody pass it on:
//! Broadcom's for the `b43` driver is the one known here. Alpymist does not
//! ship it either. `alpymist firmware broadcom` fetches it from where its
//! maker's own driver is published and cuts it out, on the machine that asks
//! (ADR 0021); and since the machine that needs it is the one with no
//! network, it can be put on a stick by another and carried. This is the
//! carrying: a directory named [`CARRIED`] on the stick, and [`take_carried`]
//! to put what is in it where the kernel looks.

use std::path::Path;

/// The directory on a stick that firmware is carried in.
pub const CARRIED: &str = "alpymist-firmware";

/// The firmware directories that may be carried: each a driver's own, under
/// `/lib/firmware`, that no package fills.
pub const CARRIED_DIRS: [&str; 1] = ["b43"];

/// Copy what a stick carries into a firmware directory: for each of
/// [`CARRIED_DIRS`] under `carried`, its `.fw` files, into the same directory
/// under `firmware`. Returns how many files were copied; none, with no
/// error, where nothing is carried.
///
/// Only ordinary files with ordinary names, and only into those directories:
/// this runs as root on what a stick holds.
///
/// # Errors
/// A directory could not be made or a file not copied.
pub fn take_carried(carried: &Path, firmware: &Path) -> std::io::Result<usize> {
    let mut copied = 0;
    for dir in CARRIED_DIRS {
        let Ok(entries) = std::fs::read_dir(carried.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            // The kernel asks for these names exactly, so exactly `.fw`.
            let plain = Path::new(&name).extension().is_some_and(|ext| ext == "fw")
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
            // Not a link: `symlink_metadata` does not follow one.
            let ordinary = std::fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_file());
            if !plain || !ordinary {
                continue;
            }
            let into = firmware.join(dir);
            std::fs::create_dir_all(&into)?;
            std::fs::copy(entry.path(), into.join(&name))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// The package that holds firmware `path`, as a driver names it
/// (`rtl_nic/rtl8153b-2.fw`), if the path is one a package could hold.
///
/// `None` for an absolute path, one that climbs out with `..`, or one whose
/// directory is not an ordinary name: the path is looked for under
/// `/lib/firmware`, and the directory becomes part of a package name given to
/// `apk`. The file name itself may be anything else; some have spaces in them.
#[must_use]
pub fn package_for(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\0'))
    {
        return None;
    }
    match parts.as_slice() {
        [_file] => Some("linux-firmware-other".to_string()),
        [vendor, ..]
            if vendor
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')) =>
        {
            Some(format!("linux-firmware-{vendor}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{package_for, take_carried};

    #[test]
    fn carried_firmware_is_copied_and_nothing_else_on_the_stick_is() {
        let d = std::env::temp_dir().join(format!("alpymist-carried-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let stick = d.join("stick");
        let firmware = d.join("firmware");
        std::fs::create_dir_all(stick.join("b43")).unwrap();
        std::fs::create_dir_all(stick.join("other")).unwrap();
        std::fs::write(stick.join("b43/ucode16_mimo.fw"), "u").unwrap();
        std::fs::write(stick.join("b43/n0initvals16.fw"), "n").unwrap();
        std::fs::write(stick.join("b43/readme.txt"), "not firmware").unwrap();
        std::fs::write(stick.join("b43/a b.fw"), "odd name").unwrap();
        std::fs::write(stick.join("other/x.fw"), "not a carried directory").unwrap();
        std::os::unix::fs::symlink("/etc/shadow", stick.join("b43/link.fw")).unwrap();

        assert_eq!(take_carried(&stick, &firmware).unwrap(), 2);
        let mut names: Vec<String> = std::fs::read_dir(firmware.join("b43"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["n0initvals16.fw", "ucode16_mimo.fw"]);
        assert!(!firmware.join("other").exists());

        // Nothing carried is nothing done, and no error.
        assert_eq!(take_carried(&d.join("empty"), &d.join("none")).unwrap(), 0);
        assert!(!d.join("none").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_in_a_directory_is_in_that_directorys_package() {
        assert_eq!(
            package_for("rtl_nic/rtl8153b-2.fw").as_deref(),
            Some("linux-firmware-rtl_nic")
        );
        assert_eq!(
            package_for("i915/kbl_dmc_ver1_04.bin").as_deref(),
            Some("linux-firmware-i915")
        );
        assert_eq!(
            package_for(
                "brcm/brcmfmac43455-sdio.Raspberry Pi Foundation-Raspberry Pi 4 Model B.txt"
            )
            .as_deref(),
            Some("linux-firmware-brcm")
        );
    }

    #[test]
    fn a_file_at_the_top_is_in_other() {
        assert_eq!(
            package_for("iwlwifi-8265-36.ucode").as_deref(),
            Some("linux-firmware-other")
        );
    }

    #[test]
    fn nothing_that_climbs_out_or_has_an_odd_directory_names_a_package() {
        for path in [
            "",
            "/lib/firmware/x.bin",
            "../etc/passwd",
            "rtl_nic/../x",
            "rtl_nic//x",
            "a b/c",
            "x;rm/y",
            "dir/",
        ] {
            assert_eq!(package_for(path), None, "{path:?}");
        }
    }
}

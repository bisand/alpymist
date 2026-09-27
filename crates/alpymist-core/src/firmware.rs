//! Which Alpine package holds a firmware file.
//!
//! Alpine splits linux-firmware by directory: a file under `vendor/` is in
//! `linux-firmware-vendor`, and a file at the top level is in
//! `linux-firmware-other`. Its `setup-disk` picks firmware by that rule, and so
//! does Alpymist, twice: the installer for the drivers loaded while it runs,
//! and `alpymist firmware watch` for whatever a driver asks for later.

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
    use super::package_for;

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

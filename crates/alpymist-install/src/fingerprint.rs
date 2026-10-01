//! A fingerprint reader, and what the installed system needs for it.
//!
//! Synaptics and Validity match-on-chip readers, as in many Lenovo laptops, have no
//! libfprint driver, and validity-fprintd drives them in fprintd's place. When
//! the live system sees one, the installer puts in that driver, its service
//! and the Fingerprints window, which brings `pam_fprintd`. Nothing is turned
//! on by it: a finger unlocks nothing until someone enrols one and Settings ›
//! System says it may (ADR 0016).
//!
//! The driver is installed, not the reader set up. A reader fresh from the
//! factory needs Lenovo's firmware and a pairing written to its flash, which
//! is not something to do to a machine on the way past; one that has been
//! used with Windows, as nearly all have, works as it is.

use std::path::Path;

/// The readers validity-fprintd drives, as USB vendor and product, with
/// their names. validity-fprintd's own list (`src/usb.rs`, and its udev rules)
/// is the authority; this follows it.
pub const READERS: &[(u16, u16, &str)] = &[
    (0x138a, 0x0090, "Validity VFS7500"),
    (0x138a, 0x0097, "Validity VFS7552"),
    (0x138a, 0x009d, "Validity VFS7552"),
    (0x06cb, 0x009a, "Synaptics Metallica MIS"),
];

/// What a machine with one of them gets: the driver, its service, and the
/// window fingers are enrolled in, which depends on `pam_fprintd`.
pub const PACKAGES: [&str; 3] = [
    "validity-fprintd",
    "validity-fprintd-openrc",
    "alpymist-fingerprint",
];

/// The driver's service.
pub const SERVICE: &str = "validity-fprintd";

/// Where the kernel lists USB devices.
const USB_DEVICES: &str = "/sys/bus/usb/devices";

/// The name of the fingerprint reader on this machine, if it is one of
/// [`READERS`].
#[must_use]
pub fn detect() -> Option<&'static str> {
    reader_in(Path::new(USB_DEVICES))
}

/// The first of [`READERS`] among the devices listed in `devices`, a
/// directory shaped like `/sys/bus/usb/devices`: one entry per device, each
/// with `idVendor` and `idProduct` in hexadecimal.
#[must_use]
pub fn reader_in(devices: &Path) -> Option<&'static str> {
    let read = |dir: &Path, name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .and_then(|text| u16::from_str_radix(text.trim(), 16).ok())
    };
    std::fs::read_dir(devices)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            Some((read(&dir, "idVendor")?, read(&dir, "idProduct")?))
        })
        .find_map(|(vendor, product)| {
            READERS
                .iter()
                .find(|(v, p, _)| (*v, *p) == (vendor, product))
                .map(|(_, _, name)| *name)
        })
}

#[cfg(test)]
mod tests {
    use super::reader_in;

    #[test]
    fn a_known_reader_is_found_among_the_usb_devices_and_nothing_else_is() {
        let d = std::env::temp_dir().join(format!("alpymist-fp-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let device = |name: &str, vendor: &str, product: &str| {
            let dir = d.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("idVendor"), format!("{vendor}\n")).unwrap();
            std::fs::write(dir.join("idProduct"), format!("{product}\n")).unwrap();
        };
        // A hub and a webcam; an interface has no ids at all.
        device("usb1", "1d6b", "0002");
        device("1-8", "5986", "2113");
        std::fs::create_dir_all(d.join("1-8:1.0")).unwrap();
        assert_eq!(reader_in(&d), None);

        // The X1 Carbon's.
        device("1-9", "06cb", "009a");
        assert_eq!(reader_in(&d), Some("Synaptics Metallica MIS"));
        assert_eq!(reader_in(&d.join("missing")), None);
        std::fs::remove_dir_all(&d).ok();
    }
}

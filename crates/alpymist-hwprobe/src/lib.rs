//! Reads [`Capabilities`] from a running Linux system via `/proc` and `/sys`.
//!
//! Everything here is best-effort: a field we cannot read becomes `None` or a
//! conservative default, never a panic and never an optimistic guess. The
//! interesting decisions all live in [`alpymist_core::select_tier`].

#![forbid(unsafe_code)]

use alpymist_core::{Capabilities, Error, GpuDevice, Result, Virtualisation};
use std::fs;
use std::path::Path;

/// Probe the current machine.
///
/// # Errors
/// Returns [`Error::UnsupportedHost`] when not running on Linux, and
/// [`Error::Probe`] when a required `/proc` node cannot be read.
pub fn probe() -> Result<Capabilities> {
    if !cfg!(target_os = "linux") {
        return Err(Error::UnsupportedHost(std::env::consts::OS));
    }
    Ok(Capabilities {
        memory_mib: memory_mib()?,
        cpus: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        gpus: drm_devices(Path::new("/sys/class/drm")),
        gles: alpymist_glesprobe::probe(),
        virtualisation: virtualisation(),
    })
}

/// Total RAM in MiB, parsed from `MemTotal` in `/proc/meminfo`.
fn memory_mib() -> Result<u64> {
    let path = "/proc/meminfo";
    let text = fs::read_to_string(path).map_err(|source| Error::Probe {
        path: path.into(),
        source,
    })?;
    let kib = text
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(kib / 1024)
}

/// Enumerate `cardN` entries under a `/sys/class/drm`-shaped directory.
///
/// Split out from [`probe`] so it can be pointed at a fixture in tests.
fn drm_devices(root: &Path) -> Vec<GpuDevice> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut cards: Vec<GpuDevice> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            // `card0` yes; `card0-HDMI-A-1` (a connector) no.
            let rest = name.strip_prefix("card")?;
            if !rest.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let path = e.path();
            Some(GpuDevice {
                driver: driver_name(&path),
                has_connected_output: any_connector_connected(root, &name),
                card: name,
            })
        })
        .collect();
    cards.sort_by(|a, b| a.card.cmp(&b.card));
    cards
}

/// Resolve `<card>/device/driver` to the bound kernel module's name.
fn driver_name(card: &Path) -> Option<String> {
    let link = fs::read_link(card.join("device/driver")).ok()?;
    Some(link.file_name()?.to_str()?.to_owned())
}

/// True if any connector belonging to `card` reports `connected`.
fn any_connector_connected(root: &Path, card: &str) -> bool {
    let prefix = format!("{card}-");
    fs::read_dir(root).into_iter().flatten().flatten().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(&prefix))
            && fs::read_to_string(e.path().join("status")).is_ok_and(|s| s.trim() == "connected")
    })
}

/// Guess the virtualisation flavour from DMI and the bound DRM driver.
fn virtualisation() -> Virtualisation {
    let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let vendor = vendor.trim().to_ascii_lowercase();
    if vendor.contains("qemu") || vendor.contains("kvm") {
        return Virtualisation::Kvm;
    }
    if [
        "vmware",
        "innotek",
        "microsoft",
        "parallels",
        "xen",
        "bochs",
    ]
    .iter()
    .any(|v| vendor.contains(v))
    {
        return Virtualisation::Other;
    }
    if Path::new("/sys/bus/virtio").exists() {
        return Virtualisation::Kvm;
    }
    Virtualisation::Bare
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fake `/sys/class/drm` tree under a scratch directory.
    fn fixture(dir: &Path, cards: &[(&str, &str)], connectors: &[(&str, &str)]) {
        for (card, _) in cards {
            fs::create_dir_all(dir.join(card)).unwrap();
        }
        for (name, status) in connectors {
            let p = dir.join(name);
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("status"), status).unwrap();
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("alpymist-hwprobe-{name}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_drm_directory_yields_no_devices() {
        assert!(drm_devices(Path::new("/nonexistent/class/drm")).is_empty());
    }

    #[test]
    fn connector_entries_are_not_mistaken_for_cards() {
        let d = scratch("connectors");
        fixture(
            &d,
            &[("card0", "i915")],
            &[
                ("card0-HDMI-A-1", "disconnected"),
                ("card0-eDP-1", "connected"),
            ],
        );
        let found = drm_devices(&d);
        assert_eq!(found.len(), 1, "only card0 is a card: {found:?}");
        assert_eq!(found[0].card, "card0");
        assert!(found[0].has_connected_output);
    }

    #[test]
    fn a_card_with_no_connected_output_is_reported_as_such() {
        let d = scratch("headless");
        fixture(&d, &[("card0", "")], &[("card0-DP-1", "disconnected")]);
        assert!(!drm_devices(&d)[0].has_connected_output);
    }

    #[test]
    fn cards_are_returned_in_stable_order() {
        let d = scratch("order");
        fixture(&d, &[("card1", ""), ("card0", ""), ("card2", "")], &[]);
        let names: Vec<_> = drm_devices(&d).into_iter().map(|g| g.card).collect();
        assert_eq!(names, ["card0", "card1", "card2"]);
    }
}

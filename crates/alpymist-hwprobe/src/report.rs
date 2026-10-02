//! What this machine is made of, as text somebody may choose to send.
//!
//! `alpymist report` prints this. It exists so that hardware Alpymist does not
//! drive yet — a Wi-Fi card with no firmware, a sound codec nobody has heard —
//! can be told to the people who could mend that, by the person who has the
//! machine, when and if they decide to (ADR 0020).
//!
//! It holds what the hardware is and which driver has it, and nothing that
//! names a person or tells one machine from another of its kind: no serial
//! number, no address of a network card, no host or account name, no network
//! that was joined. Devices are given by the numbers their makers gave them,
//! not by the names they announce, since a phone announces its owner's. A
//! network interface is given by its kind and driver, since its name can hold
//! its address. The test at the end of this file holds it to that.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the kernel's and the system's files are, so a test can point at a
/// directory of its own.
#[derive(Debug, Clone)]
pub struct Roots {
    /// `/sys`.
    pub sys: PathBuf,
    /// `/proc`.
    pub proc: PathBuf,
    /// `/etc`.
    pub etc: PathBuf,
}

impl Default for Roots {
    fn default() -> Self {
        Self {
            sys: "/sys".into(),
            proc: "/proc".into(),
            etc: "/etc".into(),
        }
    }
}

/// A file's text without the space around it, if it can be read and holds
/// any.
fn read(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A sysfs number, `0x8086`, without its prefix.
fn hex(path: &Path) -> Option<String> {
    read(path).map(|v| v.trim_start_matches("0x").to_string())
}

/// The driver bound to the device at `dir`, or a dash.
fn driver(dir: &Path) -> String {
    fs::read_link(dir.join("driver"))
        .ok()
        .and_then(|l| l.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "-".into())
}

/// The names in a directory, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// What a PCI class code's first byte says the device is.
fn pci_kind(class: &str) -> &'static str {
    match class.get(..2).unwrap_or("") {
        "01" => "storage",
        "02" => "network",
        "03" => "display",
        "04" => "multimedia",
        "05" => "memory",
        "06" => "bridge",
        "07" => "communication",
        "08" => "system",
        "09" => "input",
        "0c" => "serial bus",
        "0d" => "wireless",
        "10" => "encryption",
        "11" => "signal",
        "12" => "accelerator",
        _ => "other",
    }
}

/// The machine's make and model, and what runs on it.
fn machine(roots: &Roots, out: &mut String) {
    let dmi = roots.sys.join("class/dmi/id");
    // The serial number and the UUID are beside these, and are never read.
    for (label, file) in [
        ("vendor", "sys_vendor"),
        ("product", "product_name"),
        ("version", "product_version"),
        ("board", "board_name"),
        ("firmware", "bios_version"),
        ("firmware date", "bios_date"),
    ] {
        if let Some(value) = read(&dmi.join(file)) {
            let _ = writeln!(out, "{label:<15}{value}");
        }
    }
    let _ = writeln!(out, "{:<15}{}", "architecture", std::env::consts::ARCH);
    if let Some(kernel) = read(&roots.proc.join("sys/kernel/osrelease")) {
        let _ = writeln!(out, "{:<15}{kernel}", "kernel");
    }
    if let Some(alpine) = read(&roots.etc.join("alpine-release")) {
        let _ = writeln!(out, "{:<15}{alpine}", "alpine");
    }
    let cpuinfo = fs::read_to_string(roots.proc.join("cpuinfo")).unwrap_or_default();
    if let Some(model) = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|l| l.split_once(':'))
    {
        let _ = writeln!(out, "{:<15}{}", "processor", model.1.trim());
    }
}

/// Every PCI device: where it sits, what it is, whose it is, and its driver.
fn pci(roots: &Roots, out: &mut String) {
    let root = roots.sys.join("bus/pci/devices");
    for name in names(&root) {
        let dir = root.join(&name);
        let (Some(class), Some(vendor), Some(device)) = (
            hex(&dir.join("class")),
            hex(&dir.join("vendor")),
            hex(&dir.join("device")),
        ) else {
            continue;
        };
        let subsystem = match (
            hex(&dir.join("subsystem_vendor")),
            hex(&dir.join("subsystem_device")),
        ) {
            (Some(v), Some(d)) => format!("{v}:{d}"),
            _ => "-".into(),
        };
        // `0000:00:1f.3` without the domain nearly every machine has one of.
        let slot = name.strip_prefix("0000:").unwrap_or(&name);
        let _ = writeln!(
            out,
            "{slot:<9}{:<15}{:<6}{vendor}:{device}  {subsystem:<11}{}",
            pci_kind(&class),
            class.get(..4).unwrap_or(&class),
            driver(&dir)
        );
    }
}

/// Every USB device but the hubs the machine starts from: whose it is, and
/// what each of its interfaces is and which driver has it.
fn usb(roots: &Roots, out: &mut String) {
    let root = roots.sys.join("bus/usb/devices");
    let all = names(&root);
    let mut lines = Vec::new();
    for name in &all {
        // `usb1` is a root hub, and `1-1:1.0` an interface of `1-1`.
        if name.starts_with("usb") || name.contains(':') {
            continue;
        }
        let dir = root.join(name);
        let (Some(vendor), Some(product)) =
            (read(&dir.join("idVendor")), read(&dir.join("idProduct")))
        else {
            continue;
        };
        let prefix = format!("{name}:");
        let interfaces: Vec<String> = all
            .iter()
            .filter(|i| i.starts_with(&prefix))
            .filter_map(|i| {
                let at = root.join(i);
                let class = read(&at.join("bInterfaceClass"))?;
                Some(format!("{class} {}", driver(&at)))
            })
            .collect();
        lines.push(format!("{vendor}:{product}  {}", interfaces.join(", ")));
    }
    // By what they are, not by the socket they happen to be in.
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{}", line.trim_end());
    }
}

/// The network interfaces that are hardware: wired or wireless, and the
/// driver. Not their names, which can hold their addresses.
fn network(roots: &Roots, out: &mut String) {
    let root = roots.sys.join("class/net");
    let mut lines = Vec::new();
    for name in names(&root) {
        let dir = root.join(&name);
        if !dir.join("device").exists() {
            continue;
        }
        let kind = if dir.join("wireless").is_dir() {
            "wireless"
        } else {
            "wired"
        };
        lines.push(format!("{kind:<10}{}", driver(&dir.join("device"))));
    }
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
}

/// The sound cards, and the codec behind each.
fn sound(roots: &Roots, out: &mut String) {
    let root = roots.proc.join("asound");
    let cards = fs::read_to_string(root.join("cards")).unwrap_or_default();
    for line in cards.lines() {
        // ` 0 [PCH            ]: HDA-Intel - HDA Intel PCH`; the line after it
        // says where the card sits, which the PCI list already has.
        if let Some((_, what)) = line.split_once("]: ") {
            let _ = writeln!(out, "card   {}", what.trim());
        }
    }
    for card in names(&root) {
        if !card.starts_with("card") {
            continue;
        }
        for codec in names(&root.join(&card)) {
            if !codec.starts_with("codec#") {
                continue;
            }
            let text = fs::read_to_string(root.join(&card).join(&codec)).unwrap_or_default();
            let field = |name: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(name))
                    .map_or("-", str::trim)
                    .to_string()
            };
            let _ = writeln!(
                out,
                "codec  {}  vendor {}  subsystem {}",
                field("Codec:"),
                field("Vendor Id:"),
                field("Subsystem Id:")
            );
        }
    }
}

/// One section: its heading, and what `fill` writes under it, or a line
/// saying there was nothing.
fn section(out: &mut String, heading: &str, fill: impl FnOnce(&mut String)) {
    let mut body = String::new();
    fill(&mut body);
    let _ = writeln!(out, "\n## {heading}");
    out.push_str(if body.is_empty() {
        "none found\n"
    } else {
        &body
    });
}

/// The hardware of the machine these roots describe, as the sections of a
/// report.
#[must_use]
pub fn hardware(roots: &Roots) -> String {
    let mut out = String::new();
    section(&mut out, "Machine", |o| machine(roots, o));
    section(
        &mut out,
        "PCI: slot, kind, class, id, subsystem, driver",
        |o| {
            pci(roots, o);
        },
    );
    section(
        &mut out,
        "USB: id, then each interface's class and driver",
        |o| {
            usb(roots, o);
        },
    );
    section(&mut out, "Network", |o| network(roots, o));
    section(&mut out, "Sound", |o| sound(roots, o));
    out
}

#[cfg(test)]
mod tests {
    use super::{Roots, hardware};
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    /// A throwaway machine, removed when dropped.
    struct Machine(PathBuf);

    impl Machine {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("alpymist-report-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, path: &str, text: &str) -> &Self {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
            self
        }

        /// Bind the device at `path` to `driver`, as sysfs shows it.
        fn driver(&self, path: &str, driver: &str) -> &Self {
            let dir = self.0.join(path);
            fs::create_dir_all(&dir).unwrap();
            symlink(Path::new("../../drivers").join(driver), dir.join("driver")).unwrap();
            self
        }

        fn roots(&self) -> Roots {
            Roots {
                sys: self.0.join("sys"),
                proc: self.0.join("proc"),
                etc: self.0.join("etc"),
            }
        }
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A 2009 `MacBook` Pro, near enough: the machine this was written for.
    fn macbook(tag: &str) -> Machine {
        let m = Machine::new(tag);
        m.file("sys/class/dmi/id/sys_vendor", "Apple Inc.\n")
            .file("sys/class/dmi/id/product_name", "MacBookPro5,5\n")
            .file("sys/class/dmi/id/product_serial", "W89123ABC66D\n")
            .file("sys/class/dmi/id/product_uuid", "5d2f6a51-aaaa-bbbb\n")
            .file("proc/sys/kernel/osrelease", "6.18.54-0-lts\n")
            .file("proc/sys/kernel/hostname", "andres-macbook\n")
            .file("etc/alpine-release", "3.24.1\n")
            .file("etc/hostname", "andres-macbook\n")
            .file(
                "proc/cpuinfo",
                "processor\t: 0\nmodel name\t: Intel(R) Core(TM)2 Duo CPU P8700\n",
            )
            // Wi-Fi with nothing driving it, and sound with something.
            .file("sys/bus/pci/devices/0000:03:00.0/class", "0x028000\n")
            .file("sys/bus/pci/devices/0000:03:00.0/vendor", "0x14e4\n")
            .file("sys/bus/pci/devices/0000:03:00.0/device", "0x432b\n")
            .file(
                "sys/bus/pci/devices/0000:03:00.0/subsystem_vendor",
                "0x106b\n",
            )
            .file(
                "sys/bus/pci/devices/0000:03:00.0/subsystem_device",
                "0x008d\n",
            )
            .file("sys/bus/pci/devices/0000:00:08.0/class", "0x040300\n")
            .file("sys/bus/pci/devices/0000:00:08.0/vendor", "0x10de\n")
            .file("sys/bus/pci/devices/0000:00:08.0/device", "0x0ac0\n")
            .driver("sys/bus/pci/devices/0000:00:08.0", "snd_hda_intel")
            // A phone, which announces whose it is, and its serial number.
            .file("sys/bus/usb/devices/usb1/idVendor", "1d6b\n")
            .file("sys/bus/usb/devices/usb1/idProduct", "0002\n")
            .file("sys/bus/usb/devices/1-2/idVendor", "05ac\n")
            .file("sys/bus/usb/devices/1-2/idProduct", "12a8\n")
            .file("sys/bus/usb/devices/1-2/product", "André's iPhone\n")
            .file("sys/bus/usb/devices/1-2/serial", "00008030001C\n")
            .file("sys/bus/usb/devices/1-2:1.0/bInterfaceClass", "06\n")
            .file("sys/bus/usb/devices/1-2:1.2/bInterfaceClass", "ff\n")
            .driver("sys/bus/usb/devices/1-2:1.2", "ipheth")
            // A network card named after its own address.
            .file(
                "sys/class/net/enx0026b0aabbcc/address",
                "00:26:b0:aa:bb:cc\n",
            )
            .driver("sys/class/net/enx0026b0aabbcc/device", "forcedeth")
            .file("sys/class/net/lo/address", "00:00:00:00:00:00\n")
            .file(
                "proc/asound/cards",
                " 0 [NVidia         ]: HDA-Intel - HDA NVidia\n                      \
                 HDA NVidia at 0xd3480000 irq 21\n",
            )
            .file(
                "proc/asound/card0/codec#0",
                "Codec: Cirrus Logic CS4206\nAddress: 0\nVendor Id: 0x10134206\n\
                 Subsystem Id: 0x106b4d00\n",
            );
        m
    }

    #[test]
    fn the_report_says_what_the_hardware_is_and_what_drives_it() {
        let m = macbook("says");
        let report = hardware(&m.roots());
        for line in [
            "vendor         Apple Inc.",
            "product        MacBookPro5,5",
            "kernel         6.18.54-0-lts",
            "alpine         3.24.1",
            "processor      Intel(R) Core(TM)2 Duo CPU P8700",
            "03:00.0  network        0280  14e4:432b  106b:008d  -",
            "00:08.0  multimedia     0403  10de:0ac0  -          snd_hda_intel",
            "05ac:12a8  06 -, ff ipheth",
            "wired     forcedeth",
            "card   HDA-Intel - HDA NVidia",
            "codec  Cirrus Logic CS4206  vendor 0x10134206  subsystem 0x106b4d00",
        ] {
            assert!(
                report.lines().any(|l| l == line),
                "no line {line:?} in:\n{report}"
            );
        }
        assert!(!report.contains("1d6b"), "a root hub is not a device");
    }

    /// The whole of what makes this something a person can send: see the top
    /// of the file. A new section that reads a name or a number of the
    /// machine's own fails here.
    #[test]
    fn nothing_in_the_report_names_the_person_or_the_machine() {
        let m = macbook("names");
        let report = hardware(&m.roots()).to_lowercase();
        for secret in [
            "w89123abc66d",
            "5d2f6a51",
            "andres-macbook",
            "andré",
            "iphone",
            "00008030001c",
            "00:26:b0",
            "0026b0aabbcc",
            "enx",
        ] {
            assert!(!report.contains(secret), "{secret:?} is in:\n{report}");
        }
    }

    #[test]
    fn a_machine_with_nothing_to_read_says_so_under_every_heading() {
        let m = Machine::new("empty");
        let report = hardware(&m.roots());
        assert_eq!(report.matches("\n## ").count(), 5);
        // The machine still has an architecture; the rest found nothing.
        assert_eq!(report.matches("none found").count(), 4, "{report}");
    }
}

//! Driving QEMU and capturing what the guest says on its serial console.

use crate::Arch;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Everything a boot attempt produced.
pub struct BootLog {
    /// Serial console lines, in order.
    pub lines: Vec<String>,
    /// How long the guest ran.
    pub elapsed: Duration,
    /// Whether we gave up rather than seeing the sentinel.
    pub timed_out: bool,
    /// Where the full serial log was written, for debugging a failed boot.
    pub log_path: std::path::PathBuf,
}

impl Arch {
    /// The QEMU binary that emulates this architecture.
    fn qemu_binary(self) -> &'static str {
        match self {
            Self::Aarch64 => "qemu-system-aarch64",
            Self::X86_64 => "qemu-system-x86_64",
        }
    }

    /// Machine-specific arguments.
    ///
    /// aarch64 has no legacy BIOS, so it needs UEFI firmware; `x86_64` boots the
    /// ISO's El Torito image directly.
    ///
    /// Both get a virtio-gpu. Without one the guest has no DRM device at all
    /// and every boot reports the `Legacy` X11 tier, which would make the smoke
    /// test blind to exactly the Wayland paths it exists to cover. A VM with
    /// virtio-gpu is also the configuration most Alpymist installs will run in.
    fn machine_args(self) -> Vec<String> {
        let mut args: Vec<String> = match self {
            Self::Aarch64 => vec![
                "-M".into(),
                "virt".into(),
                "-cpu".into(),
                "cortex-a72".into(),
                "-bios".into(),
                firmware_path(),
            ],
            Self::X86_64 => vec!["-M".into(), "q35".into()],
        };
        args.extend(["-device".into(), "virtio-gpu-pci".into()]);
        args
    }
}

/// Where Homebrew's QEMU keeps its bundled UEFI firmware.
fn firmware_path() -> String {
    std::env::var("ALPYMIST_UEFI_FIRMWARE")
        .unwrap_or_else(|_| "/opt/homebrew/share/qemu/edk2-aarch64-code.fd".into())
}

/// Boot `iso` and capture serial output until `sentinel` appears or time runs out.
///
/// QEMU is always killed before returning, including on the error paths — a
/// leaked emulator holding a disk image is a genuinely annoying thing to debug.
pub fn boot_and_capture(
    iso: &Path,
    arch: Arch,
    timeout: Duration,
    sentinel: &str,
) -> Result<BootLog> {
    // QEMU writes the guest's serial console straight to a file, and we poll it.
    //
    // The obvious design — pipe QEMU's stdout and read it on a thread — is what
    // this replaced, after it wedged with no output and no QEMU process left to
    // inspect. A file has no pipe buffer to fill, needs no reader thread to stay
    // alive, and leaves the whole boot log on disk afterwards for debugging a
    // failure. Driving QEMU by hand this way worked first time, every time.
    let log_path = std::env::temp_dir().join(format!("alpymist-boot-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log_path);

    let mut child = Command::new(arch.qemu_binary())
        .args(arch.machine_args())
        .args([
            "-m",
            "2048",
            "-smp",
            "2",
            "-display",
            "none",
            "-no-reboot",
            "-boot",
            "d",
        ])
        .arg("-serial")
        .arg(format!("file:{}", log_path.display()))
        .arg("-cdrom")
        .arg(iso)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", arch.qemu_binary()))?;

    let started = Instant::now();
    let mut timed_out = false;
    loop {
        if read_log(&log_path).iter().any(|l| l.contains(sentinel)) {
            break;
        }
        // A guest that panics or powers off is a finished boot, not a timeout.
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    reap(&mut child);

    Ok(BootLog {
        lines: read_log(&log_path),
        elapsed: started.elapsed(),
        timed_out,
        log_path,
    })
}

/// Read the serial log, tolerating the file not existing yet.
///
/// Serial output carries CRLF; the trailing carriage returns are stripped so
/// that callers matching on line content do not have to know that.
fn read_log(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

/// Kill QEMU and reap it, without ever blocking forever.
///
/// `wait()` alone can hang if the child is wedged, which turns a failed boot
/// into a hung build rather than a test failure. This gives it a bounded
/// window and then gives up, leaving the caller free to report what it saw.
fn reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    eprintln!("warning: QEMU did not exit after SIGKILL; continuing anyway");
}

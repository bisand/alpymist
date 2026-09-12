//! What we managed to learn about the machine.

use serde::{Deserialize, Serialize};

/// A DRM device exposed by the kernel, as seen under `/sys/class/drm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDevice {
    /// Card node name, e.g. `card0`.
    pub card: String,
    /// Bound kernel driver, e.g. `i915`, `amdgpu`, `virtio_gpu`, `simpledrm`.
    pub driver: Option<String>,
    /// True when at least one connector reports `connected`.
    pub has_connected_output: bool,
}

impl GpuDevice {
    /// Whether this is a real accelerated driver rather than a firmware
    /// framebuffer shim.
    ///
    /// `simpledrm` / `efifb` / `vesafb` give you a scanout buffer and nothing
    /// else: modesetting works, GPU rendering does not.
    #[must_use]
    pub fn is_accelerated(&self) -> bool {
        !matches!(
            self.driver.as_deref(),
            None | Some("simpledrm" | "efifb" | "vesafb" | "offb")
        )
    }
}

/// Detected virtualisation flavour. Influences which GPU stack we trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Virtualisation {
    /// Running on physical hardware, as far as we can tell.
    Bare,
    /// QEMU/KVM, typically with `virtio_gpu`.
    Kvm,
    /// `VMware` / `VirtualBox` / Parallels / Hyper-V and friends.
    Other,
}

/// Everything the tier decision is allowed to depend on.
///
/// Kept deliberately small and plain so that [`crate::select_tier`] stays pure
/// and exhaustively testable without touching the filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Total usable RAM in mebibytes, from `MemTotal`.
    pub memory_mib: u64,
    /// Number of logical CPUs.
    pub cpus: usize,
    /// DRM devices found on the system.
    pub gpus: Vec<GpuDevice>,
    /// OpenGL ES version reported by an EGL probe, as `(major, minor)`.
    ///
    /// `None` means "not probed or probe failed", which we treat as
    /// "assume no acceleration" rather than optimistically guessing.
    pub gles_version: Option<(u32, u32)>,
    /// Detected virtualisation.
    pub virtualisation: Virtualisation,
}

impl Capabilities {
    /// The first DRM device that can actually render, if any.
    #[must_use]
    pub fn accelerated_gpu(&self) -> Option<&GpuDevice> {
        self.gpus.iter().find(|g| g.is_accelerated())
    }

    /// Whether the kernel can drive *any* display at all.
    ///
    /// If this is false we have no KMS, and a wlroots compositor has nothing to
    /// scan out to — that is the one case where legacy X11 is the answer.
    #[must_use]
    pub fn has_kms(&self) -> bool {
        !self.gpus.is_empty()
    }
}

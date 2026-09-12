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

/// What an EGL/GLES probe found on this machine.
///
/// The version alone is not enough to decide anything: Mesa's `llvmpipe`
/// software rasteriser advertises OpenGL ES 3.2, which would otherwise read as
/// a perfectly capable GPU. The renderer string is what distinguishes real
/// hardware from a CPU pretending to be one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlesInfo {
    /// OpenGL ES version as `(major, minor)`, parsed from `GL_VERSION`.
    pub version: (u32, u32),
    /// Raw `GL_RENDERER` string, e.g. `llvmpipe (LLVM 17.0.6, 256 bits)`.
    pub renderer: String,
    /// Raw `GL_VENDOR` string.
    pub vendor: String,
}

impl GlesInfo {
    /// Whether this is a CPU rasteriser rather than a GPU.
    ///
    /// Matches whole tokens rather than substrings: a bare `swr` substring
    /// would also fire on hardware renderer names that merely contain those
    /// letters. This is a denylist, so it can be wrong about a rasteriser we
    /// have never seen — it errs towards calling something hardware, which the
    /// version and RAM checks then still have to agree with.
    #[must_use]
    pub fn is_software(&self) -> bool {
        const SOFTWARE_RENDERERS: [&str; 6] = [
            "llvmpipe",
            "softpipe",
            "swrast",
            "swr",
            "lavapipe",
            "swiftshader",
        ];
        self.renderer
            .to_ascii_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| SOFTWARE_RENDERERS.contains(&token))
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
    /// What the EGL probe found, if it ran and succeeded.
    ///
    /// `None` means "not probed or probe failed", which we treat as
    /// "assume no acceleration" rather than optimistically guessing.
    pub gles: Option<GlesInfo>,
    /// Why the EGL probe produced nothing, when it produced nothing.
    ///
    /// Carried separately so the tier rationale can say *why* a machine was
    /// downgraded. "mesa-egl is not installed" and "this GPU has no working
    /// driver" both yield no `gles`, but call for very different responses.
    pub gles_error: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::GlesInfo;

    fn renderer(name: &str) -> GlesInfo {
        GlesInfo {
            version: (3, 2),
            renderer: name.into(),
            vendor: "Mesa".into(),
        }
    }

    #[test]
    fn known_cpu_rasterisers_are_recognised() {
        for name in [
            "llvmpipe (LLVM 17.0.6, 256 bits)",
            "softpipe",
            "SWR (LLVM 11)",
            "swrast",
            "lavapipe (LLVM 17.0.6, 256 bits)",
            "Google SwiftShader",
        ] {
            assert!(renderer(name).is_software(), "{name} should be software");
        }
    }

    #[test]
    fn real_gpus_are_not_flagged_as_software() {
        for name in [
            "AMD Radeon RX 580 (polaris10, LLVM 17.0.6, DRM 3.54)",
            "Mesa Intel(R) HD Graphics 620 (KBL GT2)",
            "NV137",
            "virgl (AMD Radeon RX 580)",
            "Mali-G72",
            "V3D 4.2",
        ] {
            assert!(!renderer(name).is_software(), "{name} should be hardware");
        }
    }

    #[test]
    fn matching_is_case_insensitive_and_token_based() {
        assert!(renderer("LLVMPIPE").is_software());
        // Substring-only matching would wrongly flag this.
        assert!(!renderer("Swrastite GPU 9000").is_software());
    }
}

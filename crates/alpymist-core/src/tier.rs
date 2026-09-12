//! Choosing a session backend for the hardware we are actually running on.

use crate::capabilities::Capabilities;
use serde::{Deserialize, Serialize};

/// Minimum RAM for the full compositing desktop.
const FULL_MIN_MEMORY_MIB: u64 = 3_072;
/// Minimum RAM for the accelerated lightweight desktop.
const LITE_MIN_MEMORY_MIB: u64 = 1_536;
/// Hyprland requires OpenGL ES 3.2 or better.
const FULL_MIN_GLES: (u32, u32) = (3, 2);
/// wlroots' GLES2 renderer requires OpenGL ES 2.0.
const LITE_MIN_GLES: (u32, u32) = (2, 0);

/// The concrete compositor/WM that a tier resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "backend")]
pub enum SessionBackend {
    /// Hyprland: animations, blur, tiling. Needs a real GPU.
    Hyprland,
    /// labwc: a stacking wlroots compositor, deliberately X11-like in feel.
    Labwc {
        /// Force `WLR_RENDERER=pixman`, i.e. CPU-only rendering.
        software_render: bool,
    },
    /// X11 with i3. Only for machines with no usable KMS.
    I3,
}

/// The capability tier of a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// No usable KMS: fall back to the X11 session.
    Legacy,
    /// KMS but no usable GPU rendering: labwc on the pixman software renderer.
    Potato,
    /// Modest GPU: labwc with GLES2 acceleration.
    Lite,
    /// Capable GPU and enough RAM: Hyprland.
    Full,
}

impl Tier {
    /// The session backend this tier runs.
    #[must_use]
    pub fn backend(self) -> SessionBackend {
        match self {
            Self::Full => SessionBackend::Hyprland,
            Self::Lite => SessionBackend::Labwc {
                software_render: false,
            },
            Self::Potato => SessionBackend::Labwc {
                software_render: true,
            },
            Self::Legacy => SessionBackend::I3,
        }
    }

    /// The Alpine metapackage that provides this tier's session.
    #[must_use]
    pub fn metapackage(self) -> &'static str {
        match self {
            Self::Full => "alpymist-desktop-full",
            Self::Lite | Self::Potato => "alpymist-desktop-lite",
            Self::Legacy => "alpymist-desktop-legacy",
        }
    }
}

/// Why a given tier was chosen — surfaced to the user and written to the
/// install log, so a surprising downgrade is always explainable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rationale {
    /// The selected tier.
    pub tier: Tier,
    /// Human-readable reasons, most significant first.
    pub reasons: Vec<String>,
}

/// Decide which desktop tier this machine should run.
///
/// Pure: it reads nothing but `caps`. Deliberately pessimistic — an unknown
/// value downgrades rather than upgrades, because a desktop that starts slowly
/// is recoverable and one that does not start at all is not.
#[must_use]
pub fn select_tier(caps: &Capabilities) -> Rationale {
    let mut reasons = Vec::new();

    if !caps.has_kms() {
        reasons.push("no DRM/KMS device found; Wayland has nothing to scan out to".into());
        return Rationale {
            tier: Tier::Legacy,
            reasons,
        };
    }

    let Some(gpu) = caps.accelerated_gpu() else {
        reasons.push("only a firmware framebuffer is available (no accelerated DRM driver)".into());
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    };
    reasons.push(format!(
        "accelerated DRM driver {} on {}",
        gpu.driver.as_deref().unwrap_or("unknown"),
        gpu.card
    ));

    let Some(gles) = caps.gles.as_ref() else {
        reasons.push(caps.gles_error.as_ref().map_or_else(
            || "EGL was not probed; assuming no acceleration".to_string(),
            |why| format!("EGL probe failed: {why}"),
        ));
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    };

    // Checked before the version, because llvmpipe advertises GL ES 3.2 and
    // would otherwise sail through every check below as a capable GPU.
    if gles.is_software() {
        reasons.push(format!(
            "renderer {:?} is a CPU rasteriser, not the GPU",
            gles.renderer
        ));
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    }

    let version = gles.version;
    if version < LITE_MIN_GLES {
        reasons.push(format!(
            "GL ES {}.{} is below the 2.0 minimum",
            version.0, version.1
        ));
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    }
    if caps.memory_mib < LITE_MIN_MEMORY_MIB {
        reasons.push(format!(
            "{} MiB RAM is below the {LITE_MIN_MEMORY_MIB} MiB needed for GPU compositing",
            caps.memory_mib
        ));
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    }

    if version < FULL_MIN_GLES {
        reasons.push(format!(
            "GL ES {}.{} is below the {}.{} Hyprland requires",
            version.0, version.1, FULL_MIN_GLES.0, FULL_MIN_GLES.1
        ));
        return Rationale {
            tier: Tier::Lite,
            reasons,
        };
    }
    if caps.memory_mib < FULL_MIN_MEMORY_MIB {
        reasons.push(format!(
            "{} MiB RAM is below the {FULL_MIN_MEMORY_MIB} MiB Hyprland wants",
            caps.memory_mib
        ));
        return Rationale {
            tier: Tier::Lite,
            reasons,
        };
    }

    reasons.push(format!(
        "GL ES {}.{} on {} with {} MiB RAM",
        version.0, version.1, gles.renderer, caps.memory_mib
    ));
    Rationale {
        tier: Tier::Full,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::{GlesInfo, GpuDevice, Virtualisation};

    /// Build capabilities with a hardware renderer at the given GL ES version.
    fn caps(driver: Option<&str>, gles: Option<(u32, u32)>, mib: u64) -> Capabilities {
        caps_with_renderer(driver, gles, mib, "AMD Radeon RX 580 (polaris10)")
    }

    fn caps_with_renderer(
        driver: Option<&str>,
        gles: Option<(u32, u32)>,
        mib: u64,
        renderer: &str,
    ) -> Capabilities {
        Capabilities {
            memory_mib: mib,
            cpus: 2,
            gpus: driver
                .map(|d| GpuDevice {
                    card: "card0".into(),
                    driver: Some(d.into()),
                    has_connected_output: true,
                })
                .into_iter()
                .collect(),
            gles: gles.map(|version| GlesInfo {
                version,
                renderer: renderer.into(),
                vendor: "Mesa".into(),
            }),
            gles_error: None,
            virtualisation: Virtualisation::Bare,
        }
    }

    #[test]
    fn no_drm_device_falls_back_to_x11() {
        let c = Capabilities {
            gpus: vec![],
            ..caps(Some("i915"), Some((3, 2)), 8192)
        };
        assert_eq!(select_tier(&c).tier, Tier::Legacy);
    }

    #[test]
    fn firmware_framebuffer_uses_software_rendering() {
        let r = select_tier(&caps(Some("simpledrm"), Some((3, 2)), 8192));
        assert_eq!(r.tier, Tier::Potato);
        assert_eq!(
            r.tier.backend(),
            SessionBackend::Labwc {
                software_render: true
            }
        );
    }

    #[test]
    fn unknown_gles_version_downgrades_rather_than_guesses() {
        assert_eq!(
            select_tier(&caps(Some("i915"), None, 8192)).tier,
            Tier::Potato
        );
    }

    /// The regression this whole `GlesInfo` type exists for: llvmpipe reports
    /// GL ES 3.2 and would otherwise be promoted to Hyprland on a machine with
    /// no working GPU at all.
    #[test]
    fn llvmpipe_is_not_mistaken_for_a_capable_gpu() {
        let c = caps_with_renderer(
            Some("i915"),
            Some((3, 2)),
            16384,
            "llvmpipe (LLVM 17.0.6, 256 bits)",
        );
        let r = select_tier(&c);
        assert_eq!(r.tier, Tier::Potato, "reasons: {:?}", r.reasons);
        assert!(r.reasons.iter().any(|s| s.contains("CPU rasteriser")));
    }

    #[test]
    fn other_software_rasterisers_are_caught_too() {
        for renderer in ["softpipe", "SWR (LLVM 11)", "swrast", "lavapipe (LLVM 17)"] {
            let c = caps_with_renderer(Some("i915"), Some((3, 2)), 16384, renderer);
            assert_eq!(
                select_tier(&c).tier,
                Tier::Potato,
                "renderer {renderer} slipped through"
            );
        }
    }

    #[test]
    fn old_gpu_with_gles2_gets_accelerated_lite() {
        let r = select_tier(&caps(Some("i915"), Some((2, 0)), 2048));
        assert_eq!(r.tier, Tier::Lite);
        assert_eq!(
            r.tier.backend(),
            SessionBackend::Labwc {
                software_render: false
            }
        );
    }

    #[test]
    fn capable_gpu_starved_of_ram_stays_on_lite() {
        assert_eq!(
            select_tier(&caps(Some("amdgpu"), Some((3, 2)), 2048)).tier,
            Tier::Lite
        );
    }

    #[test]
    fn tiny_ram_forces_software_rendering_even_with_a_good_gpu() {
        assert_eq!(
            select_tier(&caps(Some("amdgpu"), Some((3, 2)), 1024)).tier,
            Tier::Potato
        );
    }

    #[test]
    fn modern_machine_gets_hyprland() {
        let r = select_tier(&caps(Some("amdgpu"), Some((3, 2)), 16384));
        assert_eq!(r.tier, Tier::Full);
        assert_eq!(r.tier.backend(), SessionBackend::Hyprland);
        assert_eq!(r.tier.metapackage(), "alpymist-desktop-full");
    }

    #[test]
    fn virtio_gpu_in_a_vm_is_treated_as_accelerated() {
        let c = caps_with_renderer(Some("virtio_gpu"), Some((3, 2)), 4096, "virgl (AMD Radeon)");
        assert_eq!(select_tier(&c).tier, Tier::Full);
    }

    #[test]
    fn every_tier_reports_at_least_one_reason() {
        for c in [
            caps(None, None, 512),
            caps(Some("simpledrm"), None, 512),
            caps(Some("i915"), Some((2, 0)), 2048),
            caps(Some("amdgpu"), Some((3, 2)), 16384),
        ] {
            assert!(!select_tier(&c).reasons.is_empty());
        }
    }
}

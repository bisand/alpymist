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
            Self::Full => "alpy-desktop-full",
            Self::Lite | Self::Potato => "alpy-desktop-lite",
            Self::Legacy => "alpy-desktop-legacy",
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

    let Some(gles) = caps.gles_version else {
        reasons.push("EGL probe did not report a GL ES version; assuming none".into());
        return Rationale {
            tier: Tier::Potato,
            reasons,
        };
    };

    if gles < LITE_MIN_GLES {
        reasons.push(format!(
            "GL ES {}.{} is below the 2.0 minimum",
            gles.0, gles.1
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

    if gles < FULL_MIN_GLES {
        reasons.push(format!(
            "GL ES {}.{} is below the {}.{} Hyprland requires",
            gles.0, gles.1, FULL_MIN_GLES.0, FULL_MIN_GLES.1
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
        "GL ES {}.{} and {} MiB RAM",
        gles.0, gles.1, caps.memory_mib
    ));
    Rationale {
        tier: Tier::Full,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::{GpuDevice, Virtualisation};

    fn caps(driver: Option<&str>, gles: Option<(u32, u32)>, mib: u64) -> Capabilities {
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
            gles_version: gles,
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
        assert_eq!(r.tier.metapackage(), "alpy-desktop-full");
    }

    #[test]
    fn virtio_gpu_in_a_vm_is_treated_as_accelerated() {
        let r = select_tier(&caps(Some("virtio_gpu"), Some((3, 2)), 4096));
        assert_eq!(r.tier, Tier::Full);
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

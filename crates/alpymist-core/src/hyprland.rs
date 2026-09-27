//! Whether this machine can run Alpymist's desktop, which is Hyprland.
//!
//! There is one desktop (ADR 0001's addendum of 2026-09-27), so the question
//! is no longer which desktop suits the hardware, but whether Hyprland will
//! start on it and how well. Nothing refuses on the answer: the installer
//! warns and lets the person go on, since the check is cautious and a machine
//! it doubts may well run Hyprland.

use crate::capabilities::Capabilities;
use serde::{Deserialize, Serialize};

/// Below this much memory, Hyprland runs but the desktop and a browser
/// together crowd it. The Asus E200HA, with 2 GiB, runs it.
const COMFORTABLE_MEMORY_MIB: u64 = 3_072;
/// Hyprland requires OpenGL ES 3.2 or better.
const MIN_GLES: (u32, u32) = (3, 2);

/// How Hyprland will do here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// It probably will not start: no display to drive, or no GPU driver
    /// that can give it GL ES 3.2.
    Unlikely,
    /// It will run, but the processor draws every frame, or memory is short.
    Slow,
    /// It will run as it is meant to.
    Runs,
}

impl Verdict {
    /// As a person reads it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Runs => "Hyprland runs well here",
            Self::Slow => "Hyprland runs here, but slowly",
            Self::Unlikely => "Hyprland will probably not start here",
        }
    }
}

/// The answer, and why: shown to the person and written to the install log,
/// so a desktop that will not start is always explained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// How Hyprland will do.
    pub verdict: Verdict,
    /// Human-readable reasons, most significant first.
    pub reasons: Vec<String>,
}

/// Decide how Hyprland will do on a machine with `caps`.
///
/// Pure: it reads nothing but `caps`. Cautious: what cannot be known counts
/// against, because "it may not start" said wrongly costs a warning, and "it
/// runs" said wrongly costs a login that never arrives.
#[must_use]
pub fn check(caps: &Capabilities) -> Check {
    let mut reasons = Vec::new();
    let unlikely = |reasons| Check {
        verdict: Verdict::Unlikely,
        reasons,
    };

    if !caps.has_kms() {
        reasons.push("no DRM/KMS device found; Wayland has nothing to scan out to".into());
        return unlikely(reasons);
    }

    let Some(gpu) = caps.accelerated_gpu() else {
        reasons.push("only a firmware framebuffer is available (no accelerated DRM driver)".into());
        return unlikely(reasons);
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
        return unlikely(reasons);
    };

    let mut verdict = Verdict::Runs;
    // Checked before the version, because llvmpipe advertises GL ES 3.2: it
    // runs Hyprland, as the dev VM does, but on the processor.
    if gles.is_software() {
        reasons.push(format!(
            "renderer {:?} is a CPU rasteriser, not the GPU",
            gles.renderer
        ));
        verdict = Verdict::Slow;
    } else if gles.version < MIN_GLES {
        reasons.push(format!(
            "GL ES {}.{} is below the {}.{} Hyprland requires",
            gles.version.0, gles.version.1, MIN_GLES.0, MIN_GLES.1
        ));
        return unlikely(reasons);
    }
    if caps.memory_mib < COMFORTABLE_MEMORY_MIB {
        reasons.push(format!(
            "{} MiB RAM is below the {COMFORTABLE_MEMORY_MIB} MiB it is comfortable in",
            caps.memory_mib
        ));
        verdict = Verdict::Slow;
    }
    if verdict == Verdict::Runs {
        reasons.push(format!(
            "GL ES {}.{} on {} with {} MiB RAM",
            gles.version.0, gles.version.1, gles.renderer, caps.memory_mib
        ));
    }
    Check { verdict, reasons }
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
    fn no_drm_device_cannot_run_it() {
        let c = Capabilities {
            gpus: vec![],
            ..caps(Some("i915"), Some((3, 2)), 8192)
        };
        assert_eq!(check(&c).verdict, Verdict::Unlikely);
    }

    #[test]
    fn a_firmware_framebuffer_is_doubted() {
        let r = check(&caps(Some("simpledrm"), Some((3, 2)), 8192));
        assert_eq!(r.verdict, Verdict::Unlikely);
    }

    #[test]
    fn unknown_gles_is_doubted_rather_than_guessed() {
        assert_eq!(
            check(&caps(Some("i915"), None, 8192)).verdict,
            Verdict::Unlikely
        );
    }

    /// llvmpipe reports GL ES 3.2, and Hyprland does run on it, as on the dev
    /// VM: but on the processor, which is slow, not impossible.
    #[test]
    fn llvmpipe_runs_it_slowly() {
        let c = caps_with_renderer(
            Some("virtio-pci"),
            Some((3, 2)),
            4096,
            "llvmpipe (LLVM 22.1.3, 128 bits)",
        );
        let r = check(&c);
        assert_eq!(r.verdict, Verdict::Slow, "reasons: {:?}", r.reasons);
        assert!(r.reasons.iter().any(|s| s.contains("CPU rasteriser")));
    }

    #[test]
    fn other_software_rasterisers_are_caught_too() {
        for renderer in ["softpipe", "SWR (LLVM 11)", "swrast", "lavapipe (LLVM 17)"] {
            let c = caps_with_renderer(Some("i915"), Some((3, 2)), 16384, renderer);
            assert_eq!(check(&c).verdict, Verdict::Slow, "renderer {renderer}");
        }
    }

    #[test]
    fn a_gpu_below_gles_3_2_cannot_run_it() {
        let r = check(&caps(Some("i915"), Some((2, 0)), 8192));
        assert_eq!(r.verdict, Verdict::Unlikely);
        assert!(r.reasons.iter().any(|s| s.contains("3.2")));
    }

    /// The Asus E200HA: a capable GPU and 2 GiB, and Hyprland runs on it.
    #[test]
    fn little_memory_is_slow_not_impossible() {
        assert_eq!(
            check(&caps(Some("i915"), Some((3, 2)), 1900)).verdict,
            Verdict::Slow
        );
    }

    #[test]
    fn a_modern_machine_runs_it() {
        let r = check(&caps(Some("amdgpu"), Some((3, 2)), 16384));
        assert_eq!(r.verdict, Verdict::Runs);
    }

    #[test]
    fn virtio_gpu_with_virgl_runs_it() {
        let c = caps_with_renderer(Some("virtio_gpu"), Some((3, 2)), 4096, "virgl (AMD Radeon)");
        assert_eq!(check(&c).verdict, Verdict::Runs);
    }

    #[test]
    fn every_verdict_gives_at_least_one_reason() {
        for c in [
            caps(None, None, 512),
            caps(Some("simpledrm"), None, 512),
            caps(Some("i915"), Some((2, 0)), 2048),
            caps(Some("amdgpu"), Some((3, 2)), 16384),
        ] {
            assert!(!check(&c).reasons.is_empty());
        }
    }
}

//! Checking that the password prompt on screen is Alpymist's.
//!
//! A program running as the user could draw something that looks like the
//! prompt. What it cannot do is run as `/usr/bin/alpymist-auth`, which root
//! owns: and a prompt that is that binary hands the password to polkit's
//! helper and nowhere else, whoever started it. So "is this prompt genuine?"
//! comes down to "is every overlay on screen that binary?", and the
//! compositor knows which process drew each overlay.
//!
//! Ctrl+Alt+Delete is bound in the compositor, where no program sees it, to
//! `alpymist-auth attention`. That asks Hyprland for its layers, checks the
//! process behind every overlay, and has Hyprland itself say what it found.
//! A genuine prompt is also told, so it can show that it was checked.
//!
//! What this cannot stop: a program that rebinds Ctrl+Alt+Delete through
//! Hyprland's IPC socket, which every program of the user's can reach. It
//! stops a lookalike drawn by anything that does not know to do that.

use serde_json::Value;
use std::path::PathBuf;

/// The prompt's binary, owned by root.
pub const GENUINE: &str = "/usr/bin/alpymist-auth";

/// The layer namespace the prompt draws under.
pub const NAMESPACE: &str = "alpymist-auth";

/// Hyprland's overlay level: the one above everything, where the prompt is.
const OVERLAY: &str = "3";

/// An overlay on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// The namespace it claims.
    pub namespace: String,
    /// The process that drew it.
    pub pid: u32,
}

/// What the check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No prompt, and nothing pretending to be one.
    Nothing,
    /// Alpymist's prompt, with nothing drawn over it; its pids.
    Genuine(Vec<u32>),
    /// Something that is not Alpymist's prompt is on top.
    Impostor,
}

impl Verdict {
    /// What Hyprland is asked to say.
    #[must_use]
    pub fn sentence(&self) -> &'static str {
        match self {
            Self::Nothing => {
                "No Alpymist password prompt is open: anything asking for your password now is not ours"
            }
            Self::Genuine(_) => {
                "This is Alpymist's password prompt: it is safe to type your password"
            }
            Self::Impostor => {
                "This is NOT Alpymist's password prompt: do not type your password, and press Escape"
            }
        }
    }
}

/// The overlays in `hyprctl -j layers` output, on every output.
#[must_use]
pub fn overlays(layers: &Value) -> Vec<Overlay> {
    let Some(outputs) = layers.as_object() else {
        return Vec::new();
    };
    outputs
        .values()
        .filter_map(|output| output.get("levels")?.get(OVERLAY)?.as_array())
        .flatten()
        .filter_map(|layer| {
            Some(Overlay {
                namespace: layer.get("namespace")?.as_str()?.to_owned(),
                pid: u32::try_from(layer.get("pid")?.as_u64()?).ok()?,
            })
        })
        .collect()
}

/// Judge the overlays, given what each process runs.
///
/// An overlay claiming the prompt's name must be the prompt's binary, and
/// one claiming any Alpymist program's name that program's. Beside a genuine
/// prompt nothing else may be drawn over the screen. With no genuine prompt
/// open, the verdict is that none is — which is the answer to a lookalike,
/// whatever name it drew under.
pub fn judge(overlays: &[Overlay], exe: impl Fn(u32) -> Option<PathBuf>) -> Verdict {
    let mut genuine = Vec::new();
    let mut others = false;
    for o in overlays {
        let runs = exe(o.pid);
        if o.namespace.starts_with("alpymist-") {
            let expected = PathBuf::from(format!("/usr/bin/{}", o.namespace));
            if runs.as_deref() != Some(expected.as_path()) {
                return Verdict::Impostor;
            }
            if o.namespace == NAMESPACE {
                genuine.push(o.pid);
            }
        } else {
            others = true;
        }
    }
    match (genuine.is_empty(), others) {
        (true, _) => Verdict::Nothing,
        (false, true) => Verdict::Impostor,
        (false, false) => Verdict::Genuine(genuine),
    }
}

/// What process `pid` runs. Readable for the user's own processes, which are
/// the only ones that can draw on the user's session.
#[must_use]
pub fn exe(pid: u32) -> Option<PathBuf> {
    let link = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    // A binary replaced by an upgrade while running reads "… (deleted)";
    // only root could have put anything at that path.
    let text = link.to_string_lossy();
    Some(
        text.strip_suffix(" (deleted)")
            .map_or(link.clone(), PathBuf::from),
    )
}

/// Where a running prompt listens to be told it was checked.
#[must_use]
pub fn socket(pid: u32) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    Some(PathBuf::from(dir).join(format!("alpymist-auth-{pid}.sock")))
}

#[cfg(test)]
mod tests {
    use super::{GENUINE, NAMESPACE, Overlay, Verdict, judge, overlays};
    use std::path::PathBuf;

    fn o(namespace: &str, pid: u32) -> Overlay {
        Overlay {
            namespace: namespace.into(),
            pid,
        }
    }

    /// pid 1 runs the genuine prompt; everything else something else.
    #[allow(clippy::unnecessary_wraps)] // the shape `judge` takes
    fn exe(pid: u32) -> Option<PathBuf> {
        Some(PathBuf::from(if pid == 1 {
            GENUINE
        } else {
            "/home/me/.local/bin/evil"
        }))
    }

    #[test]
    fn the_real_prompt_passes_and_a_lookalike_does_not() {
        assert_eq!(judge(&[o(NAMESPACE, 1)], exe), Verdict::Genuine(vec![1]));
        assert_eq!(
            judge(&[o(NAMESPACE, 7)], exe),
            Verdict::Impostor,
            "same name, wrong binary"
        );
        assert_eq!(
            judge(&[o(NAMESPACE, 1), o("fake-dialog", 7)], exe),
            Verdict::Impostor,
            "something drawn beside the real one"
        );
        assert_eq!(
            judge(&[o("alpymist-power", 7)], exe),
            Verdict::Impostor,
            "a popup's name, not its binary"
        );
        assert_eq!(
            judge(&[o("fake-dialog", 7)], exe),
            Verdict::Nothing,
            "no prompt of ours is open, whatever is drawn"
        );
        assert_eq!(judge(&[], exe), Verdict::Nothing);
    }

    #[test]
    fn overlays_are_read_from_every_output() {
        let layers = serde_json::json!({
            "DP-1": { "levels": { "0": [{ "namespace": "wallpaper", "pid": 5 }],
                                  "3": [{ "namespace": "alpymist-auth", "pid": 1 }] } },
            "HDMI-A-1": { "levels": { "3": [{ "namespace": "alpymist-auth", "pid": 1 }] } }
        });
        assert_eq!(overlays(&layers), [o(NAMESPACE, 1), o(NAMESPACE, 1)]);
    }
}

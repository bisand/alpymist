//! Guest graphics: a Mesa that can use the 3D a virtual machine is offered.
//!
//! Alpine 3.24 builds Mesa without `virgl`, the driver for virtio-gpu's 3D, so
//! a guest under QEMU or UTM draws every frame on the processor. A second
//! repository carries Alpine's Mesa with that driver added, and a system
//! follows it only when asked to: the line below in `/etc/apk/repositories`,
//! and the key its index is signed with (ADR 0018).

use crate::capabilities::Capabilities;

/// The line `/etc/apk/repositories` has for it.
pub const REPOSITORY: &str = "https://guest.pkgs.alpymist.org/v3.24/guest";

/// The public key its index is signed with. `alpymist-keys` installs it to
/// [`crate::Channel::SHIPPED_KEYS`], and it is copied to `/etc/apk/keys` only
/// while the system follows the repository, because apk trusts every key
/// there for every repository.
pub const KEY: &str = "alpymist-guest-2026.rsa.pub";

/// Whether this machine is one the repository is for: a virtual machine whose
/// display is virtio-gpu. Whether the host offers 3D through it cannot be
/// told from here; where it does not, Mesa draws on the processor exactly as
/// Alpine's does.
#[must_use]
pub fn offered(caps: &Capabilities) -> bool {
    caps.gpus.iter().any(|g| {
        // The card's device is bound to virtio-pci on a PCI machine, which is
        // what sysfs names; virtio_gpu is the DRM driver above it.
        matches!(g.driver.as_deref(), Some("virtio_gpu" | "virtio-pci"))
    })
}

/// Whether a repositories file follows it.
#[must_use]
pub fn follows(text: &str) -> bool {
    text.lines().any(is_line)
}

/// `text` following it, or not: its line appended when missing, or every one
/// of its lines dropped. Everything else is left as it was.
#[must_use]
pub fn rewrite_repositories(text: &str, on: bool) -> String {
    let mut out = String::with_capacity(text.len() + REPOSITORY.len() + 1);
    for line in text.lines().filter(|l| on || !is_line(l)) {
        out.push_str(line);
        out.push('\n');
    }
    if on && !follows(text) {
        out.push_str(REPOSITORY);
        out.push('\n');
    }
    out
}

/// Whether one line of a repositories file names it, and is not commented out.
fn is_line(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let Some(mut url) = words.next() else {
        return false;
    };
    if url.starts_with('@') {
        let Some(next) = words.next() else {
            return false;
        };
        url = next;
    }
    url.trim_end_matches('/') == REPOSITORY
}

#[cfg(test)]
mod tests {
    use super::{KEY, REPOSITORY, follows, offered, rewrite_repositories};
    use crate::capabilities::{Capabilities, GpuDevice, Virtualisation};
    use crate::{Channel, guest};

    const INSTALLED: &str = "https://dl-cdn.alpinelinux.org/alpine/v3.24/main\n\
                             https://dl-cdn.alpinelinux.org/alpine/v3.24/community\n\
                             https://pkgs.alpymist.org/v3.24/alpymist\n";

    fn machine(driver: Option<&str>) -> Capabilities {
        Capabilities {
            memory_mib: 4096,
            cpus: 4,
            gpus: vec![GpuDevice {
                card: "card0".into(),
                driver: driver.map(str::to_owned),
                has_connected_output: true,
            }],
            gles: None,
            gles_error: None,
            virtualisation: Virtualisation::Kvm,
        }
    }

    #[test]
    fn it_is_offered_where_the_display_is_virtio_gpu() {
        // What sysfs says on UTM and QEMU's virt machine, and on a bus where
        // the DRM driver is the one bound.
        assert!(offered(&machine(Some("virtio-pci"))));
        assert!(offered(&machine(Some("virtio_gpu"))));
        for other in [
            Some("i915"),
            Some("amdgpu"),
            Some("simpledrm"),
            Some("vmwgfx"),
            None,
        ] {
            assert!(!offered(&machine(other)), "{other:?}");
        }
    }

    #[test]
    fn an_installed_system_does_not_follow_it() {
        assert!(!follows(INSTALLED));
    }

    #[test]
    fn turning_it_on_adds_one_line_and_off_takes_it_away() {
        let on = rewrite_repositories(INSTALLED, true);
        assert_eq!(on, format!("{INSTALLED}{REPOSITORY}\n"));
        assert!(follows(&on));
        assert_eq!(rewrite_repositories(&on, true), on, "twice is once");
        assert_eq!(rewrite_repositories(&on, false), INSTALLED);
        assert_eq!(rewrite_repositories(INSTALLED, false), INSTALLED);
    }

    #[test]
    fn a_commented_line_is_not_followed_and_is_kept() {
        let text = format!("#{REPOSITORY}\n");
        assert!(!follows(&text));
        assert_eq!(rewrite_repositories(&text, false), text);
        assert_eq!(
            rewrite_repositories(&text, true),
            format!("{text}{REPOSITORY}\n")
        );
    }

    #[test]
    fn a_tagged_line_and_a_trailing_slash_are_still_it() {
        let text = format!("x\n@guest {REPOSITORY}/\n");
        assert!(follows(&text));
        assert_eq!(rewrite_repositories(&text, false), "x\n");
    }

    /// The channel and this are separate lines: changing one leaves the other.
    #[test]
    fn the_channel_and_the_guest_repository_do_not_touch_each_other() {
        let on = rewrite_repositories(INSTALLED, true);
        let dev = Channel::Dev.rewrite_repositories(&on);
        assert!(guest::follows(&dev));
        assert_eq!(Channel::of_repositories(&dev), Some(Channel::Dev));
        assert_eq!(
            Channel::of_repositories(&rewrite_repositories(&dev, false)),
            Some(Channel::Dev)
        );
        assert!(
            Channel::ALL
                .into_iter()
                .all(|c| c.repository() != REPOSITORY)
        );
    }

    /// The repository is for the Alpine release the channels are for.
    #[test]
    fn it_is_for_the_same_alpine_release_as_the_channels() {
        assert!(REPOSITORY.contains("/v3.24/"));
        assert!(Channel::Stable.repository().contains("/v3.24/"));
    }

    /// `alpymist guest on` copies the key from where the package puts it,
    /// which is not where apk looks.
    #[test]
    fn alpymist_keys_ships_the_guest_key_outside_apks_keys() {
        let apkbuild = include_str!("../../../aports/alpymist-keys/APKBUILD");
        assert!(apkbuild.contains(&format!("\"$pkgdir\"{}/{KEY}", Channel::SHIPPED_KEYS)));
        assert!(!apkbuild.contains(&format!("\"$pkgdir\"/etc/apk/keys/{KEY}")));
        assert!(
            include_str!("../../../aports/alpymist-keys/alpymist-guest-2026.rsa.pub")
                .starts_with("-----BEGIN PUBLIC KEY-----")
        );
    }
}

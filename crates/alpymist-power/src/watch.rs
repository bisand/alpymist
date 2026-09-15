//! Waking when something changes, rather than on a timer alone.
//!
//! The kernel announces a charger plugged in, and most batteries' changes of
//! level, as uevents on a netlink socket any user may read; the popup saving
//! the configuration is a file closed in `~/.config/alpymist`. Both wake a
//! wait here at once. Neither covers everything — some firmware changes the
//! level without a word — so a wait also ends after its timeout.

use std::path::Path;
use std::time::Duration;

/// What wakes a wait.
pub struct Changes {
    #[cfg(target_os = "linux")]
    uevents: Option<std::os::fd::OwnedFd>,
    #[cfg(target_os = "linux")]
    inotify: Option<nix::sys::inotify::Inotify>,
}

impl Changes {
    /// Listen for power supply uevents, and for files written in
    /// `config_dir`, where it exists.
    #[must_use]
    pub fn new(config_dir: Option<&Path>) -> Self {
        #[cfg(target_os = "linux")]
        {
            Self {
                uevents: uevents(),
                inotify: config_dir.and_then(watch_dir),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = config_dir;
            Self {}
        }
    }

    /// Wait for a change, or `timeout`. Returns whether something changed.
    ///
    /// A change wakes the wait; a moment is then allowed for the rest of a
    /// burst — a charger plugged in sends several — and all of it is taken.
    #[cfg(target_os = "linux")]
    #[allow(clippy::must_use_candidate)]
    pub fn wait(&self, timeout: Duration) -> bool {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        use std::os::fd::{AsFd, AsRawFd};

        let mut fds = Vec::new();
        if let Some(u) = &self.uevents {
            fds.push(PollFd::new(u.as_fd(), PollFlags::POLLIN));
        }
        if let Some(i) = &self.inotify {
            fds.push(PollFd::new(i.as_fd(), PollFlags::POLLIN));
        }
        if fds.is_empty() {
            std::thread::sleep(timeout);
            return false;
        }
        let limit = PollTimeout::try_from(timeout).unwrap_or(PollTimeout::MAX);
        if poll(&mut fds, limit).unwrap_or(0) <= 0 {
            return false;
        }
        drop(fds);
        std::thread::sleep(Duration::from_millis(150));
        let mut changed = false;
        if let Some(u) = &self.uevents {
            let mut buf = [0u8; 4096];
            while let Ok(n) = nix::sys::socket::recv(
                u.as_raw_fd(),
                &mut buf,
                nix::sys::socket::MsgFlags::MSG_DONTWAIT,
            ) {
                if n == 0 {
                    break;
                }
                changed |= is_power_supply(buf.get(..n).unwrap_or_default());
            }
        }
        if let Some(i) = &self.inotify {
            while let Ok(events) = i.read_events() {
                if events.is_empty() {
                    break;
                }
                changed = true;
            }
        }
        changed
    }

    /// Wait for `timeout`: without Linux there is nothing to listen to.
    #[cfg(not(target_os = "linux"))]
    #[allow(clippy::must_use_candidate)]
    pub fn wait(&self, timeout: Duration) -> bool {
        std::thread::sleep(timeout);
        false
    }
}

/// Whether a uevent message is about a power supply. Messages are
/// NUL-separated `KEY=value` lines after an `action@devpath` header.
#[must_use]
pub fn is_power_supply(message: &[u8]) -> bool {
    message
        .split(|b| *b == 0)
        .any(|field| field == b"SUBSYSTEM=power_supply")
}

#[cfg(target_os = "linux")]
fn uevents() -> Option<std::os::fd::OwnedFd> {
    use nix::sys::socket::{
        AddressFamily, NetlinkAddr, SockFlag, SockProtocol, SockType, bind, socket,
    };
    use std::os::fd::AsRawFd;
    let fd = socket(
        AddressFamily::Netlink,
        SockType::Datagram,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        SockProtocol::NetlinkKObjectUEvent,
    )
    .ok()?;
    // Group 1 is the kernel's own broadcast; udev's rebroadcast is another.
    bind(fd.as_raw_fd(), &NetlinkAddr::new(0, 1)).ok()?;
    Some(fd)
}

#[cfg(target_os = "linux")]
fn watch_dir(dir: &Path) -> Option<nix::sys::inotify::Inotify> {
    use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
    let inotify = Inotify::init(InitFlags::IN_CLOEXEC | InitFlags::IN_NONBLOCK).ok()?;
    std::fs::create_dir_all(dir).ok()?;
    inotify
        .add_watch(
            dir,
            AddWatchFlags::IN_CLOSE_WRITE | AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_DELETE,
        )
        .ok()?;
    Some(inotify)
}

#[cfg(test)]
mod tests {
    use super::is_power_supply;

    #[test]
    fn only_power_supply_messages_count() {
        assert!(is_power_supply(
            b"change@/devices/LNXSYSTM:00/ACPI0003:00/power_supply/AC\0ACTION=change\0SUBSYSTEM=power_supply\0"
        ));
        assert!(!is_power_supply(
            b"add@/devices/virtual/net/tun0\0ACTION=add\0SUBSYSTEM=net\0"
        ));
    }
}

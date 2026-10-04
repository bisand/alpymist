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
    inotify: Option<std::os::fd::OwnedFd>,
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
        use rustix::event::{PollFd, PollFlags, poll};
        use rustix::net::{RecvFlags, recv};

        let mut fds: Vec<PollFd<'_>> = [&self.uevents, &self.inotify]
            .into_iter()
            .flatten()
            .map(|fd| PollFd::new(fd, PollFlags::IN))
            .collect();
        if fds.is_empty() {
            std::thread::sleep(timeout);
            return false;
        }
        if poll(&mut fds, Some(&limit(timeout))).unwrap_or(0) == 0 {
            return false;
        }
        drop(fds);
        std::thread::sleep(Duration::from_millis(150));
        let mut changed = false;
        if let Some(u) = &self.uevents {
            let mut buf = [0u8; 4096];
            while let Ok((n, _)) = recv(u, &mut buf[..], RecvFlags::DONTWAIT) {
                if n == 0 {
                    break;
                }
                changed |= is_power_supply(buf.get(..n).unwrap_or_default());
            }
        }
        if let Some(i) = &self.inotify {
            // What was written is not looked at: that something was, is all
            // there is to know. The descriptor does not block.
            let mut events = [0u8; 4096];
            while rustix::io::read(i, &mut events[..]).is_ok_and(|n| n > 0) {
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

/// `timeout` as poll takes it.
#[cfg(target_os = "linux")]
fn limit(timeout: Duration) -> rustix::event::Timespec {
    rustix::event::Timespec {
        tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: timeout.subsec_nanos().into(),
    }
}

#[cfg(target_os = "linux")]
fn uevents() -> Option<std::os::fd::OwnedFd> {
    use rustix::net::netlink::{KOBJECT_UEVENT, SocketAddrNetlink};
    use rustix::net::{AddressFamily, SocketFlags, SocketType, bind, socket_with};
    let fd = socket_with(
        AddressFamily::NETLINK,
        SocketType::DGRAM,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        Some(KOBJECT_UEVENT),
    )
    .ok()?;
    // Group 1 is the kernel's own broadcast; udev's rebroadcast is another.
    bind(&fd, &SocketAddrNetlink::new(0, 1)).ok()?;
    Some(fd)
}

#[cfg(target_os = "linux")]
fn watch_dir(dir: &Path) -> Option<std::os::fd::OwnedFd> {
    use rustix::fs::inotify::{CreateFlags, WatchFlags, add_watch, init};
    let inotify = init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK).ok()?;
    std::fs::create_dir_all(dir).ok()?;
    add_watch(
        &inotify,
        dir,
        WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO | WatchFlags::DELETE,
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

    /// The kernel itself: a file written where the configuration is kept
    /// ends a wait, and nothing written does not.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_written_in_the_directory_ends_the_wait() {
        use super::Changes;
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("alpymist-power-{}", std::process::id()));
        let changes = Changes::new(Some(&dir));
        assert!(changes.inotify.is_some(), "the directory is not watched");
        assert!(changes.uevents.is_some(), "uevents are not listened to");
        assert!(!changes.wait(Duration::from_millis(50)));
        std::fs::write(dir.join("power.toml"), "mode = \"saver\"\n").unwrap();
        assert!(changes.wait(Duration::from_secs(5)));
        // Taken, all of it: the next wait is for the next change.
        assert!(!changes.wait(Duration::from_millis(50)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

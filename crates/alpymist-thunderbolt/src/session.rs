//! The session's side: whether its screen is locked, and waking when a
//! Thunderbolt device comes or goes.

use crate::policy::Session;
use std::path::Path;
use std::time::Duration;

/// The lock screen's process name ([ADR 0010](../../docs/adr/0010-lock-screen.md)).
const LOCK: &str = "alpymist-lock";

/// Whether this user's session is locked: `alpymist-lock` is running as
/// them. Read from `/proc` under `root`, so it holds under any compositor.
#[must_use]
pub fn state(root: &Path, uid: u32) -> Session {
    let Ok(entries) = std::fs::read_dir(root.join("proc")) else {
        return Session::Unlocked;
    };
    let locked = entries.filter_map(Result::ok).any(|e| {
        let dir = e.path();
        let is_pid = e
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit());
        is_pid
            && std::fs::read_to_string(dir.join("comm")).is_ok_and(|c| c.trim() == LOCK)
            && std::fs::read_to_string(dir.join("status")).is_ok_and(|s| real_uid(&s) == Some(uid))
    });
    if locked {
        Session::Locked
    } else {
        Session::Unlocked
    }
}

/// The real user id in a `/proc/<pid>/status`.
fn real_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// This process's real user id.
#[must_use]
pub fn own_uid() -> Option<u32> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| real_uid(&s))
}

/// Whether a uevent message is about the Thunderbolt bus. Messages are
/// NUL-separated `KEY=value` lines after an `action@devpath` header.
#[must_use]
pub fn is_thunderbolt(message: &[u8]) -> bool {
    message
        .split(|b| *b == 0)
        .any(|field| field == b"SUBSYSTEM=thunderbolt")
}

/// What wakes the watch: the kernel's uevents, which any user may read.
pub struct Changes {
    #[cfg(target_os = "linux")]
    uevents: Option<std::os::fd::OwnedFd>,
}

impl Changes {
    /// Listen for uevents.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            uevents: uevents(),
        }
    }

    /// Wait for a Thunderbolt uevent, or `timeout`. Returns whether one came.
    #[cfg(target_os = "linux")]
    #[allow(clippy::must_use_candidate)]
    pub fn wait(&self, timeout: Duration) -> bool {
        use rustix::event::{PollFd, PollFlags, Timespec, poll};
        use rustix::net::{RecvFlags, recv};

        let Some(u) = &self.uevents else {
            std::thread::sleep(timeout);
            return false;
        };
        let mut fds = [PollFd::new(u, PollFlags::IN)];
        let limit = Timespec {
            tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: timeout.subsec_nanos().into(),
        };
        if poll(&mut fds, Some(&limit)).unwrap_or(0) == 0 {
            return false;
        }
        // A dock arrives as a burst: the device, then what is behind it.
        std::thread::sleep(Duration::from_millis(300));
        let mut buf = [0u8; 4096];
        let mut changed = false;
        while let Ok((n, _)) = recv(u, &mut buf[..], RecvFlags::DONTWAIT) {
            if n == 0 {
                break;
            }
            changed |= is_thunderbolt(buf.get(..n).unwrap_or_default());
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

impl Default for Changes {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use super::{Session, is_thunderbolt, state};
    use crate::sysfs::tests::Fake;

    fn process(f: &Fake, pid: u32, comm: &str, uid: u32) {
        let dir = f.root().join("proc").join(pid.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        std::fs::write(
            dir.join("status"),
            format!("Name:\t{comm}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
        )
        .unwrap();
    }

    #[test]
    fn locked_while_our_lock_screen_runs() {
        let f = Fake::new("session");
        process(&f, 100, "Hyprland", 1000);
        assert_eq!(state(f.root(), 1000), Session::Unlocked);
        process(&f, 200, "alpymist-lock", 1000);
        assert_eq!(state(f.root(), 1000), Session::Locked);
        // Someone else's lock is not ours.
        assert_eq!(state(f.root(), 1001), Session::Unlocked);
    }

    #[test]
    fn only_thunderbolt_uevents_count() {
        assert!(is_thunderbolt(
            b"add@/devices/pci0000:00/0000:00:1c.4/domain0/0-0/0-1\0ACTION=add\0SUBSYSTEM=thunderbolt\0"
        ));
        assert!(!is_thunderbolt(
            b"change@/devices/LNXSYSTM:00/power_supply/AC\0SUBSYSTEM=power_supply\0"
        ));
    }
}

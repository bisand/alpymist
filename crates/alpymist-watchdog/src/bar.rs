//! Keeping the bar listening.
//!
//! The bar learns which workspace each screen shows from Hyprland's event
//! socket. Hyprland takes a listener that falls behind off that socket — one
//! whose sixty-four waiting events are still unread — and waybar, once taken
//! off, never connects again: it stays up, showing the workspaces as they
//! were, and a click on one changes the screen and not the bar.
//!
//! So the bars are looked at every so often. The kernel says which sockets
//! are on the other end of Hyprland's, and a bar that was listening and is
//! not, twice running, is ended and started again as it was started. A bar
//! that never listened — one of the account's own, with no Hyprland modules
//! — is left alone, as is everything when the kernel cannot be asked.

// Built off Linux only to be checked: what asks the kernel is left out
// there, and what it would have used is then used by the tests alone.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use rustix::process::{Pid, Signal, kill_process};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// How often the bars are looked at.
pub const EVERY: Duration = Duration::from_secs(20);
/// How many looks in a row a bar must have been found not listening: one
/// alone could be a bar caught between two connections.
const LOOKS: u8 = 2;
/// How long a bar is given to end before it is made to.
const GRACE: Duration = Duration::from_secs(2);

/// The name of the program that is the bar.
const BAR: &str = "waybar";

const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NLM_F_REQUEST: u16 = 0x1;
const NLM_F_DUMP: u16 = 0x300;
const NLM_F_DUMP_INTR: u16 = 0x10;
const SOCK_DIAG_BY_FAMILY: u16 = 20;
const AF_UNIX: u8 = 1;
const UDIAG_SHOW_NAME: u32 = 0x1;
const UDIAG_SHOW_PEER: u32 = 0x4;
const UNIX_DIAG_NAME: u16 = 0;
const UNIX_DIAG_PEER: u16 = 2;
/// A netlink message's header, and a unix socket's own: sixteen bytes each.
const HEADER: usize = 16;

/// What is known of each bar from one look to the next.
#[derive(Debug, Default)]
pub struct Bars {
    /// Each bar that has been seen listening, by process, and how many looks
    /// in a row it has not been since.
    seen: HashMap<u32, u8>,
}

impl Bars {
    /// Take in one look — each bar and whether it is listening — and say
    /// which are to be started again.
    pub fn look(&mut self, bars: &[(u32, bool)]) -> Vec<u32> {
        self.seen
            .retain(|pid, _| bars.iter().any(|(p, _)| p == pid));
        let mut lost = Vec::new();
        for &(pid, listening) in bars {
            if listening {
                self.seen.insert(pid, 0);
            } else if let Some(missed) = self.seen.get_mut(&pid) {
                *missed += 1;
                if *missed >= LOOKS {
                    lost.push(pid);
                }
            }
        }
        for pid in &lost {
            self.seen.remove(pid);
        }
        lost
    }

    /// Look at the bars, and start again each that has stopped listening to
    /// the event socket at `socket`. Whether Hyprland still has that socket;
    /// it is taken to when the kernel could not be asked.
    pub fn keep(&mut self, socket: &Path) -> bool {
        let Some(listeners) = listeners(socket) else {
            return true;
        };
        if !listeners.there {
            return false;
        }
        let bars: Vec<(u32, bool)> = bars()
            .into_iter()
            .map(|(pid, sockets)| (pid, sockets.iter().any(|s| listeners.peers.contains(s))))
            .collect();
        for pid in self.look(&bars) {
            match restart(pid) {
                Ok(()) => eprintln!(
                    "alpymist watchdog: the bar had stopped following Hyprland, and was started again"
                ),
                Err(e) => eprintln!("alpymist watchdog: the bar: {e}"),
            }
        }
        true
    }
}

/// Keep the bars listening until Hyprland ends.
///
/// # Errors
/// There is no Hyprland whose bars to keep.
pub fn run() -> Result<(), String> {
    let socket = alpymist_displays::watch::socket().ok_or("not in a Hyprland session")?;
    let mut bars = Bars::default();
    loop {
        std::thread::sleep(EVERY);
        if !bars.keep(&socket) {
            return Ok(());
        }
    }
}

/// What the kernel says of Hyprland's event socket.
#[derive(Debug, Default, PartialEq, Eq)]
struct Listeners {
    /// Whether any socket has its name: Hyprland's own, listening, if no
    /// other.
    there: bool,
    /// The socket at the other end of each connection to it, by inode.
    peers: Vec<u64>,
}

/// The request for every unix socket, with its name and the socket at its
/// other end.
fn request() -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + 24);
    out.extend_from_slice(&40u32.to_ne_bytes());
    out.extend_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
    out.extend_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
    out.extend_from_slice(&1u32.to_ne_bytes()); // sequence
    out.extend_from_slice(&0u32.to_ne_bytes()); // from: the kernel fills it in
    out.extend_from_slice(&[AF_UNIX, 0, 0, 0]); // family, protocol, padding
    out.extend_from_slice(&u32::MAX.to_ne_bytes()); // in any state
    out.extend_from_slice(&0u32.to_ne_bytes()); // any inode
    out.extend_from_slice(&(UDIAG_SHOW_NAME | UDIAG_SHOW_PEER).to_ne_bytes());
    out.extend_from_slice(&[0xff; 8]); // any cookie
    out
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// Up to the next multiple of four, as netlink lays things out.
const fn aligned(n: usize) -> usize {
    (n + 3) & !3
}

/// Read one datagram of the kernel's answer: each socket named `name` is
/// taken into `found`, with the socket at its other end. Whether that was the last of
/// the answer; `None` when it cannot be trusted — the kernel refused, the
/// sockets changed while it was listing them, or it is not laid out as
/// expected.
fn read(data: &[u8], name: &[u8], found: &mut Listeners) -> Option<bool> {
    let mut at = 0;
    while at < data.len() {
        let len = u32_at(data, at)? as usize;
        let kind = u16_at(data, at + 4)?;
        let flags = u16_at(data, at + 6)?;
        if len < HEADER || kind == NLMSG_ERROR || flags & NLM_F_DUMP_INTR != 0 {
            return None;
        }
        if kind == NLMSG_DONE {
            return Some(true);
        }
        let message = data.get(at..at + len)?;
        if kind == SOCK_DIAG_BY_FAMILY {
            let (mut named, mut peer) = (false, None);
            let mut a = HEADER + HEADER;
            while a + 4 <= message.len() {
                let alen = u16_at(message, a)? as usize;
                if alen < 4 {
                    return None;
                }
                let value = message.get(a + 4..a + alen)?;
                match u16_at(message, a + 2)? {
                    UNIX_DIAG_NAME => {
                        let end = value.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
                        named = &value[..end] == name;
                    }
                    UNIX_DIAG_PEER => peer = u32_at(value, 0),
                    _ => {}
                }
                a += aligned(alen);
            }
            found.there |= named;
            if let (true, Some(peer)) = (named, peer) {
                found.peers.push(u64::from(peer));
            }
        }
        at += aligned(len);
    }
    Some(false)
}

/// The socket at `socket` and those connected to it, as the kernel has them
/// now; `None` when it could not be asked.
#[cfg(target_os = "linux")]
fn listeners(socket: &Path) -> Option<Listeners> {
    use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketType, netlink};
    use std::os::unix::ffi::OsStrExt as _;
    let fd = rustix::net::socket(
        AddressFamily::NETLINK,
        SocketType::RAW,
        Some(netlink::SOCK_DIAG),
    )
    .ok()?;
    // A kernel that does not answer is one that was not asked.
    rustix::net::sockopt::set_socket_timeout(
        &fd,
        rustix::net::sockopt::Timeout::Recv,
        Some(Duration::from_secs(2)),
    )
    .ok()?;
    rustix::net::send(&fd, &request(), SendFlags::empty()).ok()?;
    let name = socket.as_os_str().as_bytes();
    let mut found = Listeners::default();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let (n, _) = rustix::net::recv(&fd, &mut buffer[..], RecvFlags::empty()).ok()?;
        if n == 0 || read(&buffer[..n], name, &mut found)? {
            return Some(found);
        }
    }
}

/// Only Linux's kernel can be asked: anywhere else, where this is built to
/// be checked and never run, it could not be.
#[cfg(not(target_os = "linux"))]
fn listeners(_socket: &Path) -> Option<Listeners> {
    None
}

/// Every bar of this account's that is running, with the inodes of the
/// sockets it holds.
fn bars() -> Vec<(u32, Vec<u64>)> {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    processes
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(entry.path().join("comm")).ok()?;
            if comm.trim_end() != BAR {
                return None;
            }
            // Another account's bar cannot be read, and is not ours to mind.
            let sockets = std::fs::read_dir(entry.path().join("fd"))
                .ok()?
                .filter_map(Result::ok)
                .filter_map(|fd| socket_inode(std::fs::read_link(fd.path()).ok()?.to_str()?))
                .collect();
            Some((pid, sockets))
        })
        .collect()
}

/// The inode in what `/proc` says an open socket is: `socket:[5734]`.
fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// One argument as the shell takes it whole, whatever is in it.
fn quoted(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// What to give Hyprland's `exec` to start a program with the arguments in
/// `cmdline`, which is as `/proc` keeps them: each ended by a zero byte.
fn command(cmdline: &[u8]) -> Option<String> {
    let args: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| std::str::from_utf8(a).map(quoted))
        .collect::<Result<_, _>>()
        .ok()?;
    (!args.is_empty()).then(|| args.join(" "))
}

/// End the bar that is process `pid`, and have Hyprland start it again with
/// the arguments it had.
fn restart(pid: u32) -> Result<(), String> {
    let proc = Path::new("/proc").join(pid.to_string());
    let command = std::fs::read(proc.join("cmdline"))
        .ok()
        .and_then(|c| command(&c))
        .ok_or("how it was started could not be read")?;
    let id = i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or("no such process")?;
    kill_process(id, Signal::TERM).map_err(|e| format!("could not be ended: {e}"))?;
    let asked = Instant::now();
    while proc.exists() {
        if asked.elapsed() > GRACE {
            let _ = kill_process(id, Signal::KILL);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    alpymist_displays::hypr::request(&format!("dispatch exec {command}")).map(drop)
}

#[cfg(test)]
mod tests {
    use super::{
        Bars, HEADER, Listeners, NLM_F_DUMP_INTR, NLMSG_DONE, NLMSG_ERROR, SOCK_DIAG_BY_FAMILY,
        UNIX_DIAG_NAME, UNIX_DIAG_PEER, aligned, command, read, request, socket_inode,
    };

    const SOCKET: &[u8] = b"/run/user/1000/hypr/x/.socket2.sock";

    fn attribute(kind: u16, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u16::try_from(4 + value.len()).unwrap().to_ne_bytes());
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(value);
        out.resize(aligned(out.len()), 0);
        out
    }

    fn message(kind: u16, flags: u16, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::try_from(HEADER + body.len()).unwrap().to_ne_bytes());
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(&flags.to_ne_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(body);
        out.resize(aligned(out.len()), 0);
        out
    }

    /// One unix socket as the kernel lists it.
    fn socket(name: Option<&[u8]>, peer: Option<u32>) -> Vec<u8> {
        let mut body = vec![0; HEADER];
        if let Some(name) = name {
            body.extend(attribute(UNIX_DIAG_NAME, name));
        }
        if let Some(peer) = peer {
            body.extend(attribute(UNIX_DIAG_PEER, &peer.to_ne_bytes()));
        }
        message(SOCK_DIAG_BY_FAMILY, 0, &body)
    }

    #[test]
    fn the_request_is_as_long_as_it_says() {
        let request = request();
        assert_eq!(request.len(), 40);
        assert_eq!(u32::from_ne_bytes(request[..4].try_into().unwrap()), 40);
    }

    #[test]
    fn the_other_end_of_each_connection_to_the_socket_is_found() {
        let mut answer = Vec::new();
        // Hyprland listening: the name, and nothing at the other end.
        answer.extend(socket(Some(SOCKET), None));
        // Its end of two connections.
        answer.extend(socket(Some(SOCKET), Some(7562)));
        answer.extend(socket(Some(SOCKET), Some(182_191)));
        // Sockets that are nothing to do with it.
        answer.extend(socket(Some(b"/run/dbus/system_bus_socket"), Some(9)));
        answer.extend(socket(None, Some(11)));
        let mut found = Listeners::default();
        assert_eq!(read(&answer, SOCKET, &mut found), Some(false));
        assert!(found.there);
        assert_eq!(found.peers, [7562, 182_191]);
        let done = message(NLMSG_DONE, 0, &[0; 4]);
        assert_eq!(read(&done, SOCKET, &mut found), Some(true));
        // Hyprland gone: nothing has the name.
        let mut found = Listeners::default();
        let others = socket(Some(b"/run/dbus/system_bus_socket"), Some(9));
        assert_eq!(read(&others, SOCKET, &mut found), Some(false));
        assert_eq!(found, Listeners::default());
    }

    #[test]
    fn an_answer_that_cannot_be_trusted_is_not() {
        let mut peers = Listeners::default();
        let refused = message(NLMSG_ERROR, 0, &[0; 20]);
        assert_eq!(read(&refused, SOCKET, &mut peers), None);
        let mut changed = socket(Some(SOCKET), Some(1));
        changed[6..8].copy_from_slice(&NLM_F_DUMP_INTR.to_ne_bytes());
        assert_eq!(read(&changed, SOCKET, &mut peers), None);
        let whole = socket(Some(SOCKET), Some(1));
        assert_eq!(read(&whole[..whole.len() - 6], SOCKET, &mut peers), None);
        assert_eq!(read(&[1, 0, 0, 0, 0, 0, 0, 0], SOCKET, &mut peers), None);
    }

    #[test]
    fn a_bar_is_started_again_only_after_it_listened_and_then_did_not_twice() {
        let mut bars = Bars::default();
        // A bar of the account's own that never listens is never touched.
        for _ in 0..5 {
            assert!(bars.look(&[(7, false)]).is_empty());
        }
        assert!(bars.look(&[(7, false), (8, true)]).is_empty());
        assert!(bars.look(&[(7, false), (8, false)]).is_empty(), "once");
        assert!(bars.look(&[(7, false), (8, true)]).is_empty());
        assert!(bars.look(&[(7, false), (8, false)]).is_empty(), "once");
        assert_eq!(bars.look(&[(7, false), (8, false)]), [8], "twice");
        // The one started in its place has yet to connect.
        assert!(bars.look(&[(7, false), (9, false)]).is_empty());
        assert!(bars.look(&[(7, false), (9, false)]).is_empty());
        // A bar turned off, and another with its number, starts from nothing.
        assert!(bars.look(&[(9, true)]).is_empty());
        assert!(bars.look(&[]).is_empty());
        assert!(bars.look(&[(9, false)]).is_empty());
        assert!(bars.look(&[(9, false)]).is_empty());
    }

    #[test]
    fn a_bar_is_started_as_it_was() {
        assert_eq!(
            command(b"waybar\0-c\0/home/a b/it's.jsonc\0").as_deref(),
            Some(r"'waybar' '-c' '/home/a b/it'\''s.jsonc'")
        );
        assert_eq!(command(b""), None);
        assert_eq!(socket_inode("socket:[5734]"), Some(5734));
        assert_eq!(socket_inode("pipe:[5734]"), None);
    }
}

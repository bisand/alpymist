//! The address a network interface has.
//!
//! It is not on iwd's bus: iwd joins the network, and what gives the
//! interface an address is somebody else. The kernel is who knows, and is
//! asked as `ip addr` asks it: a dump of the IPv4 addresses over a route
//! netlink socket, from which the interface's own is picked by its index.

// What asks the kernel is Linux's; elsewhere only the tests read an answer.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::net::Ipv4Addr;

/// A netlink message's header, and an address message's own after it.
const HEADER: usize = 16;
const IFADDR: usize = 8;

/// From `linux/netlink.h` and `linux/rtnetlink.h`.
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const RTM_NEWADDR: u16 = 20;
const RTM_GETADDR: u16 = 22;
const NLM_F_REQUEST: u16 = 0x1;
const NLM_F_DUMP: u16 = 0x300;
const NLM_F_DUMP_INTR: u16 = 0x10;
const AF_INET: u8 = 2;

/// From `linux/if_addr.h`. On an ordinary interface the two are the same; on
/// a point-to-point one `IFA_ADDRESS` is the far end, so `IFA_LOCAL` is the
/// one taken where both are there.
const IFA_ADDRESS: u16 = 1;
const IFA_LOCAL: u16 = 2;

/// The first IPv4 address of the interface named `interface`.
#[must_use]
pub fn ipv4(interface: &str) -> Option<String> {
    // A name is one path component, or it is not a name.
    if interface.is_empty() || interface.contains(['/', '\0']) || interface.starts_with('.') {
        return None;
    }
    let index = std::fs::read_to_string(format!("/sys/class/net/{interface}/ifindex"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    asked(index).map(|address| address.to_string())
}

/// Ask the kernel for the first IPv4 address of the interface with `index`.
#[cfg(target_os = "linux")]
fn asked(index: u32) -> Option<Ipv4Addr> {
    use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketType};
    // No protocol named is protocol 0, which for netlink is the route one.
    let fd = rustix::net::socket(AddressFamily::NETLINK, SocketType::RAW, None).ok()?;
    // A kernel that does not answer is one that was not asked.
    rustix::net::sockopt::set_socket_timeout(
        &fd,
        rustix::net::sockopt::Timeout::Recv,
        Some(std::time::Duration::from_secs(2)),
    )
    .ok()?;
    rustix::net::send(&fd, &request(), SendFlags::empty()).ok()?;
    let mut found = None;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let (n, _) = rustix::net::recv(&fd, &mut buffer[..], RecvFlags::empty()).ok()?;
        if n == 0 || read(&buffer[..n], index, &mut found)? {
            return found;
        }
    }
}

/// Without Linux there is no kernel to ask this way.
#[cfg(not(target_os = "linux"))]
fn asked(_index: u32) -> Option<Ipv4Addr> {
    None
}

/// The request for every IPv4 address on the machine. The kernel does not
/// narrow a dump to one interface; that is done on what comes back.
fn request() -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + IFADDR);
    out.extend_from_slice(&24u32.to_ne_bytes());
    out.extend_from_slice(&RTM_GETADDR.to_ne_bytes());
    out.extend_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
    out.extend_from_slice(&1u32.to_ne_bytes()); // sequence
    out.extend_from_slice(&0u32.to_ne_bytes()); // from: the kernel fills it in
    out.extend_from_slice(&[AF_INET, 0, 0, 0]); // family, prefix, flags, scope
    out.extend_from_slice(&0u32.to_ne_bytes()); // any interface
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

/// Read one datagram of the kernel's answer: the first IPv4 address of the
/// interface with `index` is taken into `found`, where none is there yet.
/// Whether that was the last of the answer; `None` when it cannot be trusted
/// — the kernel refused, the addresses changed while it was listing them, or
/// it is not laid out as expected.
fn read(data: &[u8], index: u32, found: &mut Option<Ipv4Addr>) -> Option<bool> {
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
        if kind == RTM_NEWADDR
            && found.is_none()
            && message.get(HEADER) == Some(&AF_INET)
            && u32_at(message, HEADER + 4)? == index
        {
            let (mut local, mut address) = (None, None);
            let mut a = HEADER + IFADDR;
            while a + 4 <= message.len() {
                let alen = u16_at(message, a)? as usize;
                if alen < 4 {
                    return None;
                }
                let value = message.get(a + 4..a + alen)?;
                let ip = <[u8; 4]>::try_from(value).ok().map(Ipv4Addr::from);
                match u16_at(message, a + 2)? {
                    IFA_LOCAL => local = ip,
                    IFA_ADDRESS => address = ip,
                    _ => {}
                }
                a += aligned(alen);
            }
            *found = local.or(address);
        }
        at += aligned(len);
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::{
        AF_INET, IFA_ADDRESS, IFA_LOCAL, NLM_F_DUMP_INTR, NLMSG_DONE, NLMSG_ERROR, RTM_NEWADDR,
        ipv4, read, request,
    };
    use std::net::Ipv4Addr;

    /// A netlink message of `kind` holding `body`.
    fn message(kind: u16, flags: u16, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::try_from(16 + body.len()).unwrap().to_ne_bytes());
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(&flags.to_ne_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(body);
        out.resize(out.len().next_multiple_of(4), 0);
        out
    }

    fn attribute(kind: u16, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u16::try_from(4 + value.len()).unwrap().to_ne_bytes());
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(value);
        out.resize(out.len().next_multiple_of(4), 0);
        out
    }

    /// The kernel's word for one address on the interface with `index`.
    fn address(family: u8, index: u32, attributes: &[(u16, &[u8])]) -> Vec<u8> {
        let mut body = vec![family, 24, 0, 0];
        body.extend_from_slice(&index.to_ne_bytes());
        for (kind, value) in attributes {
            body.extend(attribute(*kind, value));
        }
        message(RTM_NEWADDR, 0, &body)
    }

    fn done() -> Vec<u8> {
        message(NLMSG_DONE, 0, &[0; 4])
    }

    /// `IFA_LABEL`, the interface's name: one of the attributes that are not
    /// the address, and not a multiple of four long.
    const IFA_LABEL: u16 = 3;

    #[test]
    fn the_request_is_a_dump_of_ipv4_addresses() {
        let request = request();
        assert_eq!(request.len(), 24);
        assert_eq!(request[..4], 24u32.to_ne_bytes());
        assert_eq!(request[4..6], 22u16.to_ne_bytes());
        assert_eq!(request[6..8], 0x301u16.to_ne_bytes());
        assert_eq!(request[16], AF_INET);
    }

    #[test]
    fn the_address_is_the_one_on_the_interface_asked_about() {
        let mut data = address(AF_INET, 1, &[(IFA_LOCAL, &[127, 0, 0, 1])]);
        data.extend(address(
            AF_INET,
            3,
            &[
                (IFA_ADDRESS, &[192, 168, 1, 40]),
                (IFA_LABEL, b"wlan0\0"),
                (IFA_LOCAL, &[192, 168, 1, 40]),
            ],
        ));
        data.extend(done());
        let mut found = None;
        assert_eq!(read(&data, 3, &mut found), Some(true));
        assert_eq!(found, Some(Ipv4Addr::new(192, 168, 1, 40)));

        let mut found = None;
        assert_eq!(read(&data, 2, &mut found), Some(true));
        assert_eq!(found, None);
    }

    #[test]
    fn the_first_address_is_kept_and_the_answer_may_come_in_parts() {
        let first = address(AF_INET, 3, &[(IFA_LOCAL, &[10, 0, 0, 2])]);
        let mut second = address(AF_INET, 3, &[(IFA_LOCAL, &[10, 0, 0, 3])]);
        second.extend(done());
        let mut found = None;
        assert_eq!(read(&first, 3, &mut found), Some(false));
        assert_eq!(read(&second, 3, &mut found), Some(true));
        assert_eq!(found, Some(Ipv4Addr::new(10, 0, 0, 2)));
    }

    #[test]
    fn this_end_of_a_point_to_point_link_is_the_address_and_not_the_far_end() {
        let data = address(
            AF_INET,
            7,
            &[(IFA_ADDRESS, &[10, 8, 0, 1]), (IFA_LOCAL, &[10, 8, 0, 2])],
        );
        let mut found = None;
        assert_eq!(read(&data, 7, &mut found), Some(false));
        assert_eq!(found, Some(Ipv4Addr::new(10, 8, 0, 2)));
    }

    #[test]
    fn an_address_that_is_not_ipv4_is_not_taken_for_one() {
        // AF_INET6, whose address is sixteen bytes.
        let data = address(10, 3, &[(IFA_ADDRESS, &[0xfe; 16])]);
        let mut found = None;
        assert_eq!(read(&data, 3, &mut found), Some(false));
        assert_eq!(found, None);
    }

    #[test]
    fn an_answer_that_cannot_be_trusted_is_no_answer() {
        let mut found = None;
        let refused = message(NLMSG_ERROR, 0, &[0; 20]);
        assert_eq!(read(&refused, 3, &mut found), None);
        let interrupted = message(
            RTM_NEWADDR,
            NLM_F_DUMP_INTR,
            &[AF_INET, 0, 0, 0, 3, 0, 0, 0],
        );
        assert_eq!(read(&interrupted, 3, &mut found), None);
        // Cut short: a length that runs past the end.
        let mut cut = address(AF_INET, 3, &[(IFA_LOCAL, &[10, 0, 0, 2])]);
        cut.truncate(cut.len() - 2);
        assert_eq!(read(&cut, 3, &mut found), None);
        // An attribute shorter than its own header.
        let mut body = vec![AF_INET, 24, 0, 0, 3, 0, 0, 0];
        body.extend_from_slice(&[2, 0, 2, 0]);
        assert_eq!(read(&message(RTM_NEWADDR, 0, &body), 3, &mut found), None);
        assert_eq!(found, None);
    }

    #[test]
    fn a_name_that_is_not_one_is_not_looked_up() {
        assert_eq!(ipv4(""), None);
        assert_eq!(ipv4("../../etc"), None);
        assert_eq!(ipv4("no-such-interface-here"), None);
    }

    /// The loopback interface has an address on any Linux there is, and its
    /// index is 1: the kernel itself, asked and understood.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_kernel_says_loopback_is_where_it_always_is() {
        assert_eq!(ipv4("lo").as_deref(), Some("127.0.0.1"));
    }
}

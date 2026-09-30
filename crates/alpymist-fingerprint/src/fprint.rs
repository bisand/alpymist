//! Talking to the fingerprint daemon: fprintd, or validity-fprintd in its
//! place, which serve the same `net.reactivated.Fprint` interface. Nothing
//! here knows which it is.
//!
//! One connection does everything, on a thread of its own: the claim belongs
//! to the connection that made it, and enrolling, checking and removing are
//! only accepted from the connection holding it. Enrolling and removing ask
//! polkit, which may ask for the password through the agent the window
//! registered, so a call can wait on a person; the window carries on drawing
//! meanwhile, and hears how things went as [`Event`]s.

use std::sync::mpsc::Receiver;
use zbus::blocking::{Connection, MessageIterator, Proxy};

/// The daemon's bus name.
pub const SERVICE: &str = "net.reactivated.Fprint";
const MANAGER_PATH: &str = "/net/reactivated/Fprint/Manager";
const MANAGER: &str = "net.reactivated.Fprint.Manager";
const DEVICE: &str = "net.reactivated.Fprint.Device";

/// What the window asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Add this finger.
    Enrol(String),
    /// Check a finger against those enrolled.
    Test,
    /// Stop enrolling or checking.
    Stop,
    /// Remove every enrolled finger.
    RemoveAll,
}

/// What the daemon said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The reader, its enrolled fingers, and how many scans an enrolment
    /// takes; `None` for the reader when there is none, or no daemon.
    Ready {
        /// The reader's name.
        reader: Option<String>,
        /// This account's enrolled fingers.
        enrolled: Vec<String>,
        /// Scans an enrolment takes.
        stages: u32,
        /// Why there is no reader, when there is not.
        why: Option<String>,
    },
    /// `EnrollStatus`: a result, and whether it is the last.
    Enroll(String, bool),
    /// `VerifyStatus`: a result, and whether it is the last.
    Verify(String, bool),
    /// Removing finished.
    Removed(Result<(), String>),
    /// A request was turned down: by polkit, or by the daemon.
    Refused(String),
}

/// Talk to the daemon until `orders` is closed, sending what it says with
/// `send` for as long as that returns true. Releases the claim at the end.
pub fn run(
    orders: &Receiver<Order>,
    send: &(impl Fn(Event) -> bool + Send + Sync + 'static + Clone),
) {
    let (conn, device, ready) = match open() {
        Ok(opened) => opened,
        Err(why) => {
            send(Event::Ready {
                reader: None,
                enrolled: Vec::new(),
                stages: 0,
                why: Some(why),
            });
            return;
        }
    };
    listen(&conn, &device, send.clone());
    if !send(ready) {
        return;
    }
    for order in orders {
        let answer = match order {
            Order::Enrol(finger) => device
                .call_method("EnrollStart", &(finger.as_str(),))
                .err()
                .map(|e| Event::Refused(reason(&e))),
            Order::Test => device
                .call_method("VerifyStart", &("any",))
                .err()
                .map(|e| Event::Refused(reason(&e))),
            Order::Stop => {
                // Whichever is going on; the other says so, and is ignored.
                let _ = device.call_method("EnrollStop", &());
                let _ = device.call_method("VerifyStop", &());
                None
            }
            Order::RemoveAll => Some(Event::Removed(
                device
                    .call_method("DeleteEnrolledFingers2", &())
                    .map(drop)
                    .map_err(|e| reason(&e)),
            )),
        };
        if let Some(event) = answer
            && !send(event)
        {
            break;
        }
    }
    let _ = device.call_method("Release", &());
}

/// Connect, find the reader, claim it for this account, and read what it has.
fn open() -> Result<(Connection, Proxy<'static>, Event), String> {
    let conn = Connection::system().map_err(|e| format!("no system bus: {e}"))?;
    let manager = proxy(&conn, MANAGER_PATH.to_owned(), MANAGER)?;
    let path: zbus::zvariant::OwnedObjectPath = manager
        .call("GetDefaultDevice", &())
        .map_err(|e| no_reader(&e))?;
    let device = proxy(&conn, path.to_string(), DEVICE)?;
    let name: String = device
        .get_property("name")
        .unwrap_or_else(|_| "Fingerprint reader".into());
    let stages: i32 = device.get_property("num-enroll-stages").unwrap_or(5);
    // For the caller: an empty name means this account.
    device
        .call_method("Claim", &("",))
        .map_err(|e| format!("The reader is in use: {}", reason(&e)))?;
    // fprintd says "none" as an error.
    let enrolled: Vec<String> = device
        .call("ListEnrolledFingers", &("",))
        .unwrap_or_default();
    let ready = Event::Ready {
        reader: Some(name),
        enrolled,
        stages: u32::try_from(stages.max(1)).unwrap_or(5),
        why: None,
    };
    Ok((conn, device, ready))
}

fn proxy(
    conn: &Connection,
    path: String,
    interface: &'static str,
) -> Result<Proxy<'static>, String> {
    zbus::blocking::proxy::Builder::new(conn)
        .destination(SERVICE)
        .and_then(|b| b.path(path))
        .and_then(|b| b.interface(interface))
        .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
        .and_then(zbus::blocking::proxy::Builder::build)
        .map_err(|e| format!("the fingerprint daemon: {e}"))
}

/// Forward the device's `EnrollStatus` and `VerifyStatus` from a thread of
/// their own, while the calls wait on theirs.
fn listen(
    conn: &Connection,
    device: &Proxy<'static>,
    send: impl Fn(Event) -> bool + Send + 'static,
) {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(SERVICE)
        .and_then(|b| b.interface(DEVICE))
        .and_then(|b| b.path(device.path().to_owned()))
        .map(zbus::match_rule::Builder::build);
    let Ok(rule) = rule else {
        return;
    };
    let Ok(messages) = MessageIterator::for_match_rule(rule, conn, Some(64)) else {
        return;
    };
    std::thread::spawn(move || {
        for message in messages.flatten() {
            let header = message.header();
            let Some(member) = header.member() else {
                continue;
            };
            let Ok((result, done)) = message.body().deserialize::<(String, bool)>() else {
                continue;
            };
            let event = match member.as_str() {
                "EnrollStatus" => Event::Enroll(result, done),
                "VerifyStatus" => Event::Verify(result, done),
                _ => continue,
            };
            if !send(event) {
                return;
            }
        }
    });
}

/// Why there is no reader: none connected, or no daemon to ask.
fn no_reader(e: &zbus::Error) -> String {
    let text = e.to_string();
    if text.contains("NoSuchDevice") {
        "No fingerprint reader was found.".into()
    } else if text.contains("ServiceUnknown") || text.contains("NameHasNoOwner") {
        "The fingerprint service is not running.".into()
    } else {
        format!("The fingerprint service did not answer: {}", reason(e))
    }
}

/// A D-Bus error as a person reads it: its message, not its name.
fn reason(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, message, _) => {
            if name.as_str().ends_with("AccessDenied")
                || name.as_str().ends_with("PermissionDenied")
            {
                "Not allowed: the password was not given, or not right.".into()
            } else {
                message.clone().unwrap_or_else(|| name.to_string())
            }
        }
        other => other.to_string(),
    }
}

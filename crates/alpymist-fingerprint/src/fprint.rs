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

use alpymist_dbus::{Connection, Error, Proxy, Rule};
use std::sync::mpsc::Receiver;

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
                .call("EnrollStart", vec![finger.as_str().into()])
                .err()
                .map(|e| Event::Refused(reason(&e))),
            Order::Test => device
                .call("VerifyStart", vec!["any".into()])
                .err()
                .map(|e| Event::Refused(reason(&e))),
            Order::Stop => {
                // Whichever is going on; the other says so, and is ignored.
                let _ = device.call("EnrollStop", Vec::new());
                let _ = device.call("VerifyStop", Vec::new());
                None
            }
            Order::RemoveAll => Some(Event::Removed(
                device
                    .call("DeleteEnrolledFingers2", Vec::new())
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
    let _ = device.call("Release", Vec::new());
}

/// Connect, find the reader, claim it for this account, and read what it has.
fn open() -> Result<(Connection, Proxy, Event), String> {
    let conn = Connection::system().map_err(|e| format!("no system bus: {e}"))?;
    let manager = conn.proxy(SERVICE, MANAGER_PATH, MANAGER);
    let path = manager
        .ask("GetDefaultDevice", Vec::new())
        .map_err(|e| no_reader(&e))?;
    let path = path
        .as_path()
        .ok_or("The fingerprint service did not say where the reader is.")?;
    let device = conn.proxy(SERVICE, path, DEVICE);
    let name = device
        .get("name")
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_else(|| "Fingerprint reader".into());
    let stages = device
        .get("num-enroll-stages")
        .ok()
        .and_then(|stages| stages.as_i64())
        .unwrap_or(5);
    // For the caller: an empty name means this account.
    device
        .call("Claim", vec!["".into()])
        .map_err(|e| format!("The reader is in use: {}", reason(&e)))?;
    // fprintd says "none" as an error.
    let enrolled = device
        .ask("ListEnrolledFingers", vec!["".into()])
        .map(|fingers| {
            fingers
                .items()
                .iter()
                .filter_map(|finger| finger.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let ready = Event::Ready {
        reader: Some(name),
        enrolled,
        stages: u32::try_from(stages.max(1)).unwrap_or(5),
        why: None,
    };
    Ok((conn, device, ready))
}

/// Forward the device's `EnrollStatus` and `VerifyStatus` from a thread of
/// their own, while the calls wait on theirs.
fn listen(conn: &Connection, device: &Proxy, send: impl Fn(Event) -> bool + Send + 'static) {
    let rule = Rule::from(SERVICE).interface(DEVICE).path(device.path());
    let Ok(messages) = conn.signals(rule) else {
        return;
    };
    std::thread::spawn(move || {
        for message in messages {
            // Both say how it went, and whether that is the end of it.
            let [result, done] = message.body.as_slice() else {
                continue;
            };
            let (Some(result), Some(done)) = (result.as_str(), done.as_bool()) else {
                continue;
            };
            let event = match message.member.as_deref() {
                Some("EnrollStatus") => Event::Enroll(result.to_owned(), done),
                Some("VerifyStatus") => Event::Verify(result.to_owned(), done),
                _ => continue,
            };
            if !send(event) {
                return;
            }
        }
    });
}

/// Why there is no reader: none connected, or no daemon to ask.
fn no_reader(e: &Error) -> String {
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
fn reason(e: &Error) -> String {
    match e {
        Error::Method { name, message } => {
            if name.ends_with("AccessDenied") || name.ends_with("PermissionDenied") {
                "Not allowed: the password was not given, or not right.".into()
            } else {
                message.clone().unwrap_or_else(|| name.clone())
            }
        }
        other => other.to_string(),
    }
}

//! A connection to the bus.
//!
//! One socket, and one thread reading it for as long as the connection is
//! kept. What it reads is one of three things: the answer to a call, which
//! goes to whoever is waiting for it; a signal, which goes to whoever asked
//! to hear it; or a call of a method served here, which is answered on a
//! thread of its own — so that a method may take as long as a person does to
//! type a password, and another may be called meanwhile to call it off.

use crate::wire::{Kind, Message, valid_path};
use crate::{Error, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

/// The bus itself, as a name, a path and an interface.
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// Where the system bus listens, when nothing says otherwise.
const SYSTEM_SOCKETS: [&str; 2] = [
    "/run/dbus/system_bus_socket",
    "/var/run/dbus/system_bus_socket",
];

/// A served method's refusal: an error's name and what it has to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The error's name, as `org.freedesktop.PolicyKit1.Error.Cancelled`.
    pub name: String,
    /// What to say with it.
    pub message: String,
}

impl Failure {
    /// An error called `name`, saying `message`.
    #[must_use]
    pub fn new(name: &str, message: &str) -> Self {
        Self {
            name: name.to_owned(),
            message: message.to_owned(),
        }
    }
}

/// What answers the calls made of an object served here: given the call, it
/// returns the body of the answer, or the error to answer with.
type Handler = dyn Fn(&Message) -> Result<Vec<Value>, Failure> + Send + Sync;

/// Which signals to hear. Every part given has to match; a rule with nothing
/// given hears every signal there is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    sender: Option<String>,
    path: Option<String>,
    interface: Option<String>,
    member: Option<String>,
    arg0: Option<String>,
}

impl Rule {
    /// Signals from `sender`: a name on the bus.
    #[must_use]
    pub fn from(sender: &str) -> Self {
        Self {
            sender: Some(sender.to_owned()),
            ..Self::default()
        }
    }

    /// Only those about the object at `path`.
    #[must_use]
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_owned());
        self
    }

    /// Only those of `interface`.
    #[must_use]
    pub fn interface(mut self, interface: &str) -> Self {
        self.interface = Some(interface.to_owned());
        self
    }

    /// Only the signal called `member`.
    #[must_use]
    pub fn member(mut self, member: &str) -> Self {
        self.member = Some(member.to_owned());
        self
    }

    /// Only those whose first argument is the text `value`.
    #[must_use]
    pub fn arg0(mut self, value: &str) -> Self {
        self.arg0 = Some(value.to_owned());
        self
    }

    /// The rule as the bus is given it.
    fn text(&self) -> Result<String, Error> {
        let mut out = String::from("type='signal'");
        for (key, value) in [
            ("sender", &self.sender),
            ("path", &self.path),
            ("interface", &self.interface),
            ("member", &self.member),
            ("arg0", &self.arg0),
        ] {
            if let Some(value) = value {
                // A rule is quoted with apostrophes, and has no way to hold
                // one that is worth the trouble here.
                if value.contains(['\'', '\\']) {
                    return Err(Error::Protocol(format!("a rule cannot hold {value:?}")));
                }
                let _ = write!(out, ",{key}='{value}'");
            }
        }
        Ok(out)
    }

    /// Whether `signal` is one this rule asked for, as far as can be told
    /// here. The bus has matched the sender already, by a name's owner, which
    /// is not known on this side; all that is checked of it is that the
    /// bus's own signals go only to those who asked for the bus's.
    fn hears(&self, signal: &Message) -> bool {
        let same = |want: &Option<String>, have: &Option<String>| {
            want.is_none() || want.as_deref() == have.as_deref()
        };
        let from_bus = signal.sender.as_deref() == Some(BUS);
        let wants_bus = self.sender.as_deref() == Some(BUS);
        (self.sender.is_none() || from_bus == wants_bus)
            && same(&self.path, &signal.path)
            && same(&self.interface, &signal.interface)
            && same(&self.member, &signal.member)
            && (self.arg0.is_none() || self.arg0.as_deref() == signal.text())
    }
}

/// A connection to the bus. Cheap to clone; the connection closes when the
/// last of them is dropped.
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Inner>,
}

struct Inner {
    /// The socket, for writing: one message at a time.
    writer: Mutex<UnixStream>,
    serial: AtomicU32,
    /// What the bus calls this connection.
    name: OnceLock<String>,
    /// Who waits for the answer to which call.
    waiting: Mutex<HashMap<u32, Sender<Message>>>,
    listeners: Mutex<Vec<(Rule, Sender<Message>)>>,
    /// The objects served, by path and interface.
    served: Mutex<HashMap<(String, String), Arc<Handler>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Ends the thread that reads, which holds the socket's other handle.
        let _ = lock(&self.writer).shutdown(std::net::Shutdown::Both);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Connection {
    /// Connect to the system bus: where `DBUS_SYSTEM_BUS_ADDRESS` says, or
    /// where it always is.
    ///
    /// # Errors
    /// No bus there, or one that would not have us.
    pub fn system() -> Result<Self, Error> {
        let named = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok();
        let from_env = named.as_deref().and_then(socket_of);
        let mut last = Error::Io("no system bus".into());
        for path in from_env.iter().copied().chain(SYSTEM_SOCKETS) {
            match Self::open(path) {
                Ok(connection) => return Ok(connection),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Connect to the bus listening on the socket at `path`.
    ///
    /// # Errors
    /// Nothing listening there, or a bus that would not have us.
    pub fn open(path: &str) -> Result<Self, Error> {
        let io = |e: std::io::Error| Error::Io(format!("{path}: {e}"));
        let mut stream = UnixStream::connect(path).map_err(io)?;
        authenticate(&mut stream).map_err(|why| Error::Io(format!("{path}: {why}")))?;
        let reading = stream.try_clone().map_err(io)?;
        let connection = Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(stream),
                serial: AtomicU32::new(1),
                name: OnceLock::new(),
                waiting: Mutex::default(),
                listeners: Mutex::default(),
                served: Mutex::default(),
            }),
        };
        let inner = Arc::downgrade(&connection.inner);
        std::thread::Builder::new()
            .name("dbus".into())
            .spawn(move || read(reading, &inner))
            .map_err(io)?;
        // Nothing may be said on a bus before this, and its answer is the
        // name the bus gives the connection.
        let hello = connection.call(BUS, BUS_PATH, BUS, "Hello", Vec::new())?;
        let name = hello.first().and_then(Value::as_str).unwrap_or_default();
        let _ = connection.inner.name.set(name.to_owned());
        Ok(connection)
    }

    /// The name the bus gave this connection, as `:1.42`: what another
    /// connection calls it by.
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name.get().map_or("", String::as_str)
    }

    /// Call `member` of `interface` on the object at `path` that
    /// `destination` has, and wait for the answer: the values it came with.
    ///
    /// Waits for as long as the other end takes. Should that end leave the
    /// bus, the bus answers for it with an error.
    ///
    /// # Errors
    /// The other end's error, or the connection's.
    pub fn call(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Result<Vec<Value>, Error> {
        if !valid_path(path) {
            return Err(Error::Protocol(format!("not an object's path: {path:?}")));
        }
        let mut call = Message::new(Kind::Call);
        call.destination = Some(destination.to_owned());
        call.path = Some(path.to_owned());
        call.interface = Some(interface.to_owned());
        call.member = Some(member.to_owned());
        call.body = body;

        let (answer, answered) = channel();
        let serial = self.inner.send(&call, Some(answer))?;
        let Ok(reply) = answered.recv() else {
            lock(&self.inner.waiting).remove(&serial);
            return Err(Error::Closed);
        };
        match reply.kind {
            Kind::Error => Err(Error::Method {
                message: reply.text().map(str::to_owned),
                name: reply.error_name.unwrap_or_default(),
            }),
            _ => Ok(reply.body),
        }
    }

    /// An object on the bus, to call and to read without naming it each time.
    #[must_use]
    pub fn proxy(&self, destination: &str, path: &str, interface: &str) -> Proxy {
        Proxy {
            connection: self.clone(),
            destination: destination.to_owned(),
            path: path.to_owned(),
            interface: interface.to_owned(),
        }
    }

    /// Hear the signals `rule` asks for, from now until the receiver is
    /// dropped.
    ///
    /// # Errors
    /// The bus would not take the rule.
    pub fn signals(&self, rule: Rule) -> Result<Receiver<Message>, Error> {
        let text = rule.text()?;
        let (sender, receiver) = channel();
        // Listening before asking: nothing sent in between is missed.
        lock(&self.inner.listeners).push((rule, sender));
        self.call(BUS, BUS_PATH, BUS, "AddMatch", vec![text.into()])?;
        Ok(receiver)
    }

    /// Serve `interface` on an object at `path`: `handler` is given each call
    /// made of it, on a thread of that call's own, and returns the answer.
    ///
    /// # Errors
    /// `path` is not an object's path.
    pub fn serve(
        &self,
        path: &str,
        interface: &str,
        handler: impl Fn(&Message) -> Result<Vec<Value>, Failure> + Send + Sync + 'static,
    ) -> Result<(), Error> {
        if !valid_path(path) {
            return Err(Error::Protocol(format!("not an object's path: {path:?}")));
        }
        lock(&self.inner.served).insert((path.to_owned(), interface.to_owned()), Arc::new(handler));
        Ok(())
    }
}

impl Inner {
    /// Number `message` and write it; `answer` is who waits for its reply.
    fn send(&self, message: &Message, answer: Option<Sender<Message>>) -> Result<u32, Error> {
        // Zero is not a serial; after four thousand million it comes round.
        let serial = loop {
            let serial = self.serial.fetch_add(1, Ordering::Relaxed);
            if serial != 0 {
                break serial;
            }
        };
        let bytes = message.encode(serial).map_err(Error::Protocol)?;
        if let Some(answer) = answer {
            lock(&self.waiting).insert(serial, answer);
        }
        let written = lock(&self.writer).write_all(&bytes);
        if let Err(e) = written {
            lock(&self.waiting).remove(&serial);
            return Err(Error::Io(e.to_string()));
        }
        Ok(serial)
    }

    /// Answer `call`, which is done on a thread of its own.
    fn answer(self: &Arc<Self>, call: Message) {
        let key = (
            call.path.clone().unwrap_or_default(),
            call.interface.clone().unwrap_or_default(),
        );
        let handler = lock(&self.served).get(&key).cloned();
        let inner = Arc::clone(self);
        let work = move || {
            let result = match handler {
                Some(handler) => handler(&call),
                None => Err(Failure::new(
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "nothing here answers that",
                )),
            };
            if call.no_reply {
                return;
            }
            let mut reply = match result {
                Ok(body) => {
                    let mut reply = Message::new(Kind::Return);
                    reply.body = body;
                    reply
                }
                Err(failure) => {
                    let mut reply = Message::new(Kind::Error);
                    reply.error_name = Some(failure.name);
                    reply.body = vec![failure.message.into()];
                    reply
                }
            };
            reply.no_reply = true;
            reply.reply_serial = Some(call.serial);
            reply.destination = call.sender;
            let _ = inner.send(&reply, None);
        };
        // No thread to be had is a call unanswered, which the caller's end
        // of the bus turns into an error in its own time.
        let _ = std::thread::Builder::new()
            .name("dbus-call".into())
            .spawn(work);
    }
}

/// Read messages for as long as the connection is kept and the bus speaks.
fn read(mut stream: UnixStream, inner: &Weak<Inner>) {
    while let Ok(message) = Message::read(&mut stream) {
        let Some(inner) = inner.upgrade() else {
            return;
        };
        match message.kind {
            Kind::Return | Kind::Error => {
                let waiting = message
                    .reply_serial
                    .and_then(|serial| lock(&inner.waiting).remove(&serial));
                if let Some(waiting) = waiting {
                    let _ = waiting.send(message);
                }
            }
            Kind::Signal => lock(&inner.listeners).retain(|(rule, listener)| {
                // One who has stopped listening is let go of.
                !rule.hears(&message) || listener.send(message.clone()).is_ok()
            }),
            Kind::Call => inner.answer(message),
        }
    }
    // What was unreadable, or the end: either way nobody is answered now.
    if let Some(inner) = inner.upgrade() {
        lock(&inner.waiting).clear();
        lock(&inner.listeners).clear();
    }
}

/// The socket a bus address names, where it names one by its path.
fn socket_of(address: &str) -> Option<&str> {
    address.split(';').find_map(|one| {
        one.strip_prefix("unix:")?
            .split(',')
            .find_map(|part| part.strip_prefix("path="))
    })
}

/// Say who we are, which is whoever the kernel says is at this end of the
/// socket: `EXTERNAL`, with no name given, so that the bus reads it from the
/// socket's credentials and nothing here has to know its own user's number.
fn authenticate(stream: &mut UnixStream) -> Result<(), String> {
    let mut say = |text: &str| stream.write_all(text.as_bytes()).map_err(|e| e.to_string());
    say("\0AUTH EXTERNAL\r\n")?;
    let mut answer = line(stream)?;
    if answer.starts_with("DATA") {
        stream.write_all(b"DATA\r\n").map_err(|e| e.to_string())?;
        answer = line(stream)?;
    }
    if !answer.starts_with("OK") {
        return Err(format!("the bus would not have us: {answer}"));
    }
    stream.write_all(b"BEGIN\r\n").map_err(|e| e.to_string())
}

/// One line of the bus's side of authenticating, read a byte at a time so
/// that nothing past its end is taken: what follows is not lines.
fn line(stream: &mut UnixStream) -> Result<String, String> {
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    while !out.ends_with(b"\r\n") {
        if out.len() > 4096 {
            return Err("the bus said too much".into());
        }
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        out.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&out).trim_end().to_owned())
}

/// An object on the bus.
#[derive(Clone)]
pub struct Proxy {
    connection: Connection,
    destination: String,
    path: String,
    interface: String,
}

impl Proxy {
    /// The object's path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Call `member` with `body`, and wait for what it answers with.
    ///
    /// # Errors
    /// The other end's error, or the connection's.
    pub fn call(&self, member: &str, body: Vec<Value>) -> Result<Vec<Value>, Error> {
        self.connection
            .call(&self.destination, &self.path, &self.interface, member, body)
    }

    /// Call `member` with `body` for the one value it answers with.
    ///
    /// # Errors
    /// As [`Proxy::call`], and an answer with nothing in it.
    pub fn ask(&self, member: &str, body: Vec<Value>) -> Result<Value, Error> {
        self.call(member, body)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Protocol(format!("{member} answered with nothing")))
    }

    /// The property `name`.
    ///
    /// # Errors
    /// No such property, or the other end's or the connection's error.
    pub fn get(&self, name: &str) -> Result<Value, Error> {
        let body = vec![self.interface.as_str().into(), name.into()];
        let value = self
            .connection
            .call(&self.destination, &self.path, PROPERTIES, "Get", body)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Protocol(format!("{name} has no value")))?;
        Ok(value.plain().clone())
    }

    /// Set the property `name` to `value`.
    ///
    /// # Errors
    /// The other end's refusal, or the connection's error.
    pub fn set(&self, name: &str, value: Value) -> Result<(), Error> {
        let body = vec![
            self.interface.as_str().into(),
            name.into(),
            Value::variant(value),
        ];
        self.connection
            .call(&self.destination, &self.path, PROPERTIES, "Set", body)
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::{BUS, Rule, socket_of};
    use crate::wire::{Kind, Message};

    #[test]
    fn a_bus_address_names_its_socket() {
        assert_eq!(
            socket_of("unix:path=/run/dbus/system_bus_socket"),
            Some("/run/dbus/system_bus_socket")
        );
        assert_eq!(
            socket_of("tcp:host=x;unix:guid=abc,path=/tmp/bus"),
            Some("/tmp/bus")
        );
        assert_eq!(socket_of("unix:abstract=/tmp/bus"), None);
        assert_eq!(socket_of(""), None);
    }

    fn signal(sender: &str, path: &str, interface: &str, member: &str, arg0: &str) -> Message {
        let mut signal = Message::new(Kind::Signal);
        signal.sender = Some(sender.into());
        signal.path = Some(path.into());
        signal.interface = Some(interface.into());
        signal.member = Some(member.into());
        signal.body = vec![arg0.into()];
        signal
    }

    #[test]
    fn a_rule_is_what_the_bus_is_told_and_what_is_heard() {
        let iwd = Rule::from("net.connman.iwd");
        assert_eq!(
            iwd.text().unwrap(),
            "type='signal',sender='net.connman.iwd'"
        );
        let owner = Rule::from(BUS)
            .member("NameOwnerChanged")
            .arg0("net.connman.iwd");
        assert_eq!(
            owner.text().unwrap(),
            "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',\
             arg0='net.connman.iwd'"
        );
        assert!(Rule::from("it's").text().is_err());

        let changed = signal(
            ":1.7",
            "/net/connman/iwd/0/3",
            "p",
            "PropertiesChanged",
            "x",
        );
        let started = signal(
            BUS,
            "/org/freedesktop/DBus",
            BUS,
            "NameOwnerChanged",
            "net.connman.iwd",
        );
        let other = signal(
            BUS,
            "/org/freedesktop/DBus",
            BUS,
            "NameOwnerChanged",
            "org.bluez",
        );
        assert!(iwd.hears(&changed));
        // The bus's own signals are not iwd's, though the bus sent both here.
        assert!(!iwd.hears(&started));
        assert!(owner.hears(&started));
        assert!(!owner.hears(&other));
        assert!(!owner.hears(&changed));

        let device = Rule::from("net.reactivated.Fprint")
            .interface("net.reactivated.Fprint.Device")
            .path("/net/reactivated/Fprint/Device/0");
        let status = signal(
            ":1.9",
            "/net/reactivated/Fprint/Device/0",
            "net.reactivated.Fprint.Device",
            "EnrollStatus",
            "enroll-stage-passed",
        );
        assert!(device.hears(&status));
        let elsewhere = signal(
            ":1.9",
            "/net/reactivated/Fprint/Device/1",
            "net.reactivated.Fprint.Device",
            "EnrollStatus",
            "x",
        );
        assert!(!device.hears(&elsewhere));
        assert!(Rule::default().hears(&status) && Rule::default().hears(&started));
    }
}

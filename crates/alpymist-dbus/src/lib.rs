//! A small D-Bus client, for the system bus.
//!
//! iwd, the fingerprint daemon and polkit are spoken to over D-Bus, and that
//! is all of the bus Alpymist uses: methods called and answered, signals
//! listened for, properties read and set, and two small objects served — the
//! agents iwd and polkit ask for a passphrase and a password through. This
//! does those things and no others, blocking, over a unix socket, with a
//! thread that reads.
//!
//! What it leaves out, on purpose: file descriptors in messages, the session
//! bus and its ways of being found, introspection, and anything asynchronous.
//! A caller that needs one of those needs a different crate, and should say
//! why (ADR 0025).
//!
//! # How it is used
//!
//! ```no_run
//! use alpymist_dbus::{Connection, Value};
//!
//! let bus = Connection::system()?;
//! let iwd = bus.proxy("net.connman.iwd", "/net/connman/iwd/0/3", "net.connman.iwd.Station");
//! iwd.call("Scan", Vec::new())?;
//! let scanning = iwd.get("Scanning")?.as_bool();
//! # let _ = (scanning, Value::U8(0));
//! # Ok::<(), alpymist_dbus::Error>(())
//! ```
//!
//! # What is trusted
//!
//! Nothing that is read. See [`wire`]: a message is checked as it is taken
//! apart, whoever it says it is from.

#![forbid(unsafe_code)]

mod connection;
mod value;
pub mod wire;

pub use connection::{Connection, Failure, Proxy, Rule};
pub use value::Value;
pub use wire::{Kind, Message};

/// Why something asked of the bus did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The other end answered with an error: its name, as
    /// `net.connman.iwd.Busy`, and what it had to say.
    Method {
        /// The error's name.
        name: String,
        /// Its message, where it gave one.
        message: Option<String>,
    },
    /// The bus could not be reached, or the connection to it failed.
    Io(String),
    /// Something was not D-Bus: what was to be sent could not be, or what
    /// came could not be read.
    Protocol(String),
    /// The connection closed before an answer came.
    Closed,
}

impl Error {
    /// The name of the error the other end answered with, when it did.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Method { name, .. } => Some(name),
            _ => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Method {
                name,
                message: Some(message),
            } if !message.is_empty() => write!(f, "{name}: {message}"),
            Self::Method { name, .. } => f.write_str(name),
            Self::Io(why) | Self::Protocol(why) => f.write_str(why),
            Self::Closed => f.write_str("the connection to the bus closed"),
        }
    }
}

impl std::error::Error for Error {}

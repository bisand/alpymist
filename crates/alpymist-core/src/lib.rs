//! Shared domain model for Alpymist.
//!
//! Alpymist is one desktop, Hyprland on Wayland. [`hyprland::check`] is the
//! pure function that says how well it will run on a machine, and why.

#![forbid(unsafe_code)]

mod capabilities;
pub mod catalog;
mod channel;
pub mod defaults;
pub mod desktop_entry;
pub mod firmware;
pub mod guest;
pub mod hyprland;

pub use capabilities::{Capabilities, GlesInfo, GpuDevice, Virtualisation};
pub use channel::Channel;

/// Errors produced by Alpymist libraries.
#[derive(Debug)]
pub enum Error {
    /// A `/sys` or `/proc` node could not be read.
    Probe {
        /// The path that could not be read.
        path: String,
        /// Underlying I/O failure.
        source: std::io::Error,
    },

    /// Hardware probing is not implemented for the host platform.
    UnsupportedHost(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Probe { path, source } => write!(f, "reading {path}: {source}"),
            Self::UnsupportedHost(host) => {
                write!(
                    f,
                    "hardware probing is only supported on Linux (host: {host})"
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Probe { source, .. } => Some(source),
            Self::UnsupportedHost(_) => None,
        }
    }
}

/// Convenient result alias.
pub type Result<T> = std::result::Result<T, Error>;

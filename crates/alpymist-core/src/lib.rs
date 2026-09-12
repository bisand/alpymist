//! Shared domain model for Alpymist.
//!
//! The central idea of Alpymist is that the *user-facing desktop* (keybindings, theme,
//! panel, launcher, lock screen) is defined once, and rendered onto whichever
//! session backend the machine can actually drive. [`Tier`] is the decision, and
//! [`select_tier`] is the pure function that makes it.

#![forbid(unsafe_code)]

mod capabilities;
mod tier;

pub use capabilities::{Capabilities, GlesInfo, GpuDevice, Virtualisation};
pub use tier::{Rationale, SessionBackend, Tier, select_tier};

/// Errors produced by Alpymist libraries.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A `/sys` or `/proc` node could not be read.
    #[error("reading {path}: {source}")]
    Probe {
        /// The path that could not be read.
        path: String,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// Hardware probing is not implemented for the host platform.
    #[error("hardware probing is only supported on Linux (host: {0})")]
    UnsupportedHost(&'static str),
}

/// Convenient result alias.
pub type Result<T> = std::result::Result<T, Error>;

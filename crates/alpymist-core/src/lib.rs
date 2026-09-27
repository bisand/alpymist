//! Shared domain model for Alpymist.
//!
//! Alpymist is one desktop, Hyprland on Wayland. [`hyprland::check`] is the
//! pure function that says how well it will run on a machine, and why.

#![forbid(unsafe_code)]

mod capabilities;
pub mod catalog;
mod channel;
pub mod firmware;
pub mod hyprland;

pub use capabilities::{Capabilities, GlesInfo, GpuDevice, Virtualisation};
pub use channel::Channel;

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

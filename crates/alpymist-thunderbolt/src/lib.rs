//! Thunderbolt and USB4 devices: asking before one is let in.
//!
//! A Thunderbolt or USB4 device, a dock most often, is connected to the
//! computer as if it were inside it: over `PCIe`, able to read and change
//! memory. Where the firmware leaves the decision to the system (security
//! level `user` or `secure`), nothing plugged in works — the dock's keyboard,
//! mouse, network and sound included — until something writes to its
//! `authorized` file. Alpymist asks the person at the desktop, and remembers
//! the devices they allow always. See ADR 0012. Split so each part does one
//! thing:
//!
//! - [`sysfs`]: the bus as the kernel describes it.
//! - [`store`]: the devices allowed always, and their keys.
//! - [`policy`]: what to do about a device, with no files.
//! - [`system`]: the root helper's verbs.
//! - [`session`]: whether the session is locked, and waking on uevents.
//! - [`ask`]: the question, with no pixels.
//! - [`view`]: the question, laid out and painted.

#![forbid(unsafe_code)]

#[cfg(feature = "dialog")]
pub mod ask;
pub mod policy;
pub mod session;
pub mod store;
pub mod sysfs;
pub mod system;
#[cfg(feature = "dialog")]
pub mod view;

/// Where the helper is installed.
pub const HELPER: &str = "/usr/libexec/alpymist-thunderbolt-helper";

/// Where `alpymist-auth`, which asks for the password, is installed.
pub const AUTH: &str = "/usr/bin/alpymist-auth";

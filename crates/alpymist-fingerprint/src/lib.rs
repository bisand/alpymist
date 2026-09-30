//! Alpymist's fingerprint window (#58): enrol a finger, test it, remove them.
//!
//! It talks only to `net.reactivated.Fprint`, the interface stock fprintd
//! serves and validity-fprintd serves in its place for the readers libfprint
//! has no driver for, so it works with either and never needs to know which is
//! running. Split so each part does one thing:
//!
//! - [`fprint`]: the daemon client, on a thread of its own.
//! - [`enrol`]: what the window is doing, and what each result means.
//! - [`view`]: the window, laid out and painted.
//!
//! Adding or removing a fingerprint is decided by the daemon, which asks
//! polkit; the window registers Alpymist's password agent for itself, so the
//! question comes in `alpymist-auth`'s dialog.

#![forbid(unsafe_code)]

pub mod enrol;
pub mod fprint;
#[cfg(feature = "window")]
pub mod view;

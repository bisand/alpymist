//! The Alpymist login screen: a greetd greeter drawn straight to DRM/KMS.
//!
//! greetd does the part that has to be right — PAM, the session, the VT — and
//! this only asks the questions and draws. It runs with no compositor, so it
//! looks the same on every tier and cannot be broken by a desktop that does not
//! start; and it is drawn with the same code as the splash and installer, so
//! the first boot looks like the thing that installed it.

#![forbid(unsafe_code)]

pub mod app;
pub mod clock;
pub mod ipc;
pub mod login;
pub mod users;

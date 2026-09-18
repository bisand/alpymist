//! The Alpymist screensaver: the same mountains, drawn small and blown up.
//!
//! Three pieces, kept apart as elsewhere in Alpymist. [`config`] is what the
//! user chose. [`scene`] is how the picture moves and how it is magnified,
//! which is arithmetic and is tested on any machine. [`saver`] joins the two to
//! Denise and to the layer surface the [`alpymist_widget`] host puts on screen.
//!
//! [`idle`] is the separate question of *when*: Wayland has no screensaver
//! protocol, only a way to be told that a seat has gone still, and that module
//! is what turns the settings into a `swayidle` that watches for it.

#![forbid(unsafe_code)]

pub mod config;
pub mod idle;
#[cfg(feature = "saver")]
pub mod saver;
#[cfg(feature = "saver")]
pub mod scene;

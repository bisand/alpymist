//! Alpymist Settings: every setting of [`alpymist_settings`] in one window.
//!
//! [`view`] is the window's contents, drawn with Denise's widgets and free of
//! Wayland, so a snapshot can be painted anywhere; the binary puts it in a
//! window and applies what changes.

#![forbid(unsafe_code)]

pub mod view;

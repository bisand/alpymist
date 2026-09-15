//! Alpymist's Wi-Fi manager.
//!
//! A popup under the bar that shows the joined network, lists the rest, joins
//! and forgets them, and switches the radio on and off; the icon in the bar
//! that opens it; and a command line for the same. All of it drives iwd over
//! D-Bus.
//!
//! The split is the menu's. [`model`] is what is known, as plain data;
//! [`popup`] is what every key and click does to the popup, with no pixels;
//! [`view`] paints it with Denise. Only [`iwd`] talks to anything, and only
//! the Wayland host in the binary puts pixels on a screen.
//!
//! Without the default `popup` feature the crate is iwd, the model and the
//! bar's line, which is what the installer builds on.

#![forbid(unsafe_code)]

pub mod bar;
pub mod iwd;
pub mod model;
pub mod popup;
#[cfg(feature = "popup")]
pub mod view;

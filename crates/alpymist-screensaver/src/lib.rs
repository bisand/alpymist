//! Alpymist's screensavers: the idle watch, and the library each one is built on.
//!
//! A screensaver here is a *program*, not a setting. It ships a binary and a
//! file beside it saying what it is called, what to run and what it lets you
//! change ([`definition`]); Settings renders a page from that file without
//! knowing what the program draws, and the program reads its own values from a
//! file of its own ([`values`]). Anything that ships those two becomes a
//! screensaver Alpymist offers, without Alpymist being rebuilt.
//!
//! What this crate provides:
//!
//! - [`definition`] and [`values`]: what a screensaver says about itself, and
//!   what it has been set to.
//! - [`config`]: the policy — when the screensaver comes on and when the screen
//!   goes off — and [`picture::Show`], which one.
//! - [`idle`]: the watch that decides *when*. Wayland has no screensaver
//!   protocol, only a way to be told a seat has gone still.
//! - [`paint`]: what a screensaver program writes. It draws a small picture;
//!   the shared host puts it on a layer surface, blows it up in square blocks,
//!   paces the frames and takes it away at the first key. A screensaver never
//!   touches a Wayland surface itself, which is why a second one costs a file
//!   and a few hundred lines rather than a copy of all this.
//! - [`scene`]: the arithmetic a picture is likely to want — waves, drift, and
//!   the block magnification — with no graphics stack behind it.

#![forbid(unsafe_code)]

pub mod config;
pub mod definition;
pub mod idle;
#[cfg(feature = "saver")]
pub mod paint;
pub mod picture;
// Off Linux there is no layer shell to put it on, so nothing here calls it —
// but its state machine is plain logic worth testing on any machine.
#[cfg(feature = "saver")]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod saver;
#[cfg(feature = "saver")]
pub mod scene;
pub mod values;

pub use definition::Definition;
pub use values::{Value, Values};

/// Everything the screensaver called `id` has been set to, with the defaults
/// from its own definition filled in.
///
/// What a screensaver program calls to find out what it was asked for. An `id`
/// that names nothing installed gives nothing, which a program should read as
/// "run with your own defaults" rather than as a reason not to draw.
#[must_use]
pub fn values_for(id: &str) -> Option<Values> {
    definition::discover()
        .iter()
        .find(|d| d.id == id)
        .map(Values::read)
}

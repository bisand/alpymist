//! Shared visual identity for Alpymist's boot splash and installer.
//!
//! The splash and the installer are separate binaries that run one after the
//! other, but they must look like one continuous thing — the handover should be
//! invisible. Everything that defines that shared look lives here: the palette,
//! the procedurally drawn mountain backdrop, and the scene that composes them.
//!
//! Nothing in this crate touches a GPU. Denise rasterises in software, so the
//! installer renders identically whether the machine can drive Hyprland or can
//! barely drive a framebuffer. That is deliberate: nobody should get a worse
//! setup experience because their hardware is old.

#![forbid(unsafe_code)]

pub mod backdrop;
pub mod convert;
pub mod palette;
#[cfg(feature = "render")]
pub mod render;
pub mod terrain;

pub use backdrop::{Backdrop, Layer};
pub use palette::Palette;
pub use terrain::Ridge;

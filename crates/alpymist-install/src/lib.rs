//! The Alpymist installer.
//!
//! A graphical wizard that runs before any desktop exists, drawing through
//! Denise's software rasteriser straight to DRM/KMS or the framebuffer. It
//! needs no compositor, no GPU and no Mesa, which is both why it can run this
//! early and why it looks the same on every hardware tier — nobody should get a
//! worse setup experience because their machine is old.
//!
//! The wizard itself is a pure state machine over [`answers::Answers`], so
//! every route through it and every refusal to advance is testable without a
//! screen. Rendering is a separate, thinner layer on top.

#![forbid(unsafe_code)]

pub mod answers;
pub mod wizard;

pub use answers::{Answers, DiskPlan, Field, Issue, Network};
pub use wizard::{Step, Wizard};

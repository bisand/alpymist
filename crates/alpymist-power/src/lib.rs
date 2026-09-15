//! Alpymist's power manager.
//!
//! The battery icon in the bar and what is shown beside it; a popup under the
//! bar with the battery's details, the power mode, what the lid and power
//! button do, and what the bar shows; the lid and power button themselves;
//! and a command line for all of it.
//!
//! The split is the Wi-Fi manager's. [`battery`], [`profile`] and [`config`]
//! are what is known, read from sysfs and a file; [`popup`] is what every key
//! and click does, with no pixels; [`view`] paints it with Denise. [`system`]
//! is the little that needs root, run by `alpymist-power-helper` through pkexec.

#![forbid(unsafe_code)]

pub mod actions;
pub mod bar;
pub mod battery;
pub mod config;
#[cfg(feature = "popup")]
pub mod popup;
pub mod profile;
pub mod system;
#[cfg(feature = "popup")]
pub mod view;
pub mod watch;

/// Read everything the popup and the bar show.
#[cfg(feature = "popup")]
#[must_use]
pub fn read() -> popup::Reading {
    let knobs = profile::Knobs::now();
    popup::Reading {
        power: battery::Power::now(),
        profile: knobs.current(),
        profiles: knobs.available(),
        can_hibernate: system::can_hibernate(std::path::Path::new(profile::SYSFS)),
    }
}

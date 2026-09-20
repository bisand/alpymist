//! The Alpymist lock screen.
//!
//! The screen is `alpymist-greeter`'s: the same mountains, the same clock, the
//! same card with a name and a password field. What differs is what is behind
//! it and what holds it up.
//!
//! - **Behind it is PAM**, not greetd. [`pam::check`] asks the `alpymist-lock`
//!   service whether this is the person, which on Alpine is the same
//!   `base-auth` stack a login goes through.
//! - **Holding it up is the compositor.** This is an `ext-session-lock-v1`
//!   client: the compositor blanks every output the moment the lock is asked
//!   for, keeps the session covered even if this process dies, and gives it
//!   the keyboard. A screensaver is a picture in front of an unlocked session
//!   (ADR 0009); this is not.
//!
//! Which account it asks about is [`who`]: the one this session belongs to,
//! and no list to choose from — switching users is what the login screen is
//! for.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
pub mod host;
#[cfg(target_os = "linux")]
pub mod pam;
pub mod who;

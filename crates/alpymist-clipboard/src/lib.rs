//! Alpymist's clipboard.
//!
//! Two things, both reached through `alpymist clipboard …`:
//!
//! - **Keys.** Super+C and Super+V copy and paste in whatever window has the
//!   focus: [`keys`] asks Hyprland which window that is and sends it the
//!   shortcut it understands — Ctrl+Shift+C/V to a terminal, Ctrl+C/V to
//!   anything else.
//! - **History,** off until turned on in Settings › Clipboard, since it keeps
//!   whatever was copied, passwords included. A [`daemon`] holds it in memory
//!   only, unless it is also asked to remember it across logins; it is fed by
//!   `wl-paste --watch`, skips what a password manager marks as secret, and
//!   forgets all but pinned entries when the screen locks. The menu's
//!   clipboard picker (Super+Shift+V) reads it through [`client`].

#![forbid(unsafe_code)]

pub mod client;
pub mod config;
pub mod daemon;
pub mod history;
pub mod keys;
pub mod protocol;

//! Alpymist Store: software from every source, in one window.
//!
//! Flathub's applications and Alpine's packages are searched together, as
//! the menu searches applications: type, and the results follow at the next
//! frame. Everything a search needs is already on disk — Flathub's `AppStream`
//! catalogue with its icons, apk's index — so nothing waits on the network
//! until something is installed.
//!
//! Split as the menu and the popups are: the [`catalog`] and its [`search`];
//! the [`source`]s, which read catalogues and carry out installs; the
//! [`store`], the window's state and what every key and click does, with no
//! pixels; and the [`view`], which lays that out and paints it with Denise.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod config;
pub mod icons;
pub mod pictures;
pub mod search;
pub mod source;
pub mod store;
pub mod view;

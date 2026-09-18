//! The Alpymist menu.
//!
//! One searchable menu for everything a desktop asks of its user: launching an
//! application, taking a screenshot, opening a setting, locking the screen.
//! The shape is Omarchy's — a short tree of submenus, every level searchable,
//! and the top level searching all of them at once — and the tree itself is a
//! TOML file the user can change.
//!
//! The split is the one the installer uses. Everything with logic in it —
//! reading the configuration, finding applications, ranking a search, walking
//! the tree — is plain Rust with no graphics, tested on any machine. The view
//! paints that state with Denise, and the Wayland host only moves pixels and
//! keystrokes between the two.

#![forbid(unsafe_code)]

pub mod apps;
pub mod config;
pub mod exec;
pub mod fuzzy;
pub mod history;
pub mod menu;
pub mod tree;
pub mod view;

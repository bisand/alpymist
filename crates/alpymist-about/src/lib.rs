//! About Alpymist: which version this system runs, from which channel, on
//! what.
//!
//! The split is the other windows'. [`info`] is what is known, read from files
//! any account can read; [`dialog`] is what every key and click does, with no
//! pixels; [`view`] paints it with Denise.

#![forbid(unsafe_code)]

pub mod dialog;
pub mod info;
pub mod view;

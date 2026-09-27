//! `alpymist-thunderbolt-helper` — the part of device approval that needs root.
//!
//! Run through pkexec by `alpymist-thunderbolt`, by udev when a device
//! arrives, and at boot by its `OpenRC` service:
//!
//! ```text
//! alpymist-thunderbolt-helper allow UUID once|always
//! alpymist-thunderbolt-helper reconnect UUID
//! alpymist-thunderbolt-helper forget UUID
//! alpymist-thunderbolt-helper boot
//! ```
//!
//! It takes no input but its arguments, checks them against a closed set,
//! finds the device by reading the bus, and writes only under
//! /sys/bus/thunderbolt and /var/lib/alpymist/thunderbolt.

#![forbid(unsafe_code)]

use alpymist_thunderbolt::system::{Verb, carry_out};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = Verb::parse(&args).and_then(|verb| carry_out(Path::new("/"), &verb));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-thunderbolt-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

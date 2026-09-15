//! `alpymist-power-helper` — the part of power management that needs root.
//!
//! Run through pkexec by `alpymist-power`, and at boot by its `OpenRC` service:
//!
//! ```text
//! alpymist-power-helper profile power-saver|balanced|performance
//! alpymist-power-helper charge-limit 100|90|80|60
//! alpymist-power-helper suspend|hibernate|power-off|reboot
//! alpymist-power-helper restore
//! ```
//!
//! It takes no input but its arguments, checks them against a closed set, and
//! writes only fixed files under /sys and /var/lib/alpymist.

#![forbid(unsafe_code)]

use alpymist_power::system::{Verb, carry_out};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = Verb::parse(&args).and_then(carry_out);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-power-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

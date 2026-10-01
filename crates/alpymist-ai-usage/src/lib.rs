//! How much of an AI subscription or API budget is left.
//!
//! An icon in the bar, beside Wi-Fi and the battery, showing the provider
//! closest to its limit, with every provider turned on in its tooltip.
//!
//! - **A provider is a program and a file**, as a screensaver is (ADR 0009):
//!   `/usr/share/alpymist/ai-usage/<id>.toml` names it, says what it needs,
//!   and names the program to run. Nothing here knows any provider by name.
//!   See [`definition`].
//! - **The program reports** on its standard output, as JSON: a [`report`].
//!   It is handed its keys on standard input, never as arguments, where any
//!   process could read them.
//! - **Keys live in the keyring** ([`secrets`]), never in a file in the home
//!   directory (ADR 0013).
//! - **The last answer is kept** ([`store`]) and shown with its age when a
//!   provider cannot be reached: never a blank.
//!
//! [ADR 0017](../../../docs/adr/0017-ai-usage.md) has the reasons, and
//! `docs/ai-usage-providers.md` what each provider can and cannot say.

#![forbid(unsafe_code)]

pub mod bar;
pub mod config;
pub mod definition;
pub mod http;
pub mod notify;
pub mod providers;
pub mod report;
pub mod run;
pub mod secrets;
pub mod store;
pub mod when;

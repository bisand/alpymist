//! The providers Alpymist ships.
//!
//! Each is two halves: `report`, which turns what the provider answered into
//! a [`crate::report::Report`] and is tested against the provider's own
//! documented example, and `fetch`, which asks. `alpymist-ai-provider` runs
//! one by name, and a file in `/usr/share/alpymist/ai-usage` names that —
//! which is all that makes any of them a provider.

pub mod anthropic;
pub mod claude;
pub mod openai;
pub mod openrouter;

use serde_json::Value;

/// A number from JSON that may be a number or a decimal string.
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|n| n.is_finite())
}

/// Parse a provider's answer as JSON, or say that it was not.
fn json(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|e| format!("the answer was not JSON: {e}"))
}

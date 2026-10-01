//! The providers Alpymist ships.
//!
//! Each is two halves: `report`, which turns what the provider answered into
//! a [`crate::report::Report`] and is tested against the provider's own
//! documented example, and `fetch`, which asks. `alpymist-ai-provider` runs
//! one by name, and a file in `/usr/share/alpymist/ai-usage` names that —
//! which is all that makes any of them a provider.
//!
//! [`copilot`], [`codex`] and [`gemini`] read what their vendors do not
//! document, with the login the vendor's own tool keeps: they work today and
//! may stop, and their files say `unofficial = true`. None of them writes to
//! that login or renews it; a login that has run out is the tool's to renew.

pub mod anthropic;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod gemini;
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

/// What this program says it is to a vendor.
const USER_AGENT: &str = concat!("alpymist-ai-usage/", env!("CARGO_PKG_VERSION"));

/// A plan's name as a vendor's API spells it, as a person would: `plus` is
/// "Plus", `free_workspace` is "Free workspace".
fn plan(name: &str) -> Option<String> {
    let spaced = name.trim().replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// A file under the home directory.
fn home(path: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(path))
}

#[cfg(test)]
mod tests {
    use super::plan;

    #[test]
    fn a_plan_is_spelled_as_a_person_would() {
        assert_eq!(plan("plus").as_deref(), Some("Plus"));
        assert_eq!(plan("free_workspace").as_deref(), Some("Free workspace"));
        assert_eq!(plan(""), None);
    }
}

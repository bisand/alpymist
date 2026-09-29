//! Asking Hyprland which screens there are, and telling it where they go.

use crate::screen::{self, Monitor};
use std::process::{Command, Stdio};

/// Every screen Hyprland knows of, the ones it has turned off too.
///
/// # Errors
/// Hyprland is not running, or did not answer with its list.
pub fn monitors() -> Result<Vec<Monitor>, String> {
    let out = Command::new("hyprctl")
        .args(["-j", "monitors", "all"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("hyprctl: {e}"))?;
    if !out.status.success() {
        return Err("hyprctl could not reach Hyprland".into());
    }
    screen::parse(&out.stdout)
}

/// Give Hyprland these monitor rules, all at once.
///
/// # Errors
/// hyprctl could not be run, or Hyprland refused one.
pub fn apply(rules: &[String]) -> Result<(), String> {
    if rules.is_empty() {
        return Ok(());
    }
    let batch = batch(rules);
    let out = Command::new("hyprctl")
        .args(["--batch", &batch])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("hyprctl: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    // One answer a command: "ok", or why not.
    let refused: Vec<&str> = said.split_whitespace().filter(|w| *w != "ok").collect();
    if out.status.success() && refused.is_empty() {
        Ok(())
    } else {
        Err(format!("Hyprland refused the layout: {}", said.trim()))
    }
}

/// `keyword monitor A ; keyword monitor B`: hyprctl's batch of rules.
fn batch(rules: &[String]) -> String {
    rules
        .iter()
        .map(|r| format!("keyword monitor {r}"))
        .collect::<Vec<_>>()
        .join(" ; ")
}

#[cfg(test)]
mod tests {
    use super::batch;

    #[test]
    fn rules_go_in_one_batch() {
        assert_eq!(
            batch(&["desc:A, disable".into(), "DP-3, preferred, 0x0, 1".into()]),
            "keyword monitor desc:A, disable ; keyword monitor DP-3, preferred, 0x0, 1"
        );
    }
}

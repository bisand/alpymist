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

/// Hyprland's command socket for this session.
fn socket() -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|v| !v.is_empty())?;
    Some(
        std::path::PathBuf::from(runtime)
            .join("hypr")
            .join(instance)
            .join(".socket.sock"),
    )
}

/// Ask Hyprland something on its socket, as hyprctl does but without
/// starting it: for the Super+number keys, which should feel immediate.
/// `j/monitors all`, `dispatch workspace 11`.
///
/// # Errors
/// There is no Hyprland to ask.
pub fn request(command: &str) -> Result<Vec<u8>, String> {
    use std::io::{Read as _, Write as _};
    let path = socket().ok_or("not in a Hyprland session")?;
    let mut stream = std::os::unix::net::UnixStream::connect(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    stream
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).map_err(|e| e.to_string())?;
    Ok(answer)
}

/// A workspace, and the screen it is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// Its id: 1 to 9 for Super+1 to Super+9, below 0 for special ones.
    pub id: i32,
    /// Its name: its id, unless something named it.
    pub name: String,
    /// The connector of the screen it is on.
    pub monitor: String,
}

/// Every workspace there is.
///
/// # Errors
/// Hyprland could not be asked.
pub fn workspaces() -> Result<Vec<Workspace>, String> {
    let out = Command::new("hyprctl")
        .args(["-j", "workspaces"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("hyprctl: {e}"))?;
    serde_json::from_slice(&out.stdout).map_err(|e| format!("Hyprland's workspaces: {e}"))
}

/// Run these dispatchers, in order, all at once: `workspace 1`.
///
/// # Errors
/// hyprctl could not be run.
pub fn dispatch(commands: &[String]) -> Result<(), String> {
    if commands.is_empty() {
        return Ok(());
    }
    let batch = commands
        .iter()
        .map(|c| format!("dispatch {c}"))
        .collect::<Vec<_>>()
        .join(" ; ");
    Command::new("hyprctl")
        .args(["--batch", &batch])
        .stdin(Stdio::null())
        .output()
        .map(drop)
        .map_err(|e| format!("hyprctl: {e}"))
}

/// Have Hyprland read its configuration again.
///
/// # Errors
/// hyprctl could not be run.
pub fn reload() -> Result<(), String> {
    Command::new("hyprctl")
        .arg("reload")
        .stdin(Stdio::null())
        .output()
        .map(drop)
        .map_err(|e| format!("hyprctl: {e}"))
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

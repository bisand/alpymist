//! Following screens as they come and go.
//!
//! Hyprland says on its event socket when a screen is connected or taken
//! away. A dock brings its screens one after another, so the layout is put in
//! place once they have stopped arriving, and only when the set of screens is
//! not the one already laid out: turning the panel off for a closed lid is
//! itself a screen going, and must not start it all again.

use crate::screen;
use std::io::{BufRead, BufReader, ErrorKind};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long screens must have stopped arriving.
const SETTLE: Duration = Duration::from_millis(700);
/// How long after Hyprland stops telling of events it is asked again.
const RETRY: Duration = Duration::from_secs(1);

/// Whether an event line is about a screen coming or going.
#[must_use]
pub fn screens_changed(line: &str) -> bool {
    let event = line.split(">>").next().unwrap_or("");
    matches!(
        event,
        "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
    )
}

/// Hyprland's event socket for this session.
#[must_use]
pub fn socket() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|v| !v.is_empty())?;
    Some(
        PathBuf::from(runtime)
            .join("hypr")
            .join(instance)
            .join(".socket2.sock"),
    )
}

/// Hyprland's event socket, opened to be read a line at a time and to give
/// up waiting after [`SETTLE`].
fn listen(path: &Path) -> std::io::Result<BufReader<UnixStream>> {
    let stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(SETTLE))?;
    Ok(BufReader::new(stream))
}

/// Put the right layout in place now, and again whenever the screens change,
/// until Hyprland ends.
///
/// # Errors
/// There is no Hyprland to follow.
pub fn run() -> Result<(), String> {
    let path = socket().ok_or("not in a Hyprland session")?;
    let mut reader = listen(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut laid_out = settle(None);
    let mut pending = false;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            // Hyprland has gone, and the session with it — or it has only
            // stopped telling this listener, as it does one that falls
            // behind, and can be asked again. What the screens did meanwhile
            // went unheard, so they are looked at afresh.
            Ok(0) => {
                std::thread::sleep(RETRY);
                let Ok(again) = listen(&path) else {
                    return Ok(());
                };
                reader = again;
                pending = true;
            }
            Ok(_) => pending |= screens_changed(line.trim_end()),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if pending {
                    pending = false;
                    laid_out = settle(laid_out);
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Lay out the screens there now, unless they are the set `laid_out`
/// already was. The set it was done for.
fn settle(laid_out: Option<Vec<String>>) -> Option<Vec<String>> {
    let monitors = match crate::hypr::monitors() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("alpymist displays: {e}");
            return laid_out;
        }
    };
    let key = crate::layout::key(screen::names(&monitors));
    if laid_out.as_ref() == Some(&key) {
        return laid_out;
    }
    match crate::apply() {
        Ok(plan) => Some(plan.key),
        Err(e) => {
            eprintln!("alpymist displays: {e}");
            laid_out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::screens_changed;

    #[test]
    fn only_screens_coming_and_going_count() {
        assert!(screens_changed("monitoradded>>DP-3"));
        assert!(screens_changed("monitoraddedv2>>2,DP-3,Samsung S24E650"));
        assert!(screens_changed("monitorremoved>>DP-3"));
        assert!(!screens_changed("focusedmon>>DP-3,1"));
        assert!(!screens_changed("workspace>>2"));
    }
}

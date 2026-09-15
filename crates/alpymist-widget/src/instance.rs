//! One popup at a time: running the binary again closes the open one.
//!
//! A bar icon's `on-click` runs the binary each time. The first run becomes
//! the popup and listens on a socket in `$XDG_RUNTIME_DIR`; a second run finds
//! it there, connects, and exits, and the popup closes when it is connected
//! to. So one click opens and the next closes, with nothing for the bar to
//! track.

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How recently a popup must have been dismissed from outside for a new one
/// not to open.
///
/// A click on the bar's icon while the popup is open lands on the popup's
/// surface and closes it. Should the bar still see that click — on release,
/// or on a compositor that passes it through — it would start the popup
/// again, and the click meant to close it would appear to do nothing.
pub const REOPEN_GUARD: Duration = Duration::from_millis(500);

/// Where a running popup called `name` listens for another invocation.
#[must_use]
pub fn socket_path(name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    Some(PathBuf::from(dir).join(format!("{name}-{display}.sock")))
}

/// Either become the running popup, or tell the running one to close.
///
/// `Err(())` means another popup was open and has been asked to close.
fn claim(path: Option<&Path>) -> Result<Option<UnixListener>, ()> {
    let Some(path) = path else {
        return Ok(None);
    };
    if UnixStream::connect(path).is_ok() {
        return Err(());
    }
    std::fs::remove_file(path).ok();
    Ok(UnixListener::bind(path).ok())
}

/// Whether a popup called `name` is open.
#[must_use]
pub fn is_open(name: &str) -> bool {
    socket_path(name).is_some_and(|p| UnixStream::connect(p).is_ok())
}

/// Open the popup called `name`, or close it if it is already open.
///
/// `open` runs the popup, given the listener the host watches to close it
/// (`None` where there is no runtime directory), and returns whether it was
/// dismissed from outside — a click elsewhere — which arms the
/// [`REOPEN_GUARD`].
///
/// # Errors
/// Whatever `open` returns.
pub fn toggle(
    name: &str,
    open: impl FnOnce(Option<UnixListener>) -> Result<bool, String>,
) -> Result<(), String> {
    let socket = socket_path(name);
    let closed_marker = socket.as_ref().map(|s| s.with_extension("closed"));
    if let Some(marker) = &closed_marker
        && let Ok(modified) = std::fs::metadata(marker).and_then(|m| m.modified())
        && modified.elapsed().is_ok_and(|age| age < REOPEN_GUARD)
    {
        std::fs::remove_file(marker).ok();
        return Ok(());
    }
    let Ok(listener) = claim(socket.as_deref()) else {
        return Ok(());
    };
    let result = open(listener);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    if let (Ok(true), Some(marker)) = (&result, &closed_marker) {
        std::fs::write(marker, b"").ok();
    }
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::claim;

    #[test]
    fn a_second_claim_closes_the_first() {
        let dir = std::env::temp_dir().join(format!("alpymist-widget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.sock");
        let first = claim(Some(&path)).unwrap().expect("a listener");
        assert!(claim(Some(&path)).is_err(), "the second run is told no");
        assert!(first.accept().is_ok(), "and the first hears it");
        drop(first);
        std::fs::remove_dir_all(&dir).ok();
    }
}

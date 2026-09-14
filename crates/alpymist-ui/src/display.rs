//! Taking the display over from whatever had it.
//!
//! The boot splash holds the display until it sees the installer or greetd
//! starting, and lets go a moment later. Whichever of those opens the display
//! in that moment finds it still taken; failing then would stop the installer
//! or the login screen over a wait of a few hundred milliseconds.

use denise_drm::{DrmError, DrmSurface, SurfaceConfig};
use std::time::{Duration, Instant};

/// How long to wait for the display to be free.
///
/// The splash looks every 50 ms and releases at once, so this is generous: it
/// covers a slow machine, not the ordinary case.
pub const PATIENCE: Duration = Duration::from_secs(5);

/// Open the display, waiting up to `patience` for another process to let it go.
///
/// # Errors
/// The last error, if the display never became free.
pub fn open_patiently(config: SurfaceConfig, patience: Duration) -> Result<DrmSurface, DrmError> {
    let until = Instant::now() + patience;
    loop {
        match DrmSurface::open(config) {
            Ok(surface) => return Ok(surface),
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e),
        }
    }
}

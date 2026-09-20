//! Taking the display over from whatever had it.
//!
//! The boot splash holds the display until it sees the installer or greetd
//! starting, and lets go a moment later. Whichever of those opens the display
//! in that moment finds it still taken; failing then would stop the installer
//! or the login screen over a wait of a few hundred milliseconds.

use denise::geom::{Rect, Size};
use denise::pixels::PixelView;
use denise::surface::{PixelFormat, Surface, SurfaceError, required_words};
use denise_drm::{DrmError, DrmSurface, SurfaceConfig};
use denise_render::Canvas;
use std::path::Path;
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

/// The display, with a frame's worth of ordinary memory in front of it.
///
/// [`DrmSurface`] hands the renderer the scanout mapping itself, and on i915
/// that mapping is write-combining: sequential writes are fast, but scattered
/// ones defeat the combining buffers and every alpha blend has to read the
/// memory back uncached. Rasterising there is the single most expensive thing
/// the installer does. Measured on an Atom Z8350 at 1366x768, one frame:
///
/// | drawn into | cost |
/// | --- | --- |
/// | the scanout mapping | 164.9 ms |
/// | ordinary memory | 39.0 ms |
/// | ordinary memory, then copied whole to the scanout mapping | 39.0 + 2.8 ms |
///
/// The copy is sequential, which is the case write-combining exists for, so it
/// costs the same as a copy between two ordinary buffers. Nothing should ever
/// paint into an acquired frame directly; paint through here instead.
pub struct Screen {
    /// The display itself.
    surface: DrmSurface,
    /// One frame in ordinary memory, `size.width` words to a row.
    shadow: Vec<u32>,
    /// The surface's size, which DRM does not change under us.
    size: Size,
}

impl Screen {
    /// Open the display, or say why not.
    ///
    /// For callers with their own retry loop; anything starting where the
    /// splash may still hold the display wants [`Screen::open_patiently`].
    ///
    /// # Errors
    /// Whatever the display could not do.
    pub fn open(config: SurfaceConfig) -> Result<Self, DrmError> {
        Ok(Self::wrap(DrmSurface::open(config)?))
    }

    /// Open the display, waiting up to `patience` for another process to let go.
    ///
    /// # Errors
    /// The last error, if the display never became free.
    pub fn open_patiently(config: SurfaceConfig, patience: Duration) -> Result<Self, DrmError> {
        Ok(Self::wrap(open_patiently(config, patience)?))
    }

    /// Give a surface its shadow buffer.
    fn wrap(surface: DrmSurface) -> Self {
        let size = surface.size();
        // Saturating rather than failing: a size this cannot index needs a
        // display larger than the machine's address space, and `present_with`
        // reports it as the buffer shortage it is rather than refusing to open.
        let len = usize::try_from(u64::from(size.width) * u64::from(size.height)).unwrap_or(0);
        Self {
            surface,
            shadow: vec![0u32; len],
            size,
        }
    }

    /// The device node being drawn on, where there is one.
    ///
    /// For noticing that it has been replaced: early in boot the display is
    /// simpledrm, and when the real driver loads that device goes away, often
    /// replaced under the same name.
    #[must_use]
    pub fn device_path(&self) -> Option<&Path> {
        self.surface.card().path()
    }

    /// The size every frame is drawn at.
    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// Draw a frame with `paint`, then put it on the screen.
    ///
    /// `paint` gets a canvas over ordinary memory. What it leaves there is
    /// copied to the display in one pass and flipped.
    ///
    /// # Errors
    /// Whatever the display could not do, and [`SurfaceError::BufferTooSmall`]
    /// if no canvas could be made over the shadow buffer.
    pub fn present_with<F>(&mut self, paint: F) -> Result<(), SurfaceError>
    where
        F: FnOnce(&mut Canvas<'_>),
    {
        // Split the borrows: the frame borrows the surface, the view borrows
        // the shadow, and they have to be alive at the same time to copy.
        let Self {
            surface,
            shadow,
            size,
        } = self;
        let size = *size;
        let whole = [Rect::from_size(size)];

        {
            let Some(mut canvas) = Canvas::from_pixels(
                shadow.as_mut_slice(),
                size,
                size.width,
                PixelFormat::Argb8888,
            ) else {
                return Err(SurfaceError::BufferTooSmall {
                    required: usize::try_from(required_words(size, size.width))
                        .unwrap_or(usize::MAX),
                    actual: shadow.len(),
                });
            };
            paint(&mut canvas);
        }

        {
            let mut frame = surface.acquire()?;
            if let Some(view) = PixelView::new(shadow.as_slice(), size, size.width) {
                Canvas::new(&mut frame).copy_from(&view, &whole);
            }
        }
        surface.present(&whole)
    }
}

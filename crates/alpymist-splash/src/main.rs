//! The Alpymist boot splash.
//!
//! The first thing the machine draws, and the thing the installer and the
//! login screen then take over from. It renders through Denise's software
//! rasteriser, so it needs no GPU, no compositor and no desktop — which is what
//! makes it usable this early in boot and identical on every hardware tier.
//!
//! On a machine, `OpenRC` starts it at the beginning of boot, with
//! `--no-default-features --features drm`, and it leaves by itself when the
//! installer or greetd starts; see `handover`.
//!
//! Built for a desktop window by default, so it can be iterated on in seconds:
//!
//! ```sh
//! cargo run -p alpymist-splash
//! ```

#![forbid(unsafe_code)]

// Only the DRM build hands over; the window build has nothing to watch for.
#[cfg_attr(not(feature = "drm"), allow(dead_code))]
mod handover;
mod scene;

use alpymist_ui::render::{colour, paint_backdrop, paint_badge};
use alpymist_ui::typeface::{self, Typeface};
use denise::geom::Point;
use denise::painter::Pen;
use denise_render::Canvas;
use scene::{Scene, TAGLINE, WORDMARK};

/// The splash application.
struct Splash {
    scene: Scene,
    face: Typeface,
}

impl Splash {
    fn new(width: u32, height: u32) -> Self {
        Self {
            scene: Scene::new(width, height),
            face: typeface::load(),
        }
    }

    /// Draw the whole splash into `canvas`.
    fn draw(&mut self, canvas: &mut Canvas<'_>) {
        let size = canvas.size();
        self.scene.resize(size.width, size.height);
        paint_backdrop(canvas, &self.scene.backdrop);

        let layout = self.scene.layout;
        let palette = self.scene.palette;
        // Bitmap scales are glyph-cell multiples; a real font wants pixel
        // heights, and a cell is eight pixels tall.
        let wordmark_px = u16::try_from(layout.wordmark_scale * 8).unwrap_or(96);
        let tagline_px = u16::try_from(layout.tagline_scale * 8).unwrap_or(16);

        paint_badge(canvas, layout.badge_at, layout.badge_size, &palette);

        let mut pen = Pen::new(canvas);
        self.face.draw(
            &mut pen,
            Point::new(layout.wordmark_at.0, layout.wordmark_at.1),
            wordmark_px,
            WORDMARK,
            colour(palette.ink),
        );
        self.face.draw(
            &mut pen,
            Point::new(layout.tagline_at.0, layout.tagline_at.1),
            tagline_px,
            TAGLINE,
            colour(palette.ink_dim),
        );
    }
}

#[cfg(all(feature = "winit", not(feature = "drm")))]
mod window {
    use super::Splash;
    use denise::{Color, DamageTracker, Frame, InputEvent, Rect};
    use denise_render::Canvas;

    impl denise_winit::DeniseApp for Splash {
        fn update(&mut self, _events: &[InputEvent], _damage: &mut DamageTracker) {}

        fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
            let mut canvas = Canvas::new(frame);
            canvas.clear(Color::rgb(0, 0, 0));
            self.draw(&mut canvas);
        }
    }

    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        use denise::geom::Size;
        use denise_winit::{WindowConfig, run};

        let size = Size::new(1280, 800);
        let config = WindowConfig {
            title: "Alpymist".into(),
            size,
            ..WindowConfig::default()
        };
        run(config, Splash::new(size.width, size.height))?;
        Ok(())
    }
}

/// On the machine: KMS, from early in boot until something takes over.
#[cfg(feature = "drm")]
mod console {
    use super::Splash;
    use crate::handover::{self, Reason};
    use denise::Surface;
    use denise::geom::Rect;
    use denise_drm::{DrmSurface, SurfaceConfig};
    use denise_evdev::Console;
    use denise_render::Canvas;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::io::Write as _;
    use std::os::unix::fs::MetadataExt as _;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// How often to look at the boot's progress. Short enough that the next
    /// screen does not wait on it; each look reads a few small directories.
    const LOOK_EVERY: Duration = Duration::from_millis(50);

    /// A display the splash has drawn on, and which device node it was.
    struct Shown {
        /// Held, not read: dropping it gives the display up.
        _surface: DrmSurface,
        node: Option<(PathBuf, u64)>,
    }

    impl Shown {
        /// Whether the device node it drew on has gone or been replaced.
        ///
        /// Early in boot the display is simpledrm, the firmware's framebuffer.
        /// When udev loads the real driver, simpledrm's device is removed and
        /// the real one registered, often under the same name: what was on the
        /// screen goes with it, so the splash has to notice and draw again.
        fn replaced(&self) -> bool {
            self.node.as_ref().is_some_and(|(path, ino)| {
                std::fs::metadata(path).map_or(true, |m| m.ino() != *ino)
            })
        }
    }

    fn show(splash: &mut Splash) -> Option<Shown> {
        let mut surface = DrmSurface::open(SurfaceConfig::default()).ok()?;
        let size = surface.size();
        let node = surface.card().path().and_then(|path: &Path| {
            std::fs::metadata(path)
                .ok()
                .map(|m| (path.to_path_buf(), m.ino()))
        });
        {
            let mut frame = surface.acquire().ok()?;
            let mut canvas = Canvas::new(&mut frame);
            splash.draw(&mut canvas);
        }
        surface.present(&[Rect::from_size(size)]).ok()?;
        eprintln!(
            "splash: {}x{} on {}",
            size.width,
            size.height,
            node.as_ref()
                .map_or("an unnamed device".into(), |(p, _)| p.display().to_string())
        );
        Some(Shown {
            _surface: surface,
            node,
        })
    }

    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [SIGTERM, SIGINT, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&stop))?;
        }

        // Graphics mode first: it stops the console drawing boot messages even
        // before there is a display device to draw the splash on, and keeps
        // them hidden while one driver replaces another.
        let mut console = Console::open_if_present();
        if let Some(console) = console.as_mut() {
            let _ = console.graphics_mode();
        }

        let started = Instant::now();
        let run = Path::new("/run/openrc");
        let mut splash: Option<Splash> = None;
        let mut shown: Option<Shown> = None;

        let mut looks = 0u32;
        let reason = loop {
            if stop.load(Ordering::Relaxed) {
                break Reason::Signal;
            }
            // The services are a couple of directory reads and are looked at
            // every time; walking every process for a getty costs more and
            // matters less, so it happens every half second.
            looks = looks.wrapping_add(1);
            let processes = if looks % 10 == 0 {
                handover::processes()
            } else {
                Vec::new()
            };
            if let Some(reason) = handover::decide(
                &handover::services(run),
                processes.iter().map(String::as_str),
                started.elapsed(),
            ) {
                break reason;
            }
            if shown.as_ref().is_some_and(Shown::replaced) {
                eprintln!("splash: the display device changed; drawing again");
                shown = None;
            }
            if shown.is_none() {
                // Composed once the size is known, which is once there is a
                // display; loading the font is the slow part on old machines.
                let splash = splash.get_or_insert_with(|| Splash::new(1, 1));
                shown = show(splash);
            }
            std::thread::sleep(LOOK_EVERY);
        };
        eprintln!(
            "splash: leaving after {:.1}s ({reason:?})",
            started.elapsed().as_secs_f32()
        );

        // Blank the console before handing it back, so the moment between the
        // splash and the next screen is black rather than the boot messages
        // that were hidden underneath.
        if reason.blanks_console()
            && let Ok(mut tty) = std::fs::OpenOptions::new().write(true).open("/dev/tty0")
        {
            let _ = tty.write_all(b"\x1b[H\x1b[2J\x1b[3J");
        }
        drop(shown);
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        Ok(())
    }
}

fn main() {
    #[cfg(feature = "drm")]
    let result = console::main();
    #[cfg(all(feature = "winit", not(feature = "drm")))]
    let result = window::main();
    #[cfg(not(any(feature = "winit", feature = "drm")))]
    let result: Result<(), Box<dyn std::error::Error>> =
        Err("built without a backend; enable --features winit or drm".into());

    if let Err(e) = result {
        eprintln!("alpymist-splash: {e}");
        std::process::exit(1);
    }
}

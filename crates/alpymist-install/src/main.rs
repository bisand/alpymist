//! The Alpymist installer.
//!
//! On a real machine this draws straight to DRM/KMS with no compositor. During
//! development it opens a window, so the same code can be walked through in
//! seconds rather than through a three-minute image rebuild:
//!
//! ```sh
//! cargo run -p alpymist-install
//! ```
//!
//! Arrows move, Space chooses, Enter continues, Esc goes back, F10 quits.
//! Set `ALPYMIST_FONT` to a Fira Mono TTF to preview with the real typeface.

#![forbid(unsafe_code)]

#[cfg(all(feature = "winit", not(feature = "drm")))]
mod run {
    use alpymist_core::Tier;
    use alpymist_install::answers::Answers;
    use alpymist_install::app::{App, action_for};
    use denise::{Color, DamageTracker, Frame, InputEvent, Rect};
    use denise_render::Canvas;
    use denise_winit::{DeniseApp, WindowConfig, run};

    /// Wraps the installer so the backend can drive it.
    struct Host {
        app: App,
    }

    impl DeniseApp for Host {
        fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
            let mut changed = false;
            for event in events {
                // Recomputed per event: moving onto a field with an arrow key
                // changes how the next keystroke should be read.
                let editing_text = self.app.focused_field().is_some();
                if let Some(action) = action_for(event, editing_text) {
                    self.app.act(action);
                    changed = true;
                }
            }
            // Any action can change the whole panel — the cursor, the button
            // states and the advisory line all move together — so there is
            // nothing to gain from tracking finer damage here.
            if changed {
                damage.add_full();
            }
        }

        fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
            let mut canvas = Canvas::new(frame);
            canvas.clear(Color::rgb(0, 0, 0));
            self.app.draw(&mut canvas);
        }
    }

    /// Open a window and run the installer in it.
    ///
    /// # Errors
    /// Returns whatever the backend could not do.
    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        use denise::geom::Size;

        // Probing needs Linux; on a development machine there is nothing to
        // probe, so the Desktop screen simply offers the choice outright.
        let detected_tier = probe_tier();
        let answers = Answers {
            detected_tier,
            ..Answers::default()
        };

        let size = Size::new(1280, 800);
        let app = App::new(answers, size.width, size.height);
        eprintln!("{}", app.face.status.describe());
        match detected_tier {
            Some(tier) => eprintln!("hardware: reports {tier:?}"),
            None => eprintln!("hardware: not probed on this platform"),
        }

        let config = WindowConfig {
            title: "Alpymist Installer".into(),
            size,
            ..WindowConfig::default()
        };
        run(config, Host { app })?;
        Ok(())
    }

    /// Ask the hardware what it can drive, where that is possible.
    fn probe_tier() -> Option<Tier> {
        alpymist_hwprobe::probe()
            .ok()
            .map(|caps| alpymist_core::select_tier(&caps).tier)
    }
}

/// Running on the machine being installed: KMS for output, evdev for input.
///
/// No compositor, no window system and no GPU driver stack — Denise rasterises
/// on the CPU and page-flips dumb buffers straight to the scanout engine. That
/// is what lets this run before any desktop exists, and why it looks the same on
/// every hardware tier.
#[cfg(feature = "drm")]
mod drm_run {
    use alpymist_core::Tier;
    use alpymist_install::answers::Answers;
    use alpymist_install::app::{App, action_for};
    use denise::geom::Rect;
    use denise::{InputEvent, InputSource, Surface};
    use denise_drm::{DrmSurface, SurfaceConfig};
    use denise_evdev::{Console, InputBackend};
    use denise_render::Canvas;
    use std::time::{Duration, Instant};

    /// Run the installer on the console.
    ///
    /// # Errors
    /// Returns whatever the display or input layer could not do.
    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        // Take the VT out of text mode first, so the boot messages stop being
        // drawn over and the kernel stops echoing keystrokes we are about to
        // read ourselves. Absent on a system with no VTs, which is fine.
        let mut console = Console::open_if_present();
        if let Some(console) = console.as_mut() {
            console.graphics_mode()?;
            console.mute_keyboard()?;
        }

        let result = run(&mut console);

        // Always hand the console back, including on the error path: leaving a
        // VT in graphics mode with a muted keyboard is a machine that looks
        // dead to whoever is sitting at it.
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        result
    }

    fn run(console: &mut Option<Console>) -> Result<(), Box<dyn std::error::Error>> {
        let _ = console;
        let mut surface = DrmSurface::open(SurfaceConfig::default())?;
        let size = surface.size();

        eprintln!("display: {}x{} via DRM/KMS", size.width, size.height);

        // Input devices are not necessarily there when this starts. A USB
        // keyboard enumerating a second after boot is ordinary, and on the
        // hardware this targets it is close to expected. Quitting because
        // nothing is plugged in *yet* would strand the user at a blank screen
        // with no way to find out why, so the installer draws first and keeps
        // looking.
        let mut complained = false;
        let mut input = open_input(size, &mut complained);

        let detected_tier = probe_tier();
        let answers = Answers {
            detected_tier,
            ..Answers::default()
        };
        let mut app = App::new(answers, size.width, size.height);
        eprintln!("{}", app.face.status.describe());

        let mut events: Vec<InputEvent> = Vec::new();
        let mut next_retry = Instant::now() + RETRY_EVERY;
        loop {
            events.clear();
            match input.as_mut() {
                Some(input) => input.poll(&mut events),
                None if Instant::now() >= next_retry => {
                    input = open_input(size, &mut complained);
                    next_retry = Instant::now() + RETRY_EVERY;
                }
                None => {}
            }
            for event in &events {
                let editing_text = app.focused_field().is_some();
                if let Some(action) = action_for(event, editing_text) {
                    app.act(action);
                }
            }
            if app.quitting {
                return Ok(());
            }

            {
                let mut frame = surface.acquire()?;
                let mut canvas = Canvas::new(&mut frame);
                app.draw(&mut canvas);
            }
            surface.present(&[Rect::from_size(size)])?;
        }
    }

    /// How often to look again when no input device has appeared yet.
    const RETRY_EVERY: Duration = Duration::from_secs(1);

    /// Open whatever input devices exist, or report that none do yet.
    ///
    /// Returns `None` rather than failing: see the call site.
    ///
    /// `complained` makes the absence of input a *state* rather than an event.
    /// Reporting it on every retry writes a line a second for as long as the
    /// machine is on — which on an installer left at the welcome screen
    /// overnight is tens of thousands of identical lines, in a log whose only
    /// job is to be readable when something has gone wrong.
    fn open_input(size: denise::geom::Size, complained: &mut bool) -> Option<InputBackend> {
        match InputBackend::open_all(size) {
            Ok(mut input) => {
                let (layout, source) = input.set_layout_from_system();
                eprintln!("keyboard: {} (from {source:?})", layout.name);
                *complained = false;
                Some(input)
            }
            Err(why) => {
                if !*complained {
                    eprintln!("input: {why}; drawing anyway and looking again every second");
                    *complained = true;
                }
                None
            }
        }
    }

    /// Ask the hardware what desktop it can drive.
    fn probe_tier() -> Option<Tier> {
        alpymist_hwprobe::probe()
            .ok()
            .map(|caps| alpymist_core::select_tier(&caps).tier)
    }
}

#[cfg(all(feature = "winit", not(feature = "drm")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run::main()
}

#[cfg(feature = "drm")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    drm_run::main()
}

#[cfg(not(any(feature = "winit", feature = "drm")))]
fn main() {
    eprintln!("alpymist-install was built without a backend; enable --features winit or drm");
    std::process::exit(2);
}

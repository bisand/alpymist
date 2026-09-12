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

#[cfg(feature = "winit")]
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
                if let Some(action) = action_for(event) {
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

#[cfg(feature = "winit")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run::main()
}

#[cfg(not(feature = "winit"))]
fn main() {
    eprintln!("alpymist-install was built without a backend; enable --features winit or drm");
    std::process::exit(2);
}

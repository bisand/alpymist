//! The Alpymist boot splash.
//!
//! The first thing the machine draws, and the thing the installer then takes
//! over from without a visible seam. It renders through Denise's software
//! rasteriser, so it needs no GPU, no compositor and no desktop — which is what
//! makes it usable this early in boot and identical on every hardware tier.
//!
//! Built for a desktop window by default, so it can be iterated on in seconds:
//!
//! ```sh
//! cargo run -p alpymist-splash
//! ```

#![forbid(unsafe_code)]

mod scene;

use alpymist_ui::render::{colour, paint_backdrop};
use denise::geom::Point;
use denise::{Color, DamageTracker, Frame, InputEvent, Rect};
use denise_render::Canvas;
use denise_render::font::BUILT_IN;
use scene::{Scene, TAGLINE, WORDMARK};

/// The splash application.
struct Splash {
    scene: Scene,
}

impl Splash {
    fn new(width: u32, height: u32) -> Self {
        Self {
            scene: Scene::new(width, height),
        }
    }

    /// Draw the whole splash into `canvas`.
    fn draw(&mut self, canvas: &mut Canvas<'_>) {
        let size = canvas.size();
        if self.scene.resize(size.width, size.height) {
            // Recomposed for a new size; everything below is drawn fresh anyway.
        }

        paint_backdrop(canvas, &self.scene.backdrop);

        let layout = self.scene.layout;
        let palette = self.scene.palette;

        canvas.draw_text(
            &BUILT_IN,
            Point::new(layout.wordmark_at.0, layout.wordmark_at.1),
            layout.wordmark_scale,
            WORDMARK,
            colour(palette.ink),
        );
        canvas.draw_text(
            &BUILT_IN,
            Point::new(layout.tagline_at.0, layout.tagline_at.1),
            layout.tagline_scale,
            TAGLINE,
            colour(palette.ink_dim),
        );
    }
}

#[cfg(feature = "winit")]
impl denise_winit::DeniseApp for Splash {
    fn update(&mut self, _events: &[InputEvent], _damage: &mut DamageTracker) {}

    fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
        let mut canvas = Canvas::new(frame);
        canvas.clear(Color::rgb(0, 0, 0));
        self.draw(&mut canvas);
    }
}

#[cfg(feature = "winit")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
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

#[cfg(not(feature = "winit"))]
fn main() {
    eprintln!("alpymist-splash was built without a backend; enable --features winit or drm");
    std::process::exit(2);
}

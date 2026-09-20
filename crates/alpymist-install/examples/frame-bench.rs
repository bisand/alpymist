//! How long one whole installer frame costs on this machine.
//!
//! The DRM loop redraws everything every frame. This draws exactly what that
//! loop draws -- scenery, panel, every glyph and the cursor -- into ordinary
//! memory, so the rasterising cost can be told apart from the cost of writing
//! to the scanout mapping. It opens no display and needs no privileges, so it
//! runs over ssh with the desktop up.
//!
//! ```sh
//! cargo run --release -p alpymist-install --no-default-features \
//!     --example frame-bench -- 1366 768
//! ```

use alpymist_install::answers::Answers;
use alpymist_install::app::{Action, App};
use alpymist_install::execute::Mode;
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;
use std::time::Instant;

/// Rounds per measurement, after one untimed warm-up.
const ROUNDS: u32 = 20;

/// Time `body` over [`ROUNDS`] rounds and report the mean.
fn time(label: &str, mut body: impl FnMut()) {
    body();
    let start = Instant::now();
    for _ in 0..ROUNDS {
        body();
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ROUNDS);
    let rate = if ms > 0.0 {
        format!("{:7.1} fps", 1000.0 / ms)
    } else {
        "        -".to_string()
    };
    println!("  {label:34} {ms:9.2} ms {rate}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let parse = |i: usize, fallback: u32| -> u32 {
        args.get(i).and_then(|a| a.parse().ok()).unwrap_or(fallback)
    };
    let width = parse(1, 1366);
    let height = parse(2, 768);

    let Ok(len) = usize::try_from(u64::from(width) * u64::from(height)) else {
        eprintln!("{width}x{height} is too large for this machine");
        return;
    };

    let mut disks = alpymist_install::disks::discover(&alpymist_install::safety::gather());
    if disks.is_empty() {
        disks = alpymist_install::disks::sample();
    }
    let answers = Answers {
        disks,
        typed_by_os: true,
        ..Answers::default()
    }
    .with_defaults();
    let mut app = App::with_mode(answers, width, height, Mode::DryRun);

    let mut pixels = vec![0u32; len];
    let Some(mut canvas) = Canvas::from_pixels(
        &mut pixels,
        Size::new(width, height),
        width,
        PixelFormat::Argb8888,
    ) else {
        eprintln!("could not make a canvas of that size");
        return;
    };

    println!("{width}x{height}, {ROUNDS} rounds, mean per round\n");
    time("a frame: the welcome screen", || app.draw(&mut canvas));

    // A screen with a list on it, which is most of the wizard and carries far
    // more text than the welcome screen does.
    app.act(Action::Advance);
    time("a frame: the first list screen", || app.draw(&mut canvas));

    println!("\n  {len} pixels a frame");
}

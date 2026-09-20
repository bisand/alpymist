//! How long a frame of the installer's scenery actually costs.
//!
//! The installer repaints its whole backdrop every frame, straight into the
//! DRM scanout buffer. This paints the same scene into ordinary memory, which
//! separates two costs that look identical on a slow machine: the CPU work of
//! rasterising it, and writing it to a write-combining mapping. It needs no
//! display and no privileges, so it runs over ssh with the desktop up.
//!
//! ```sh
//! cargo run --release -p alpymist-wallpaper --example backdrop-bench -- 1366 768
//! ```
//!
//! The last line is the one that matters: painting the backdrop against
//! copying a cached one. That ratio is what caching it would buy.

use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::chrome::Chrome;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{paint_backdrop, paint_panel};
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;
use std::time::Instant;

/// The seed the installer and the splash both pass, so this is that picture.
const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

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
    println!("{width}x{height}, {ROUNDS} rounds, mean per round\n");

    let palette = Palette::alpymist();
    let chrome = Chrome::for_screen(width, height);

    // Composing is a one-off in the installer -- only on resize -- so it is
    // reported separately rather than counted against a frame.
    let start = Instant::now();
    let backdrop = Backdrop::compose(width, height, &palette, SCENE_SEED);
    println!(
        "  {:34} {:9.2} ms   (once, on resize)",
        "Backdrop::compose",
        start.elapsed().as_secs_f64() * 1000.0
    );
    println!("  {} ridges\n", backdrop.ridge_count());

    let mut pixels = vec![0u32; len];
    {
        let Some(mut canvas) = Canvas::from_pixels(
            &mut pixels,
            Size::new(width, height),
            width,
            PixelFormat::Argb8888,
        ) else {
            eprintln!("could not make a canvas of that size");
            return;
        };
        time("paint_backdrop", || paint_backdrop(&mut canvas, &backdrop));
        time("paint_panel", || {
            paint_panel(&mut canvas, &chrome, &palette);
        });
        time("both, as a frame draws them", || {
            paint_backdrop(&mut canvas, &backdrop);
            paint_panel(&mut canvas, &chrome, &palette);
        });
    }

    // What the proposed fix costs instead: one sequential copy of a backdrop
    // painted once. Into cached memory here; into the scanout mapping it is
    // still sequential, which is the case write-combining is built for.
    let source = pixels.clone();
    let mut target = vec![0u32; len];
    time("blit a cached backdrop", || {
        target.copy_from_slice(&source);
    });
    println!("\n  {len} pixels a frame");
}

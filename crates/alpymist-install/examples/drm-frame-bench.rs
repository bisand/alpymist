//! Where an installer frame's time actually goes, on the real display.
//!
//! [`frame-bench`](frame-bench) paints into ordinary memory and reports about
//! 25 fps on hardware that visibly runs at one or two. This splits a real frame
//! into the three things it is made of, so the missing time can be pinned on
//! one of them rather than inferred:
//!
//! * `acquire` -- waiting for the previous page flip to retire.
//! * drawing into the scanout mapping, which on i915 is write-combining.
//! * drawing the identical frame into ordinary memory, for comparison.
//!
//! It also times blitting a ready-made image into the scanout buffer, which is
//! what a shadow-buffer fix would cost instead of painting there directly.
//!
//! Needs the display: stop the desktop first, and expect to need root to take
//! DRM master from outside a VT.
//!
//! ```sh
//! doas rc-service greetd stop
//! doas ALPYMIST_DRY_RUN=1 ./drm-frame-bench
//! doas rc-service greetd start
//! ```

use alpymist_install::answers::Answers;
use alpymist_install::app::App;
use alpymist_install::execute::Mode;
use denise::geom::{Rect, Size};
use denise::{PixelFormat, Surface};
use denise_drm::SurfaceConfig;
use denise_render::Canvas;
use std::time::{Duration, Instant};

/// Rounds per measurement, after one untimed warm-up.
const ROUNDS: u32 = 20;

/// Mean of `total` over [`ROUNDS`] rounds, as milliseconds.
fn mean_ms(total: Duration) -> f64 {
    total.as_secs_f64() * 1000.0 / f64::from(ROUNDS)
}

/// Report one measurement.
fn report(label: &str, ms: f64) {
    let rate = if ms > 0.0 {
        format!("{:7.1} fps", 1000.0 / ms)
    } else {
        "        -".to_string()
    };
    println!("  {label:38} {ms:9.2} ms {rate}");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut surface = alpymist_ui::display::open_patiently(
        SurfaceConfig::default(),
        alpymist_ui::display::PATIENCE,
    )?;
    let size = surface.size();
    let everything = [Rect::from_size(size)];
    println!("display: {}x{} via DRM/KMS\n", size.width, size.height);

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
    let mut app = App::with_mode(answers, size.width, size.height, Mode::DryRun);

    // A real frame, split into waiting and drawing. One loop, timed in two
    // parts, so the numbers add up to what the installer actually does.
    let mut waiting = Duration::ZERO;
    let mut drawing = Duration::ZERO;
    for round in 0..=ROUNDS {
        let before = Instant::now();
        let mut frame = surface.acquire()?;
        let acquired = Instant::now();
        {
            let mut canvas = Canvas::new(&mut frame);
            app.draw(&mut canvas);
        }
        let drawn = Instant::now();
        drop(frame);
        surface.present(&everything)?;
        // Round zero is the warm-up: the first flip has nothing to wait for.
        if round > 0 {
            waiting += acquired - before;
            drawing += drawn - acquired;
        }
    }
    report("acquire: waiting for the flip", mean_ms(waiting));
    report("draw: into the scanout mapping", mean_ms(drawing));

    // The same frame into ordinary memory. Same code, same pixels; the only
    // difference is where they land.
    let Ok(len) = usize::try_from(u64::from(size.width) * u64::from(size.height)) else {
        return Err("this display is too large for this machine".into());
    };
    let mut pixels = vec![0u32; len];
    {
        let Some(mut canvas) = Canvas::from_pixels(
            &mut pixels,
            Size::new(size.width, size.height),
            size.width,
            PixelFormat::Xrgb8888,
        ) else {
            return Err("could not make a canvas of that size".into());
        };
        app.draw(&mut canvas);
        let start = Instant::now();
        for _ in 0..ROUNDS {
            app.draw(&mut canvas);
        }
        report("draw: into ordinary memory", mean_ms(start.elapsed()));
    }

    // What a shadow buffer would cost: paint once in memory, copy the result.
    //
    // Row by row, because the scanout stride is padded -- 1376 words for a
    // 1366-pixel panel on this machine -- so the buffer is wider than the
    // picture and a single copy_from_slice does not line up.
    let height = size.height as usize;
    let width = size.width as usize;
    let stride = {
        let mut frame = surface.acquire()?;
        let words = frame.pixels_mut().len();
        drop(frame);
        surface.present(&everything)?;
        words / height
    };
    println!("\n  scanout stride {stride} words for a {width}-pixel panel\n");

    // `rows` of `span` words each, which is what a damage rectangle costs.
    let mut blit =
        |label: &str, span: usize, rows: usize| -> Result<(), Box<dyn std::error::Error>> {
            let mut copying = Duration::ZERO;
            for round in 0..=ROUNDS {
                let mut frame = surface.acquire()?;
                let dst = frame.pixels_mut();
                let start = Instant::now();
                for y in 0..rows {
                    let d = y * stride;
                    let s = y * width;
                    dst[d..d + span].copy_from_slice(&pixels[s..s + span]);
                }
                let taken = start.elapsed();
                drop(frame);
                surface.present(&everything)?;
                if round > 0 {
                    copying += taken;
                }
            }
            report(label, mean_ms(copying));
            Ok(())
        };

    blit("blit: whole screen into scanout", width, height)?;
    // The panel is about two thirds of the screen each way; the cursor is a
    // small square. These are what damage-limited copies would actually move.
    blit(
        "blit: a panel-sized rectangle",
        width * 2 / 3,
        height * 2 / 3,
    )?;
    blit("blit: a 32x32 cursor", 32, 32)?;

    // The shipping path, end to end: paint into ordinary memory, copy it over,
    // flip. This is the number the installer actually runs at, flip wait and
    // all, so it is the one to compare against a frame's 1/60th of a second.
    drop(blit);
    drop(surface);
    let mut screen = alpymist_ui::display::Screen::open_patiently(
        SurfaceConfig::default(),
        alpymist_ui::display::PATIENCE,
    )?;
    let mut whole = Duration::ZERO;
    for round in 0..=ROUNDS {
        let start = Instant::now();
        screen.present_with(|canvas| app.draw(canvas))?;
        if round > 0 {
            whole += start.elapsed();
        }
    }
    println!();
    report("a whole frame through Screen", mean_ms(whole));

    println!("\n  {len} pixels a frame");
    Ok(())
}

//! Render the splash offscreen to a PNG.
//!
//! `cargo run -p alpymist-splash --example snapshot -- out.png [width height]`
//!
//! Useful for reviewing the design without a display, and for a CI check that
//! the splash still draws something sane on the sizes we care about.

use alpymist_splash_scene::{Scene, TAGLINE, WORDMARK};
use alpymist_ui::render::{colour, paint_backdrop};
use denise::PixelFormat;
use denise::geom::{Point, Size};
use denise_render::Canvas;
use denise_render::font::BUILT_IN;

// The scene module lives in the binary crate, so the example includes it
// directly rather than depending on a library that does not exist.
// Only part of the module is used here; the binary uses the rest.
#[allow(dead_code)]
#[path = "../src/scene.rs"]
mod alpymist_splash_scene;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "splash.png".into());
    let width: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(1280);
    let height: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(800);

    let mut pixels = vec![0u32; (width as usize) * (height as usize)];
    let mut canvas = Canvas::from_pixels(
        &mut pixels,
        Size::new(width, height),
        width,
        PixelFormat::Argb8888,
    )
    .expect("buffer large enough for the requested size");

    let scene = Scene::new(width, height);
    paint_backdrop(&mut canvas, &scene.backdrop);
    canvas.draw_text(
        &BUILT_IN,
        Point::new(scene.layout.wordmark_at.0, scene.layout.wordmark_at.1),
        scene.layout.wordmark_scale,
        WORDMARK,
        colour(scene.palette.ink),
    );
    canvas.draw_text(
        &BUILT_IN,
        Point::new(scene.layout.tagline_at.0, scene.layout.tagline_at.1),
        scene.layout.tagline_scale,
        TAGLINE,
        colour(scene.palette.ink_dim),
    );

    // ARGB8888 words out, RGBA bytes in.
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for px in &pixels {
        rgba.extend_from_slice(&[
            ((px >> 16) & 0xFF) as u8,
            ((px >> 8) & 0xFF) as u8,
            (px & 0xFF) as u8,
            0xFF,
        ]);
    }

    let file = std::fs::File::create(&path).expect("create output file");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("write png header")
        .write_image_data(&rgba)
        .expect("write png data");

    println!("wrote {path} ({width}x{height})");
}

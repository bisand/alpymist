//! Render the splash offscreen to a PNG.
//!
//! `cargo run -p alpymist-splash --example snapshot -- out.png [width height]`
//!
//! Useful for reviewing the design without a display, and for a CI check that
//! the splash still draws something sane on the sizes we care about.

use alpymist_splash_scene::{Scene, TAGLINE, WORDMARK};
use alpymist_ui::render::{colour, paint_backdrop, paint_badge};
use alpymist_ui::typeface;
use denise::PixelFormat;
use denise::geom::{Point, Size};
use denise::painter::Pen;
use denise_render::Canvas;

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

    paint_badge(
        &mut canvas,
        scene.layout.badge_at,
        scene.layout.badge_size,
        &scene.palette,
    );

    let mut face = typeface::load();
    eprintln!("{}", face.status.describe());

    // Bitmap scales are glyph-cell multiples; a real font wants pixel heights.
    let wordmark_px = u16::try_from(scene.layout.wordmark_scale * 8).unwrap_or(96);
    let tagline_px = u16::try_from(scene.layout.tagline_scale * 8).unwrap_or(16);

    let mut pen = Pen::new(&mut canvas);
    face.draw(
        &mut pen,
        Point::new(scene.layout.wordmark_at.0, scene.layout.wordmark_at.1),
        wordmark_px,
        WORDMARK,
        colour(scene.palette.ink),
    );
    face.draw(
        &mut pen,
        Point::new(scene.layout.tagline_at.0, scene.layout.tagline_at.1),
        tagline_px,
        TAGLINE,
        colour(scene.palette.ink_dim),
    );
    drop(pen);

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

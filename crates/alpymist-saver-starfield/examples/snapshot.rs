//! Write frames of the starfield to PNG, to look at without a compositor.
//!
//! `cargo run -p alpymist-saver-starfield --example snapshot -- DIR [W H BLOCK]`
//!
//! Every frame is drawn, not just the ones written out: the streak behind a
//! star is the path it took since the last frame, so a picture stepped straight
//! to a minute in would show no streaks and an asteroid field that never
//! happened.
#![allow(missing_docs)]

use alpymist_screensaver::paint::Painting;
use alpymist_screensaver::scene::expand;
use denise::geom::Size;
use std::io::BufWriter;

#[path = "../src/picture.rs"]
mod picture;

/// Seconds in: one of open sky, and then a field arriving, passing and gone.
///
/// The flight is fixed by [`picture::FLIGHT_SEED`], so these are the same
/// moments every run — which is what makes a snapshot worth comparing against
/// the last one. Change the seed or the arithmetic and they are moments of
/// some other flight; pick new ones by eye.
const AT: [u64; 6] = [2_000, 14_000, 16_000, 18_000, 20_000, 22_000];

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1366);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(768);
    let block: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(4);
    std::fs::create_dir_all(&dir).expect("a directory to write into");

    let look = picture::Look {
        block,
        ..picture::Look::default()
    };
    let output = Size::new(width, height);
    let mut scene = picture::Starfield::compose(output, &look);
    let step = scene.interval_ms().max(1);
    let mut row = Vec::new();
    let last = AT.iter().copied().max().unwrap_or(0);
    for frame in 1..=(last / step) {
        let at = frame * step;
        let small = scene.small();
        let block = scene.block();
        // Once per frame, and the pixels kept: asking for the same frame twice
        // advances the flight by no time at all, and a star that has not moved
        // leaves no streak — which is most of what there is to look at.
        let drawn = scene.frame(at);
        if !AT.iter().any(|want| want / step == frame) {
            continue;
        }
        let mut pixels = vec![0u32; width as usize * height as usize];
        expand(drawn, small, block, &mut row, |y, line| {
            let at = y as usize * width as usize;
            if let Some(dst) = pixels.get_mut(at..at + width as usize) {
                let n = dst.len().min(line.len());
                dst[..n].copy_from_slice(&line[..n]);
            }
        });
        let path = format!("{dir}/starfield-{at:07}.png");
        let file = std::fs::File::create(&path).expect("a file");
        let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut rgb = Vec::with_capacity(pixels.len() * 3);
        for px in &pixels {
            let [_, r, g, b] = px.to_be_bytes();
            rgb.extend_from_slice(&[r, g, b]);
        }
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&rgb))
            .expect("a PNG");
        println!("{path}");
    }
}

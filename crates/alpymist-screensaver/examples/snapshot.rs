//! Write frames of the screensaver to PNG, to look at without a compositor.
//!
//! `cargo run -p alpymist-screensaver --example snapshot -- DIR [WIDTH HEIGHT]`
#![allow(missing_docs)]

use alpymist_screensaver::saver::frame;
use denise::geom::Size;
use std::io::BufWriter;

/// Seconds into the animation each frame is taken at: the mist's cycles are
/// tens of seconds long, so frames a second apart would all look the same.
const AT: [u64; 6] = [0, 4_000, 11_000, 23_000, 37_000, 55_000];

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1366);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(768);
    let block: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(4);
    std::fs::create_dir_all(&dir).expect("a directory to write into");

    for at in AT {
        let pixels = frame(Size::new(width, height), block, at);
        let path = format!("{dir}/screensaver-{at:06}.png");
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

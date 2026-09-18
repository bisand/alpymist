//! Write frames of the mountains to PNG, to look at without a compositor.
//!
//! `cargo run -p alpymist-saver-mountains --example snapshot -- DIR [W H BLOCK]`
#![allow(missing_docs)]

use alpymist_screensaver::paint::Painting;
use alpymist_screensaver::scene::expand;
use denise::geom::Size;
use std::io::BufWriter;

#[path = "../src/picture.rs"]
mod picture;

/// Seconds in: far enough apart that the ranges have visibly travelled.
const AT: [u64; 6] = [0, 30_000, 90_000, 180_000, 300_000, 600_000];

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1366);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(768);
    let block: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(6);
    std::fs::create_dir_all(&dir).expect("a directory to write into");

    let look = picture::Look {
        block,
        ..picture::Look::default()
    };
    let output = Size::new(width, height);
    let mut scene = picture::Mountains::compose(output, &look);
    let mut row = Vec::new();
    for at in AT {
        let mut pixels = vec![0u32; width as usize * height as usize];
        let small = scene.small();
        let block = scene.block();
        let drawn = scene.frame(at);
        expand(drawn, small, block, &mut row, |y, line| {
            let at = y as usize * width as usize;
            if let Some(dst) = pixels.get_mut(at..at + width as usize) {
                let n = dst.len().min(line.len());
                dst[..n].copy_from_slice(&line[..n]);
            }
        });
        let path = format!("{dir}/mountains-{at:07}.png");
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

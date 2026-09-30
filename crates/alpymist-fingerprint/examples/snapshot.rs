//! Render the fingerprint window offscreen to PNGs, for review without a
//! Wayland session.
//!
//! `cargo run -p alpymist-fingerprint --example snapshot -- OUTDIR [SCALE]`
//!
//! Point `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of
//! Fira Sans and Symbols Nerd Font to preview with the real faces.

#![allow(clippy::many_single_char_names)]

use alpymist_fingerprint::enrol::Enrol;
use alpymist_fingerprint::view::{self, Layout, Target};
use alpymist_widget::Appearance;
use alpymist_widget::draw::Fonts;
use denise::geom::Size;
use denise::{BufferAge, Frame, PixelFormat};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("output directory");
    let mut appearance = Appearance::default();
    if let Ok(font) = std::env::var("ALPYMIST_MENU_FONT") {
        appearance.font = font;
    }
    if let Ok(font) = std::env::var("ALPYMIST_MENU_ICON_FONT") {
        appearance.icon_font = font;
    }
    let mut fonts = Fonts::load(&appearance);
    let reader = Some("Synaptics Metallica MIS".to_owned());

    let mut scenes: Vec<(&str, Enrol, Option<Target>)> = Vec::new();
    scenes.push((
        "empty",
        Enrol::new(reader.clone(), vec![]),
        Some(Target::Finger(3)),
    ));
    let mut e = Enrol::new(reader.clone(), vec!["right-index-finger".into()]);
    let _ = e.enrol(8);
    e.enroll_status("enroll-stage-passed", false);
    e.enroll_status("enroll-stage-passed", false);
    e.enroll_status("enroll-finger-not-centered", false);
    scenes.push(("enrolling", e, None));
    let mut e = Enrol::new(
        reader.clone(),
        vec!["right-index-finger".into(), "left-index-finger".into()],
    );
    let _ = e.test();
    e.verify_status("verify-match", true);
    scenes.push(("tested", e, Some(Target::Test)));
    let mut e = Enrol::new(None, vec![]);
    e.refused("No fingerprint reader was found.");
    scenes.push(("no-reader", e, None));

    for (name, state, hover) in &scenes {
        let height = Layout::height(&appearance, &mut fonts, state);
        let size = Size::new(view::WIDTH.unsigned_abs() * scale, height * scale);
        let layout = Layout::new(&appearance, &mut fonts, state, size, scale);
        let mut pixels = vec![0u32; (size.width * size.height) as usize];
        {
            let mut frame = Frame::new(
                &mut pixels,
                size,
                size.width,
                PixelFormat::Argb8888,
                BufferAge::Undefined,
            )
            .expect("frame");
            view::paint(&mut frame, &layout, &appearance, &mut fonts, state, *hover);
        }
        let file = std::fs::File::create(format!("{dir}/{name}.png")).expect("png");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = pixels
            .iter()
            .flat_map(|p| {
                let [a, r, g, b] = p.to_be_bytes();
                [r, g, b, a]
            })
            .collect();
        encoder
            .write_header()
            .expect("header")
            .write_image_data(&data)
            .expect("data");
    }
}

//! Render the popup offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-ai-usage --example snapshot -- outdir [scale]`
//!
//! Drives the real [`Popup`] and [`view::paint`] over sample readings. Point
//! `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of Fira
//! Sans and Symbols Nerd Font to preview with the real faces.

#![allow(clippy::many_single_char_names)]

use alpymist_ai_usage::popup::{Popup, Reading, Target, sample};
use alpymist_ai_usage::view::{self, Fonts, Layout};
use alpymist_widget::{Appearance, Key};
use denise::{BufferAge, Frame, PixelFormat};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("create output directory");

    let mut appearance = Appearance::default();
    if let Ok(font) = std::env::var("ALPYMIST_MENU_FONT") {
        appearance.font = font;
    }
    if let Ok(font) = std::env::var("ALPYMIST_MENU_ICON_FONT") {
        appearance.icon_font = font;
    }
    let mut fonts = Fonts::load(&appearance);
    for p in &fonts.problems {
        eprintln!("font: {p}");
    }

    let with = |r: Reading| {
        let mut p = Popup::new();
        p.update(r);
        p
    };
    let mut scenes: Vec<(&str, Popup)> = Vec::new();

    let mut p = with(sample());
    p.hover_over(Some(Target::Settings));
    scenes.push(("three", p));

    let mut one = sample();
    one.providers.truncate(1);
    one.providers[0].lines[1].used = Some(0.31);
    one.providers[0].lines[1].says = "31% used, resets in 3 d".into();
    let mut p = with(one);
    p.key(Key::Tab);
    scenes.push(("one-focus", p));

    let mut p = with(sample());
    p.click(Target::Refresh);
    scenes.push(("asking", p));

    scenes.push(("nothing", with(Reading::default())));
    scenes.push(("unread", Popup::new()));

    for (name, popup) in scenes {
        let layout = Layout::new(&appearance, &popup, scale);
        let (w, h) = (layout.size.width, layout.size.height);
        let mut pixels = vec![0u32; (w * h) as usize];
        let mut frame = Frame::new(
            &mut pixels,
            layout.size,
            w,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        )
        .unwrap();
        view::paint(&mut frame, &layout, &appearance, &mut fonts, &popup);
        eprintln!("{name}: {w}x{h}");

        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in pixels {
            let a = px >> 24;
            for shift in [16, 8, 0] {
                let c = (px >> shift) & 0xFF;
                rgb.push(u8::try_from(c + (0x50 * (255 - a)) / 255).unwrap_or(255));
            }
        }
        let path = format!("{dir}/{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
}

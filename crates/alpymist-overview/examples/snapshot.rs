//! Render the overview offscreen to PNGs, for review without a Wayland
//! session.
//!
//! `cargo run -p alpymist-overview --example snapshot -- outdir [width height [scale]]`
//!
//! Point `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies
//! of Fira Sans and Symbols Nerd Font to preview with the real faces.

use alpymist_overview::model::Overview;
use alpymist_overview::view::{self, Layout};
use alpymist_widget::Appearance;
use alpymist_widget::Key;
use alpymist_widget::draw::Fonts;
use denise::geom::Size;
use denise::{BufferAge, Frame, PixelFormat};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let mut number = |or: u32| args.next().and_then(|s| s.parse().ok()).unwrap_or(or);
    let (w, h, scale) = (number(1280), number(800), number(1));
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

    let size = Size::new(w, h);
    let opened = Overview::sample();
    let mut moved = Overview::sample();
    moved.key(Key::Right);
    moved.key(Key::Down);
    let _ = moved.hover(Some(3));
    // The mark half way from the first workspace to the one in the middle.
    let cells = Layout::new(&appearance, &moved, size, scale).cells;
    let gliding = view::between(cells[0], cells[4], 350);

    for (name, overview, mark) in [
        ("opened", &opened, None),
        ("moved", &moved, None),
        ("gliding", &moved, Some(gliding)),
    ] {
        // What is behind: a desktop's worth of something, to see the shade on.
        let mut pixels: Vec<u32> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                0xFF00_0000 | ((x * 90 / w + 20) << 16) | ((y * 90 / h + 40) << 8) | 0x70
            })
            .collect();
        let mut frame = Frame::new(
            &mut pixels,
            size,
            w,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        )
        .unwrap();
        view::paint(&mut frame, &appearance, &mut fonts, overview, scale, mark);

        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in pixels {
            for shift in [16, 8, 0] {
                rgb.push(u8::try_from((px >> shift) & 0xFF).unwrap_or(255));
            }
        }
        let path = format!("{dir}/{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        eprintln!("{path}");
    }
}

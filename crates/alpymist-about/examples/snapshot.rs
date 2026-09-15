//! Render the About box offscreen to PNGs, for review without a Wayland
//! session.
//!
//! `cargo run -p alpymist-about --example snapshot -- outdir [scale]`
//!
//! Point `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies
//! of Fira Sans and Symbols Nerd Font to preview with the real faces.

use alpymist_about::dialog::{Dialog, Key, Target};
use alpymist_about::info::About;
use alpymist_about::view::{self, Layout};
use alpymist_widget::Appearance;
use alpymist_widget::draw::Fonts;
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

    let mut scenes: Vec<(&str, Dialog)> = Vec::new();
    scenes.push(("dev", Dialog::new(About::sample())));

    let mut stable = About::sample();
    stable.version = Some("0.0.1-r13".into());
    stable.channel = Some(alpymist_core::Channel::Stable);
    for (_, v) in &mut stable.packages {
        if let Some((base, rest)) = v.split_once("_git")
            && let Some((_, release)) = rest.rsplit_once('-')
            && !base.starts_with("0.1")
        {
            *v = format!("{base}-{release}");
        }
    }
    let mut d = Dialog::new(stable);
    d.hover_over(Some(Target::Copy));
    let _ = d.finished_copy(Ok(()));
    scenes.push(("stable-copied", d));

    let mut d = Dialog::new(About::default());
    d.key(Key::Tab);
    d.key(Key::Tab);
    let _ = d.finished_copy(Err("Could not copy: wl-copy: not found".into()));
    scenes.push(("nothing-installed", d));

    for (name, dialog) in scenes {
        let layout = Layout::new(&appearance, &dialog, scale);
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
        view::paint(&mut frame, &layout, &appearance, &mut fonts, &dialog);
        eprintln!("{name}: {w}x{h}");

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
    }
}

//! Render the popup offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-ai-usage --example snapshot -- outdir [scale]`
//!
//! Drives the real [`Popup`] and [`view::paint`] over sample readings. Point
//! `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of Fira
//! Sans and Symbols Nerd Font to preview with the real faces.

#![allow(clippy::many_single_char_names)]

use alpymist_ai_usage::popup::{Command, Popup, Reading, Reply, Target, sample};
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
    p.hover_over(Some(Target::Switch(1)));
    scenes.push(("overview", p));

    // Turning one on that has no key yet: the field under its row.
    let mut p = with(sample());
    p.click(Target::Switch(3));
    p.finished(
        &Command::TurnOn("openrouter".into()),
        Reply::NeedsKeys(vec!["api-key".into()]),
    );
    for ch in "sk-or-v1-0123456789".chars() {
        p.text(ch);
    }
    scenes.push(("asking-for-a-key", p));

    // Turning one on whose vendor's tool is not installed.
    let mut p = with(sample());
    p.click(Target::Switch(1));
    p.finished(&Command::TurnOn("codex".into()), Reply::NeedsTool);
    scenes.push(("needs-a-tool", p));

    // The keyring would not take it.
    let mut p = with(sample());
    p.click(Target::Keys(2));
    p.text('x');
    if let alpymist_ai_usage::popup::Outcome::Run(keep) = p.click(Target::Save) {
        p.finished(
            &keep,
            Reply::Failed(
                "The keyring is locked. It opens when you log in with your password.".into(),
            ),
        );
    }
    scenes.push(("refused", p));

    let mut off = sample();
    for provider in &mut off.providers {
        provider.on = false;
        provider.lines.clear();
        provider.notes.clear();
        provider.plan = None;
    }
    let mut p = with(off);
    p.key(Key::Tab);
    scenes.push(("all-off-focus", p));

    scenes.push(("nothing", with(Reading::default())));

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

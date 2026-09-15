//! Render the dialog offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-auth --example snapshot -- OUTDIR [SCALE]`
//!
//! Point `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies
//! of Fira Sans and Symbols Nerd Font to preview with the real faces.

#![allow(clippy::many_single_char_names)]

use alpymist_auth::helper::Step;
use alpymist_auth::prompt::{Focus, Prompt, Target};
use alpymist_auth::request::{Identity, Request};
use alpymist_auth::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::{Appearance, Key};
use denise::{BufferAge, Frame, PixelFormat};

fn request(identities: usize) -> Request {
    let mut details = std::collections::BTreeMap::new();
    details.insert(
        "command_line".into(),
        "/usr/libexec/alpymist-store-helper add gimp".into(),
    );
    Request {
        action: "org.alpymist.store.install".into(),
        message:
            "Authentication is needed to install Alpine packages for everyone on this computer"
                .into(),
        cookie: String::new(),
        details,
        identities: [
            ("André Biseth", "andre", 1000),
            ("Kari Nordmann", "kari", 1001),
        ]
        .iter()
        .take(identities)
        .map(|(full, name, uid)| Identity {
            uid: *uid,
            name: (*name).into(),
            full_name: (*full).into(),
        })
        .collect(),
    }
}

fn asked(p: &mut Prompt) {
    p.step(Step::Prompt {
        text: "Password: ".into(),
        echo: false,
    });
}

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

    let mut scenes: Vec<(&str, Prompt)> = Vec::new();

    let mut p = Prompt::new(request(1), 0);
    asked(&mut p);
    for ch in "hunter2".chars() {
        p.text(ch);
    }
    p.hover = Some(Target::Authenticate);
    scenes.push(("typing", p));

    let mut p = Prompt::new(request(2), 0);
    asked(&mut p);
    p.text('x');
    let _ = p.key(Key::Enter);
    let _ = p.step(Step::Done(false));
    asked(&mut p);
    p.caps_lock = true;
    scenes.push(("wrong", p));

    let mut p = Prompt::new(request(1), 0);
    asked(&mut p);
    p.text('x');
    let _ = p.key(Key::Enter);
    p.frame = 6;
    scenes.push(("checking", p));

    let mut p = Prompt::new(request(1), 0);
    asked(&mut p);
    p.focus = Focus::Cancel;
    scenes.push(("focus-cancel", p));

    let mut p = Prompt::new(request(1), 0);
    asked(&mut p);
    p.checkable = true;
    scenes.push(("checkable", p));

    let mut p = Prompt::new(request(1), 0);
    asked(&mut p);
    p.checkable = true;
    let _ = p.verify();
    for ch in "hunter".chars() {
        p.text(ch);
    }
    scenes.push(("verified", p));

    for (name, prompt) in &scenes {
        let layout = Layout::new(&appearance, &mut fonts, prompt, scale);
        let size = layout.size;
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
            view::paint(&mut frame, &layout, &appearance, &mut fonts, prompt);
        }
        let file = std::fs::File::create(format!("{dir}/{name}.png")).expect("png");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        let data: Vec<u8> = pixels
            .iter()
            .flat_map(|p| {
                let [a, r, g, b] = p.to_be_bytes();
                [r, g, b, a]
            })
            .collect();
        writer.write_image_data(&data).expect("data");
    }
}

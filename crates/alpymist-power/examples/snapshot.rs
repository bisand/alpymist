//! Render the popup offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-power --example snapshot -- outdir [scale]`
//!
//! Drives the real [`Popup`] and [`view::paint`] over sample readings. Point
//! `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of Fira
//! Sans and Symbols Nerd Font to preview with the real faces.

#![allow(clippy::many_single_char_names)]

use alpymist_power::battery::{Power, Status, sample};
use alpymist_power::config::Config;
use alpymist_power::popup::{Command, Popup, Reading, Show, Target};
use alpymist_power::profile::Profile;
use alpymist_power::view::{self, Fonts, Layout};
use alpymist_widget::{Appearance, Key};
use denise::{BufferAge, Frame, PixelFormat};

fn reading(power: Power) -> Reading {
    Reading {
        power,
        profile: Some(Profile::Balanced),
        profiles: Profile::ALL.to_vec(),
        can_hibernate: false,
    }
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
    for p in &fonts.problems {
        eprintln!("font: {p}");
    }

    let with = |r: Reading| {
        let mut p = Popup::new(Config::default());
        p.update(r);
        p
    };
    let mut scenes: Vec<(&str, Popup)> = Vec::new();

    let mut p = with(reading(sample()));
    p.hover_over(Some(Target::Profile(Profile::Performance)));
    scenes.push(("discharging", p));

    let mut charging = sample();
    charging.plugged = Some(true);
    charging.batteries[0].status = Status::Charging;
    charging.batteries[0].power_w = Some(24.8);
    let mut p = with(reading(charging));
    p.click(Target::Show(Show::Time));
    scenes.push(("charging", p));

    let mut low = sample();
    low.batteries[0].energy_wh = Some(4.1);
    low.batteries[0].charge_limit = None;
    low.batteries[0].cycles = None;
    let mut p = with(reading(low));
    p.key(Key::Tab);
    p.key(Key::Tab);
    p.key(Key::Right);
    scenes.push(("low-focus-lid", p));

    let mut p = with(reading(sample()));
    if let alpymist_power::popup::Outcome::Run(c) = p.click(Target::Profile(Profile::PowerSaver)) {
        p.finished(
            &c,
            Err("Not allowed: this account is not an administrator.".into()),
        );
    }
    let _ = Command::SetChargeLimit(80);
    scenes.push(("refused", p));

    let mut p = with(Reading {
        power: Power::default(),
        profile: Some(Profile::Performance),
        profiles: vec![Profile::PowerSaver, Profile::Performance],
        can_hibernate: false,
    });
    p.key(Key::Tab);
    scenes.push(("desktop", p));

    let mut p = with(Reading {
        power: Power::default(),
        ..Reading::default()
    });
    p.key(Key::Escape);
    scenes.push(("nothing", p));

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

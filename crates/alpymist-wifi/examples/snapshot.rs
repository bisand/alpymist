//! Render the popup offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-wifi --example snapshot -- outdir [scale]`
//!
//! Drives the real [`Popup`] and [`view::paint`] over sample networks. Point
//! `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of Fira
//! Sans and Symbols Nerd Font to preview with the real faces; a
//! `FiraSans-SemiBold.ttf` beside the first is picked up too.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use alpymist_menu::config::Appearance;
use alpymist_wifi::model::{Radio, State, Station, sample};
use alpymist_wifi::popup::{Busy, Command, Key, Popup, Target};
use alpymist_wifi::view::{self, Fonts, Layout};
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

    let fresh = || {
        let mut p = Popup::new(view::ROWS);
        p.update(sample());
        p
    };
    let mut scenes: Vec<(&str, Popup)> = Vec::new();

    let mut p = fresh();
    p.hover_over(Some(Target::Row(0)));
    scenes.push(("connected", p));

    let mut p = fresh();
    p.key(Key::Down);
    p.key(Key::Down);
    p.key(Key::Enter);
    "hemmelig1".chars().for_each(|c| {
        p.text(c);
    });
    scenes.push(("passphrase", p));

    let mut p = fresh();
    p.key(Key::Down);
    p.key(Key::Down);
    p.key(Key::Enter);
    "feil passord".chars().for_each(|c| {
        p.text(c);
    });
    if let alpymist_wifi::popup::Outcome::Run(command) = p.key(Key::Enter) {
        p.finished(
            &command,
            Err("Could not join — check the passphrase.".into()),
        );
    }
    scenes.push(("wrong-passphrase", p));

    let mut p = Popup::new(view::ROWS);
    let mut joining = sample();
    joining.station = Station::Connecting;
    joining.link = None;
    joining.current = Some("Kaffebar Gjest".into());
    for n in &mut joining.networks {
        n.connected = false;
    }
    p.update(joining);
    let _ = p.finished(&Command::Scan, Ok(()));
    scenes.push(("joining", p));

    let mut p = Popup::new(view::ROWS);
    let mut scanning = sample();
    scanning.scanning = true;
    scanning.station = Station::Disconnected;
    scanning.link = None;
    scanning.current = None;
    scanning.networks.clear();
    p.update(scanning);
    scenes.push(("scanning", p));

    let mut p = Popup::new(view::ROWS);
    p.update(State {
        radio: Radio::Off,
        device: Some("wlan0".into()),
        ..State::default()
    });
    scenes.push(("off", p));

    let mut p = Popup::new(view::ROWS);
    p.update(State {
        radio: Radio::NoAdapter,
        ..State::default()
    });
    scenes.push(("no-adapter", p));

    let mut p = fresh();
    p.key(Key::Tab);
    p.key(Key::Tab);
    p.key(Key::Tab);
    scenes.push(("focus-disconnect", p));

    let mut p = fresh();
    p.key(Key::Down);
    p.key(Key::Down);
    scenes.push(("focus-list", p));

    let mut p = fresh();
    p.key(Key::Down);
    p.key(Key::Down);
    p.key(Key::Enter);
    "hemmelig".chars().for_each(|c| {
        p.text(c);
    });
    p.key(Key::Tab);
    p.key(Key::Tab);
    scenes.push(("focus-join", p));

    let mut p = fresh();
    p.click(Target::Switch);
    assert!(matches!(p.busy(), Some(Busy::Switching(false))));
    scenes.push(("switching-off", p));

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
        let started = std::time::Instant::now();
        view::paint(&mut frame, &layout, &appearance, &mut fonts, &popup);
        eprintln!("{name}: {w}x{h} painted in {:?}", started.elapsed());

        // Over a mid-grey, so the translucent background and the corners
        // read the way they will over a wallpaper.
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

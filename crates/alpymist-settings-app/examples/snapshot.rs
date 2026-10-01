//! Paint Settings offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-settings-app --example snapshot -- outdir [scale]`
//!
//! Drives the real [`View`] with sample values. Point `ALPYMIST_FONTS` at a
//! directory holding FiraSans-Regular.ttf, FiraSans-SemiBold.ttf and
//! SymbolsNerdFontMono-Regular.ttf to paint with the real faces.

use alpymist_about::info::About;
use alpymist_displays::screen::Monitor;
use alpymist_settings::{Settings, Value};
use alpymist_settings_app::view::{Fonts, Screens, View};
use alpymist_theme::{Scheme, ThemeFile};
use denise::{BufferAge, Frame, InputEvent, PixelFormat, Size};
use denise_text::{GlyphSource, TrueTypeSource};
use std::collections::BTreeMap;

fn fonts() -> Fonts {
    let dir = std::env::var("ALPYMIST_FONTS").unwrap_or_default();
    let load = |name: &str| -> Option<Box<dyn GlyphSource>> {
        let bytes = std::fs::read(format!("{dir}/{name}")).ok()?;
        TrueTypeSource::from_bytes(name, &bytes)
            .ok()
            .map(|f| Box::new(f) as Box<dyn GlyphSource>)
    };
    Fonts {
        text: load("FiraSans-Regular.ttf"),
        strong: load("FiraSans-SemiBold.ttf"),
        icons: load("SymbolsNerdFontMono-Regular.ttf"),
    }
}

fn values(settings: &Settings) -> BTreeMap<&'static str, Result<Value, String>> {
    let mut v: BTreeMap<_, _> = settings
        .all()
        .iter()
        .map(|s| (s.id, Ok(s.default.clone())))
        .collect();
    v.insert("keyboard.layout", Ok(Value::Text("no".into())));
    v.insert("touchpad.natural-scroll", Ok(Value::Bool(true)));
    v.insert("mouse.speed", Ok(Value::Number(-20)));
    v.insert(
        "power.charge-limit",
        Err("no battery here takes a charge limit".into()),
    );
    v.insert("updates.channel", Ok(Value::Text("dev".into())));
    v
}

/// A laptop on a dock with two screens, as the dev machine is.
fn desk() -> Vec<Monitor> {
    let screen = |name: &str, description: &str, x: i32, width: i32, height: i32| Monitor {
        name: name.into(),
        description: description.into(),
        model: description
            .split_whitespace()
            .nth(3)
            .unwrap_or_default()
            .into(),
        width,
        height,
        refresh_rate: 60.0,
        x,
        scale: 1.0,
        available_modes: vec![
            format!("{width}x{height}@60.00Hz"),
            "1680x1050@59.95Hz".into(),
            "1280x720@60.00Hz".into(),
        ],
        ..Monitor::default()
    };
    vec![
        screen("eDP-1", "Chimei Innolux Corporation 0x14C9", 0, 1920, 1080),
        screen(
            "DP-3",
            "Samsung Electric Company S24E650 H4ZK500123",
            1920,
            1920,
            1080,
        ),
        screen(
            "DP-5",
            "Samsung Electric Company S24C750 H4ZM400456",
            3840,
            1920,
            1080,
        ),
    ]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("create output directory");
    let size = Size::new(920 * scale, 660 * scale);

    // A screensaver's own area opens the Screensaver page with that
    // screensaver's dialog over it, which is the only place it is drawn — so
    // point XDG_DATA_HOME at a directory of definition files to see one.
    let scenes: [(&str, Scheme, &str, &str); 13] = [
        ("displays", Scheme::Dark, "displays", ""),
        // The providers are whatever ALPYMIST_AI_USAGE_DIR holds, or what is
        // installed: point it at desktop/ai-usage to see the shipped four.
        ("ai", Scheme::Dark, "ai", ""),
        ("touchpad", Scheme::Dark, "touchpad", ""),
        ("keyboard", Scheme::Dark, "keyboard.layout", ""),
        ("search", Scheme::Dark, "", "scroll"),
        ("power", Scheme::Dark, "power", ""),
        ("screensaver", Scheme::Dark, "screensaver", ""),
        (
            "screensaver-dialog",
            Scheme::Dark,
            "screensaver-mountains",
            "",
        ),
        ("system", Scheme::Dark, "system", ""),
        ("startup", Scheme::Dark, "startup", ""),
        ("default", Scheme::Dark, "default", ""),
        ("about", Scheme::Dark, "about", ""),
        ("appearance-light", Scheme::Light, "appearance", ""),
    ];
    for (name, scheme, open, typed) in scenes {
        let theme = ThemeFile {
            scheme,
            ..ThemeFile::default()
        };
        let settings = Settings::new();
        let values = values(&settings);
        let mut view = View::new(
            size,
            scale,
            theme.denise(),
            fonts(),
            settings,
            values,
            About::sample(),
        );
        view.screens(Screens::new(desk(), None));
        if !open.is_empty() {
            view.open(open);
        }
        let events: Vec<InputEvent> = typed.chars().map(|ch| InputEvent::Text { ch }).collect();
        let _ = view.handle(&events, 1000);

        let (w, h) = (size.width, size.height);
        let mut pixels = vec![0u32; (w * h) as usize];
        let mut frame = Frame::new(
            &mut pixels,
            size,
            w,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        )
        .unwrap();
        view.paint(&mut frame);
        drop(frame);
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

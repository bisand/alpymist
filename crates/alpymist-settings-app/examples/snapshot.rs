//! Paint Settings offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-settings-app --example snapshot -- outdir [scale]`
//!
//! Drives the real [`View`] with sample values. Point `ALPYMIST_FONTS` at a
//! directory holding FiraSans-Regular.ttf, FiraSans-SemiBold.ttf and
//! SymbolsNerdFontMono-Regular.ttf to paint with the real faces.

use alpymist_about::info::About;
use alpymist_settings::{Settings, Value};
use alpymist_settings_app::view::{Fonts, View};
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

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("create output directory");
    let size = Size::new(920 * scale, 660 * scale);

    // A screensaver's own area opens the Screensaver page with that
    // screensaver's dialog over it, which is the only place it is drawn — so
    // point XDG_DATA_HOME at a directory of definition files to see one.
    let scenes: [(&str, Scheme, &str, &str); 8] = [
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

//! Render the store offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-store --example snapshot -- CONFIG OUTDIR [SCALE]`
//!
//! Loads the sources CONFIG names — point them at copies of a system's
//! `AppStream` and apk data with `appstream` and `root` — and drives the real
//! [`Store`] and [`view::paint`] through a few scenes. Point
//! `ALPYMIST_MENU_FONT` and `ALPYMIST_MENU_ICON_FONT` at local copies of Fira
//! Sans and Symbols Nerd Font to preview with the real faces.

use alpymist_store::catalog::Installed;
use alpymist_store::config;
use alpymist_store::icons::Icons;
use alpymist_store::pictures::Pictures;
use alpymist_store::source::{self, Op};
use alpymist_store::store::{Action, Event, Key, SourceInfo, Store, Target, View};
use alpymist_store::view::{self, Layout};
use alpymist_widget::Appearance;
use alpymist_widget::draw::Fonts;
use denise::geom::Size;
use denise::{BufferAge, Frame, PixelFormat};

#[allow(clippy::too_many_lines)] // one scene after another
fn main() {
    let mut args = std::env::args().skip(1);
    let config_path = args.next().expect("a configuration");
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("create output directory");

    let loaded = config::load_from([std::path::PathBuf::from(config_path)]);
    for p in &loaded.problems {
        eprintln!("config: {p}");
    }
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

    let infos: Vec<SourceInfo> = loaded
        .config
        .sources
        .iter()
        .map(|s| {
            let flatpak = matches!(s.kind, config::Kind::Flatpak(_));
            SourceInfo {
                id: s.id.clone(),
                label: s.label.clone(),
                icon: s.icon.clone(),
                colour: s.colour,
                launches: flatpak,
                apps: flatpak,
            }
        })
        .collect();
    let fresh = || {
        let mut store = Store::new(infos.clone());
        for (i, s) in loaded.config.sources.iter().enumerate() {
            let source = u16::try_from(i).unwrap();
            store.event(Event::Loaded {
                source,
                result: source::open(s).load(),
            });
        }
        store
    };

    let mut scenes: Vec<(&str, Store)> = Vec::new();

    let mut s = fresh();
    s.hover = Some(Target::Nav(View::Installed));
    scenes.push(("discover", s));

    let mut s = fresh();
    for ch in "firefox".chars() {
        s.text(ch);
    }
    s.key(Key::Down);
    scenes.push(("search", s));

    let mut s = fresh();
    for ch in "video editor".chars() {
        s.text(ch);
    }
    let first = s.results[0];
    s.act(first, Action::Install);
    s.event(Event::Progress {
        source: first.source,
        line: "Installing runtime/org.kde.Platform/aarch64/6.9".into(),
    });
    s.frame = 5;
    scenes.push(("installing", s));

    let mut s = fresh();
    if let Some(gimp) = s.catalog.find(0, "org.gimp.GIMP") {
        s.event(Event::Installed {
            source: 0,
            result: Ok(vec![Installed {
                id: "org.gimp.GIMP".into(),
                version: "3.2.4".into(),
                explicit: true,
                update: Some("3.2.6".into()),
            }]),
        });
        s.open(gimp);
    }
    scenes.push(("detail", s));

    let mut s = fresh();
    for ch in "zsh".chars() {
        s.text(ch);
    }
    s.key(Key::Source(2));
    if let Some(&at) = s.results.first() {
        s.open(at);
    }
    scenes.push(("package", s));

    let mut s = fresh();
    s.show(View::Updates);
    scenes.push(("updates-empty", s));

    let mut s = Store::new(infos.clone());
    s.event(Event::Status {
        source: 0,
        text: "Fetching the catalogue".into(),
    });
    scenes.push(("loading", s));

    let mut s = fresh();
    for ch in "gimp".chars() {
        s.text(ch);
    }
    s.event(Event::Finished {
        source: 1,
        op: Op::Install("gimp".into()),
        result: Err(
            "Only an administrator can change system packages, and this account is not one".into(),
        ),
    });
    scenes.push(("error", s));

    for (width, height, suffix) in [(1080, 720, ""), (700, 560, "-narrow")] {
        for (name, store) in &mut scenes {
            let size = Size::new(width * scale, height * scale);
            let layout = Layout::new(&appearance, &mut fonts, store, size, scale);
            store.set_page(layout.rows);
            let layout = Layout::new(&appearance, &mut fonts, store, size, scale);
            let mut icons = Icons::default();
            let mut pictures = Pictures::default();
            // A screenshot from a local file, where one is given.
            if let (Some(at), Ok(file)) = (store.detail, std::env::var("ALPYMIST_STORE_SCREENSHOT"))
                && let Some(shot) = store
                    .catalog
                    .get(at)
                    .and_then(|e| e.screenshots.get(store.shot))
                && let Ok(f) = std::fs::File::open(&file)
            {
                pictures.request(&shot.url);
                let decoded = alpymist_store::icons::decode(std::io::BufReader::new(f), 1 << 24);
                pictures.arrived(
                    shot.url.clone(),
                    decoded.ok_or_else(|| "not a PNG".to_owned()),
                );
            }
            let mut pixels = vec![0u32; (size.width * size.height) as usize];
            let started = std::time::Instant::now();
            {
                let mut frame = Frame::new(
                    &mut pixels,
                    size,
                    size.width,
                    PixelFormat::Argb8888,
                    BufferAge::Undefined,
                )
                .expect("frame");
                view::paint(
                    &mut frame,
                    &layout,
                    &appearance,
                    &mut fonts,
                    &mut icons,
                    &mut pictures,
                    store,
                );
            }
            eprintln!(
                "{name}{suffix}: painted in {} µs",
                started.elapsed().as_micros()
            );
            write_png(&format!("{dir}/{name}{suffix}.png"), &pixels, size);
        }
    }
}

fn write_png(path: &str, pixels: &[u32], size: Size) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.width, size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    let mut data = Vec::with_capacity(pixels.len() * 4);
    for p in pixels {
        let [a, r, g, b] = p.to_be_bytes();
        data.extend([r, g, b, a]);
    }
    writer.write_image_data(&data).expect("png data");
}

//! Render the menu offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-menu --example snapshot -- outdir [scale]`
//!
//! Drives the real [`Menu`] and [`view::paint`], so a snapshot shows exactly
//! what the layer surface would. Point `ALPYMIST_MENU_FONT` and
//! `ALPYMIST_MENU_ICON_FONT` at local copies of Fira Sans and Symbols Nerd
//! Font to preview with the real faces.

use alpymist_menu::apps::App;
use alpymist_menu::config::Config;
use alpymist_menu::exec::Launch;
use alpymist_menu::history::History;
use alpymist_menu::menu::{Key, Menu};
use alpymist_menu::tree::Tree;
use alpymist_menu::view::{self, Fonts, Layout};
use denise::{BufferAge, Frame, PixelFormat};

fn app(name: &str, detail: &str) -> App {
    App {
        id: format!("{}.desktop", name.to_lowercase()),
        name: name.into(),
        detail: (!detail.is_empty()).then(|| detail.into()),
        keywords: vec![],
        icon: "󰣆",
        launch: Launch::Argv {
            argv: vec![name.to_lowercase()],
            terminal: false,
        },
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&dir).expect("create output directory");

    let mut config = Config::parse(alpymist_menu::config::DEFAULT).expect("default config");
    if let Ok(font) = std::env::var("ALPYMIST_MENU_FONT") {
        config.appearance.font = font;
    }
    if let Ok(font) = std::env::var("ALPYMIST_MENU_ICON_FONT") {
        config.appearance.icon_font = font;
    }
    let apps = vec![
        app("Btop++", "System Monitor"),
        app("Foot", "Terminal"),
        app("Foot Server", "Terminal"),
        app("Ghostty", "Terminal"),
        app("LibreWolf", "Web Browser"),
        app("Pavucontrol", "Volume Control"),
    ];
    let mut fonts = Fonts::load(&config.appearance);
    for p in &fonts.problems {
        eprintln!("font: {p}");
    }
    let layout = Layout::new(&config.appearance, scale);

    let scenes: Vec<(&str, Vec<Step>, Option<&str>)> = vec![
        ("root", vec![], None),
        ("search", vec![Step::Type("shut")], None),
        (
            "system",
            vec![Step::Key(Key::End), Step::Key(Key::Enter)],
            None,
        ),
        (
            "apps",
            vec![Step::Key(Key::Enter), Step::Key(Key::Down)],
            None,
        ),
        (
            "config",
            vec![Step::Type("conf"), Step::Key(Key::Enter)],
            None,
        ),
        ("nothing", vec![Step::Type("qqqq")], None),
        (
            "notice",
            vec![],
            Some("~/.config/alpymist/menu.toml: TOML parse error at line 3, column 1"),
        ),
    ];

    for (name, steps, notice) in scenes {
        let tree = Tree::build(&config, apps.clone());
        let root = tree.find("root").unwrap();
        let mut menu = Menu::new(tree, History::default(), root, layout.rows as usize);
        for step in steps {
            match step {
                Step::Key(k) => {
                    menu.key(k);
                }
                Step::Type(t) => t.chars().for_each(|c| {
                    menu.text(c);
                }),
            }
        }

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
        view::paint(
            &mut frame,
            &layout,
            &config.appearance,
            &mut fonts,
            &menu,
            notice,
        );
        eprintln!("{name}: painted in {:?}", started.elapsed());

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

enum Step {
    Key(Key),
    Type(&'static str),
}

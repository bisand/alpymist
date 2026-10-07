//! Render the table offscreen to PNGs, for review without a Wayland session.
//!
//! `cargo run -p alpymist-solitaire --example snapshot -- outdir [width height [scale]]`
//!
//! Point `ALPYMIST_MENU_FONT` at a local copy of Fira Sans to preview with
//! the real face.

use alpymist_solitaire::cards::{Card, Suit};
use alpymist_solitaire::faces::Faces;
use alpymist_solitaire::game::{Game, PILES, Pile, Place};
use alpymist_solitaire::view::{self, Layout, Lifted, Scene};
use alpymist_widget::Appearance;
use alpymist_widget::draw::Fonts;
use denise::geom::{Point, Size};
use denise::{BufferAge, Frame, PixelFormat};

/// A game some way in: runs on the piles, cards home, three on the waste.
fn midway() -> Game {
    let card = |suit, rank| Card { suit, rank };
    let run = |from: u8, len: u8, black_first: bool| -> Vec<Card> {
        (0..len)
            .map(|i| {
                let black = black_first == (i % 2 == 0);
                let suit = match (black, i % 4 < 2) {
                    (true, true) => Suit::Spades,
                    (true, false) => Suit::Clubs,
                    (false, true) => Suit::Hearts,
                    (false, false) => Suit::Diamonds,
                };
                card(suit, from - i)
            })
            .collect()
    };
    let hidden = |n: u8| (0..n).map(|i| card(Suit::Clubs, 1 + i)).collect();
    let mut piles: [Pile; PILES] = Default::default();
    piles[0] = Pile {
        down: vec![],
        up: run(13, 6, true),
    };
    piles[1] = Pile {
        down: hidden(1),
        up: run(9, 3, false),
    };
    piles[3] = Pile {
        down: hidden(2),
        up: run(12, 8, false),
    };
    piles[4] = Pile {
        down: hidden(3),
        up: run(6, 2, true),
    };
    piles[5] = Pile {
        down: hidden(4),
        up: run(13, 1, false),
    };
    piles[6] = Pile {
        down: hidden(5),
        up: run(10, 4, true),
    };
    let waste = vec![
        card(Suit::Diamonds, 9),
        card(Suit::Spades, 4),
        card(Suit::Hearts, 12),
    ];
    let stock = (0..9).map(|i| card(Suit::Spades, 1 + i)).collect();
    Game::set_out(piles, stock, waste, [3, 2, 4, 1], 3)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let mut number = |or: u32| args.next().and_then(|s| s.parse().ok()).unwrap_or(or);
    let (w, h, scale) = (number(1100), number(760), number(1));
    std::fs::create_dir_all(&dir).expect("create output directory");

    let mut appearance = Appearance::default();
    if let Ok(font) = std::env::var("ALPYMIST_MENU_FONT") {
        appearance.font = font;
    }
    let mut fonts = Fonts::load(&appearance);
    for p in &fonts.problems {
        eprintln!("font: {p}");
    }
    let mut faces = Faces::default();
    let size = Size::new(w, h);
    let layout = Layout::new(&appearance, size, scale);

    let dealt = Game::new(2026, 1);
    let midway = midway();
    let from = layout.top(&midway, Place::Tableau(3), 3);
    let held = Scene {
        lifted: Some(Lifted {
            place: Place::Tableau(3),
            count: 3,
            at: Point::new(from.x - layout.card.0 * 3 / 4, from.y + layout.card.1 / 5),
        }),
        ..Scene::default()
    };
    let keys = Scene {
        focus: Some(Place::Tableau(1)),
        chosen: Some((Place::Tableau(6), 2)),
        ..Scene::default()
    };

    for (name, game, scene) in [
        ("dealt", &dealt, Scene::default()),
        ("midway", &midway, Scene::default()),
        ("held", &midway, held),
        ("keys", &midway, keys),
    ] {
        let mut pixels = vec![0u32; (w * h) as usize];
        let mut frame = Frame::new(
            &mut pixels,
            size,
            w,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        )
        .unwrap();
        view::paint(
            &mut frame,
            &layout,
            &appearance,
            &mut fonts,
            &mut faces,
            game,
            &scene,
        );

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

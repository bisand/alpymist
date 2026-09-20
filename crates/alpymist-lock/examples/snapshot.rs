//! Render the lock screen offscreen to PNGs.
//!
//! `cargo run -p alpymist-lock --example snapshot -- outdir [width height]`
//!
//! The same screen the compositor would show, drawn by the same code, with a
//! checker that stands in for PAM. Set `ALPYMIST_FONT` to a Fira Mono TTF to
//! see the real typeface.

use alpymist_greeter::app::{Action, App, Authenticator, Purpose};
use alpymist_greeter::login::Outcome;
use alpymist_lock::who::User;
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn screen(width: u32, height: u32, delay: Duration) -> App {
    let check: Authenticator = Arc::new(move |_: &str, _: &str| {
        std::thread::sleep(delay);
        (
            Outcome::Rejected("That password is not right.".into()),
            Vec::new(),
        )
    });
    // One account, because a lock screen has one: this session's.
    let user = User {
        name: "andre".into(),
        display: "André Biseth".into(),
        uid: 1000,
    };
    let mut app = App::new(vec![user], check, width, height);
    app.purpose = Purpose::Unlock;
    app.hostname = "alpymist".into();
    app.set_time("23:11".into(), "Sunday 20 September".into());
    app
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(1366);
    let height: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(768);
    std::fs::create_dir_all(&dir).expect("create output directory");

    let mut typing = screen(width, height, Duration::ZERO);
    eprintln!("{}", typing.face.status.describe());
    for ch in "hunter".chars() {
        typing.act(Action::Type(ch));
    }
    save(&mut typing, &format!("{dir}/1-typing.png"), width, height);

    let mut checking = screen(width, height, Duration::from_mins(1));
    checking.act(Action::Type('x'));
    checking.act(Action::Submit);
    save(
        &mut checking,
        &format!("{dir}/2-checking.png"),
        width,
        height,
    );

    let mut wrong = screen(width, height, Duration::ZERO);
    wrong.act(Action::Submit);
    let until = Instant::now() + Duration::from_secs(5);
    while wrong.checking() && Instant::now() < until {
        wrong.tick();
    }
    save(&mut wrong, &format!("{dir}/3-wrong.png"), width, height);
    std::process::exit(0);
}

fn save(app: &mut App, path: &str, width: u32, height: u32) {
    let mut pixels = vec![0u32; (width as usize) * (height as usize)];
    {
        let mut canvas = Canvas::from_pixels(
            &mut pixels,
            Size::new(width, height),
            width,
            PixelFormat::Argb8888,
        )
        .expect("buffer large enough for the requested size");
        app.draw(&mut canvas);
    }
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for px in &pixels {
        rgba.extend_from_slice(&[
            u8::try_from((px >> 16) & 0xFF).unwrap_or(0),
            u8::try_from((px >> 8) & 0xFF).unwrap_or(0),
            u8::try_from(px & 0xFF).unwrap_or(0),
            0xFF,
        ]);
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&rgba)
        .expect("png data");
    println!("wrote {path}");
}

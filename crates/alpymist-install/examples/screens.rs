//! Render every wizard screen offscreen to PNGs.
//!
//! `cargo run -p alpymist-install --example screens -- outdir [width height]`
//!
//! The installer cannot be reviewed by running it — it wants a whole machine —
//! so this draws each screen to a file instead. It drives the real [`App`]
//! rather than re-implementing its drawing, so a screenshot cannot show
//! something the installer would never render.
//!
//! Set `ALPYMIST_FONT` to a Fira Mono TTF to preview with the real typeface;
//! without it the built-in bitmap is used, which is also what a machine missing
//! `font-fira-ttf` would show.

use alpymist_core::Tier;
use alpymist_install::answers::{Answers, DiskPlan, Network};
use alpymist_install::app::{Action, App};
use alpymist_install::wizard::Step;
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;

/// Plausible answers, so later screens have something to show.
fn answers() -> Answers {
    Answers {
        keyboard: Some("no".into()),
        keyboard_variant: Some("nodeadkeys".into()),
        timezone: Some("Europe/Oslo".into()),
        network: Some(Network::Dhcp),
        disk: Some(DiskPlan::WholeDisk {
            device: "/dev/sda".into(),
            encrypt: true,
        }),
        disk_confirmed: true,
        username: "andre".into(),
        full_name: "André Biseth".into(),
        password: "a good long passphrase".into(),
        password_confirm: "a good long passphrase".into(),
        hostname: "alpymist".into(),
        disks: alpymist_install::disks::sample(),
        detected_tier: Some(Tier::Lite),
        tier_override: None,
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(1280);
    let height: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(800);
    std::fs::create_dir_all(&dir).expect("create output directory");

    for (index, step) in Step::ALL.into_iter().enumerate() {
        let mut app = App::new(answers(), width, height);
        while app.wizard.step() != step && app.wizard.step() != Step::Done {
            app.act(Action::Advance);
        }
        if index == 0 {
            eprintln!("{}", app.face.status.describe());
        }

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

        let name = format!("{dir}/{index:02}-{step:?}.png").to_lowercase();
        write_png(&name, &pixels, width, height);
        println!("wrote {name}");
    }
}

fn write_png(path: &str, pixels: &[u32], width: u32, height: u32) {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for px in pixels {
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
}

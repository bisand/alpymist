//! Render every wizard screen offscreen to PNGs.
//!
//! `cargo run -p alpymist-install --example screens -- outdir [width height]`
//!
//! The installer cannot be reviewed by running it — it wants a whole machine —
//! so this draws each screen to a file instead. It is also what a CI visual
//! check would diff.

use alpymist_core::Tier;
use alpymist_install::answers::{Answers, DiskPlan, Network};
use alpymist_install::wizard::{Step, Wizard};
use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::chrome::Chrome;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{colour, paint_backdrop, paint_panel};
use denise::PixelFormat;
use denise::geom::{Point, Size};
use denise_render::Canvas;
use denise_render::font::BUILT_IN;

/// Same seed as the splash, so the mountains do not change at the handover.
const SEED: u64 = 0x_A1B2_C3D4_E5F6;

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
        detected_tier: Some(Tier::Lite),
        tier_override: None,
    }
}

/// The body lines each screen shows. Placeholder content standing in for the
/// real widgets, so the layout and density can be judged before they exist.
fn body_lines(step: Step, a: &Answers) -> Vec<(String, bool)> {
    let sel = |s: &str, on: bool| (s.to_string(), on);
    match step {
        Step::Welcome => vec![
            sel("Alpymist will be installed on this machine.", false),
            sel("", false),
            sel("Nothing is written to any disk until you confirm.", false),
        ],
        Step::Keyboard => vec![
            sel("Norwegian                     no", true),
            sel("Norwegian (no dead keys)      no  nodeadkeys", false),
            sel("English (UK)                  gb", false),
            sel("English (US)                  us", false),
            sel("German                        de", false),
        ],
        Step::Region => vec![
            sel("Europe / Oslo            CEST  UTC+2", true),
            sel("Europe / Stockholm       CEST  UTC+2", false),
            sel("Europe / London          BST   UTC+1", false),
            sel("UTC                            UTC+0", false),
        ],
        Step::Network => vec![
            sel("Automatic (DHCP)", true),
            sel("Static address", false),
            sel("Set up later", false),
            sel("", false),
            sel("eth0    link up    100 Mbit/s", false),
        ],
        Step::Disk => vec![
            sel("/dev/sda    238.5 GB   Samsung SSD 860", true),
            sel("/dev/sdb      3.6 TB   WDC WD40EFRX", false),
            sel("", false),
            sel("[x] Erase the whole disk and set it up for me", false),
            sel("[x] Encrypt the root filesystem (LUKS2)", false),
        ],
        Step::Account => vec![
            sel(&format!("Full name   {}", a.full_name), false),
            sel(&format!("Username    {}", a.username), true),
            sel("Password    ********************", false),
            sel("Confirm     ********************", false),
            sel(&format!("Hostname    {}", a.hostname), false),
        ],
        Step::Desktop => vec![
            sel("This machine reports:  Lite", true),
            sel("  labwc, GPU accelerated", false),
            sel("", false),
            sel("  accelerated DRM driver i915 on card0", false),
            sel("  GL ES 3.0 is below the 3.2 Hyprland requires", false),
            sel("", false),
            sel("Use something else:   Full   Lite   Potato   Legacy", false),
        ],
        Step::Confirm => vec![
            sel("Keyboard    Norwegian (no)", false),
            sel("Time        Europe/Oslo", false),
            sel("Network     Automatic (DHCP)", false),
            sel("Disk        /dev/sda  erase and encrypt", true),
            sel("Account     andre  on  alpymist", false),
            sel("Desktop     Lite  (labwc)", false),
        ],
        Step::Install => vec![
            sel("Partitioning /dev/sda                    done", false),
            sel("Creating the encrypted volume            done", false),
            sel("Installing the base system            ......", true),
            sel("Installing the desktop", false),
            sel("Configuring the bootloader", false),
        ],
        Step::Done => vec![
            sel("Alpymist is installed.", false),
            sel("", false),
            sel("Remove the installation media and restart.", false),
        ],
    }
}

fn footer_for(step: Step, wizard: &Wizard) -> Vec<String> {
    let mut lines = Vec::new();
    for note in wizard.advisories() {
        lines.push(format!("! {}", note.message));
    }
    lines.push(match step {
        Step::Welcome => "Enter  begin        Esc  shut down".into(),
        Step::Install => "Please wait".into(),
        Step::Done => "Enter  restart".into(),
        _ => "Up/Down  choose     Enter  continue     Esc  back".into(),
    });
    lines
}

#[allow(clippy::too_many_lines)]
fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let width: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(1280);
    let height: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(800);
    std::fs::create_dir_all(&dir).expect("create output directory");

    let palette = Palette::alpymist();
    let backdrop = Backdrop::compose(width, height, &palette, SEED);
    let chrome = Chrome::for_screen(width, height);

    for (index, step) in Step::ALL.into_iter().enumerate() {
        let mut wizard = Wizard::new(answers());
        while wizard.step() != step && wizard.advance().is_ok() {}

        let mut pixels = vec![0u32; (width as usize) * (height as usize)];
        let mut canvas = Canvas::from_pixels(
            &mut pixels,
            Size::new(width, height),
            width,
            PixelFormat::Argb8888,
        )
        .expect("buffer large enough");

        paint_backdrop(&mut canvas, &backdrop);
        paint_panel(&mut canvas, &chrome, &palette);

        // Header: step counter, title, subtitle.
        if let Some(n) = step.question_number() {
            canvas.draw_text(
                &BUILT_IN,
                Point::new(chrome.counter_at.0, chrome.counter_at.1),
                chrome.text_scale,
                &format!("STEP {n} OF {}", Step::questions().count()),
                colour(palette.accent),
            );
        }
        canvas.draw_text(
            &BUILT_IN,
            Point::new(chrome.title_at.0, chrome.title_at.1),
            chrome.title_scale,
            step.title(),
            colour(palette.ink),
        );
        canvas.draw_text(
            &BUILT_IN,
            Point::new(chrome.subtitle_at.0, chrome.subtitle_at.1),
            chrome.text_scale,
            step.subtitle(),
            colour(palette.ink_dim),
        );

        // Body.
        for (row, (text, selected)) in body_lines(step, &wizard.answers).iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let y = chrome.body_row(i32::try_from(row).unwrap_or(0));
            let ink = if *selected {
                palette.accent
            } else {
                palette.ink
            };
            let prefix = if *selected { ">  " } else { "   " };
            canvas.draw_text(
                &BUILT_IN,
                Point::new(chrome.body.0, y),
                chrome.text_scale,
                &format!("{prefix}{text}"),
                colour(ink),
            );
        }

        // Footer.
        for (row, line) in footer_for(step, &wizard).iter().enumerate() {
            let y = chrome.footer.1 + i32::try_from(row).unwrap_or(0) * 12 * chrome.text_scale;
            canvas.draw_text(
                &BUILT_IN,
                Point::new(chrome.footer.0, y),
                chrome.text_scale,
                line,
                colour(if line.starts_with('!') {
                    palette.accent
                } else {
                    palette.ink_dim
                }),
            );
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

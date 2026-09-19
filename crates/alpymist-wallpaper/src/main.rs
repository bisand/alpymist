//! Render the Alpymist backdrop to a PNG.
//!
//! `alpymist-wallpaper OUT.png [WIDTH HEIGHT]`
//! `alpymist-wallpaper --mark OUT.png [SIZE]`
//! `alpymist-wallpaper --boot OUT.png [WIDTH HEIGHT]`
//!
//! Run while building the desktop package, so the desktop background is the
//! same misty mountains as the boot splash and the installer — drawn by the
//! same code with the same seed, rather than a picture that could drift from
//! them.

#![forbid(unsafe_code)]

use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::badge;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{paint_backdrop, paint_badge};
use denise::PixelFormat;
use denise::geom::Size;
use denise_render::Canvas;
use std::io::BufWriter;
use std::process::ExitCode;

/// The splash's and installer's seed, so the desktop shows the same range.
const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// Largest size accepted: a 16K panel, and far beyond any machine this targets.
const MAX_SIDE: u32 = 15_360;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(path) => {
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("alpymist-wallpaper: {why}");
            eprintln!("usage: alpymist-wallpaper OUT.png [WIDTH HEIGHT]");
            eprintln!("       alpymist-wallpaper --mark OUT.png [SIZE]");
            eprintln!("       alpymist-wallpaper --boot OUT.png [WIDTH HEIGHT]");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    if let Some(rest) = args.split_first().filter(|(f, _)| *f == "--mark") {
        let (out, size) = parse_mark(rest.1)?;
        write_mark(&out, size)?;
        return Ok(out);
    }
    if let Some(rest) = args.split_first().filter(|(f, _)| *f == "--boot") {
        let (out, width, height) = parse(rest.1)?;
        let pixels = render_boot(width, height)?;
        write_png(&out, &pixels, width, height)?;
        return Ok(out);
    }
    let (out, width, height) = parse(args)?;
    let pixels = render(width, height)?;
    write_png(&out, &pixels, width, height)?;
    Ok(out)
}

/// Arguments for the mark: an output path and an optional square size.
fn parse_mark(args: &[String]) -> Result<(String, u32), String> {
    match args {
        [out] => Ok((out.clone(), 64)),
        [out, size] => {
            let size = size
                .parse::<u32>()
                .ok()
                .filter(|v| (8..=1024).contains(v))
                .ok_or_else(|| format!("{size:?} is not an icon size between 8 and 1024"))?;
            Ok((out.clone(), size))
        }
        _ => Err("expected an output path and an optional size".into()),
    }
}

/// Write the Alpymist badge as a transparent PNG in the accent colour.
///
/// The badge rather than the bare mark: at the size a bar draws an icon the
/// ridgeline alone is a few dark pixels, and the disc is what gives it an
/// edge. Transparent rather than on a panel, so the bar's own background
/// shows through the peaks and the mark inherits whatever the bar is
/// coloured.
fn write_mark(path: &str, size: u32) -> Result<(), String> {
    let ink = Palette::alpymist().accent;
    let mask = badge::mask(size);
    // The badge is square.
    let height = size;
    let mut rgba = Vec::with_capacity(mask.len() * 4);
    for alpha in &mask {
        rgba.extend_from_slice(&[ink.r, ink.g, ink.b, *alpha]);
    }
    let file = std::fs::File::create(path).map_err(|e| format!("creating {path}: {e}"))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), size, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .map_err(|e| format!("writing {path}: {e}"))
}

/// Arguments: an output path and an optional size, 2560x1440 by default.
fn parse(args: &[String]) -> Result<(String, u32, u32), String> {
    let side = |s: &str| -> Result<u32, String> {
        s.parse::<u32>()
            .ok()
            .filter(|v| (1..=MAX_SIDE).contains(v))
            .ok_or_else(|| format!("{s:?} is not a size between 1 and {MAX_SIDE}"))
    };
    match args {
        [out] => Ok((out.clone(), 2560, 1440)),
        [out, w, h] => Ok((out.clone(), side(w)?, side(h)?)),
        _ => Err("expected an output path, and optionally a width and height".into()),
    }
}

/// Paint the backdrop into an ARGB buffer.
fn render(width: u32, height: u32) -> Result<Vec<u32>, String> {
    let len = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "image too large for this machine".to_string())?;
    let mut pixels = vec![0u32; len];
    let palette = Palette::alpymist();
    let backdrop = Backdrop::compose(width, height, &palette, SCENE_SEED);
    let mut canvas = Canvas::from_pixels(
        &mut pixels,
        Size::new(width, height),
        width,
        PixelFormat::Argb8888,
    )
    .ok_or("could not create a canvas of that size")?;
    paint_backdrop(&mut canvas, &backdrop);
    Ok(pixels)
}

/// Where the badge sits on the boot background, as a fraction of the height.
///
/// High enough that a boot menu drawn under it has the lower two-thirds to
/// itself: syslinux and GRUB both start their entries near the middle, and a
/// mark they overlap is worse than no mark.
const BOOT_BADGE_TOP: u32 = 8;
/// The badge's side on the boot background, as a fraction of the height.
const BOOT_BADGE_SIZE: u32 = 4;

/// Paint the boot menu's background: the backdrop, with the badge above the
/// space the menu will use.
fn render_boot(width: u32, height: u32) -> Result<Vec<u32>, String> {
    let len = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "image too large for this machine".to_string())?;
    let mut pixels = vec![0u32; len];
    let palette = Palette::alpymist();
    let backdrop = Backdrop::compose(width, height, &palette, SCENE_SEED);
    let mut canvas = Canvas::from_pixels(
        &mut pixels,
        Size::new(width, height),
        width,
        PixelFormat::Argb8888,
    )
    .ok_or("could not create a canvas of that size")?;
    paint_backdrop(&mut canvas, &backdrop);

    let size = i32::try_from(height / BOOT_BADGE_SIZE).unwrap_or(i32::MAX);
    let left = (i32::try_from(width).unwrap_or(i32::MAX) - size) / 2;
    let top = i32::try_from(height / BOOT_BADGE_TOP).unwrap_or(0);
    paint_badge(&mut canvas, (left, top), size, &palette);
    Ok(pixels)
}

fn write_png(path: &str, pixels: &[u32], width: u32, height: u32) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("creating {path}: {e}"))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    // Opaque RGB: a wallpaper has no transparency, and dropping alpha saves a
    // quarter of the size before compression.
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    let mut rgb = Vec::with_capacity(pixels.len() * 3);
    for px in pixels {
        let [_, r, g, b] = px.to_be_bytes();
        rgb.extend_from_slice(&[r, g, b]);
    }
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .map_err(|e| format!("writing {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{parse, render};

    #[test]
    fn the_mark_defaults_to_an_icon_sized_square() {
        use super::parse_mark;
        assert_eq!(parse_mark(&["m.png".into()]).unwrap(), ("m.png".into(), 64));
        assert_eq!(parse_mark(&["m.png".into(), "32".into()]).unwrap().1, 32);
        assert!(parse_mark(&["m.png".into(), "4".into()]).is_err());
    }

    #[test]
    fn a_path_alone_gets_the_default_size() {
        assert_eq!(
            parse(&["a.png".into()]).unwrap(),
            ("a.png".into(), 2560, 1440)
        );
    }

    #[test]
    fn nonsense_sizes_are_refused() {
        for (w, h) in [("0", "10"), ("x", "10"), ("10", "99999")] {
            assert!(
                parse(&["a.png".into(), w.into(), h.into()]).is_err(),
                "{w}x{h}"
            );
        }
    }

    /// The backdrop is a cold night sky over dark ridges: the top should be
    /// darker than white and the image should not be one flat colour.
    #[test]
    fn it_draws_mountains_not_a_blank() {
        let px = render(320, 180).unwrap();
        let distinct: std::collections::BTreeSet<u32> =
            px.iter().map(|p| p & 0x00FF_FFFF).collect();
        assert!(distinct.len() > 8, "only {} colours", distinct.len());
        assert!(px.iter().all(|p| p & 0x00FF_FFFF != 0x00FF_FFFF));
    }
}

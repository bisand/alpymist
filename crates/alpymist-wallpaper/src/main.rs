//! Render the Alpymist backdrop to a PNG.
//!
//! `alpymist-wallpaper OUT.png [WIDTH HEIGHT]`
//! `alpymist-wallpaper --mark OUT.png [SIZE]`
//! `alpymist-wallpaper --boot OUT.png|OUT.jpg [WIDTH HEIGHT [PICTURE.jpg]]`
//!
//! Run while building the desktop package, so the drawn wallpaper is the same
//! misty mountains as the installer and the login screen — drawn by the same
//! code with the same seed, rather than a picture that could drift from them.
//!
//! The boot menus are the exception: they show the same photograph as the
//! splash that follows them, so `--boot` scales that onto the menu's size and
//! puts the badge over it. Without one it draws the mountains, as before.

#![forbid(unsafe_code)]

use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::badge;
use alpymist_ui::palette::Palette;
use alpymist_ui::picture::Picture;
use alpymist_ui::render::{paint_backdrop, paint_badge};
use denise::PixelFormat;
use denise::geom::{Rect, Size};
use denise::pixels::PixelView;
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
            eprintln!(
                "       alpymist-wallpaper --boot OUT.png|OUT.jpg [WIDTH HEIGHT [PICTURE.jpg]]"
            );
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
        let (sized, picture) = match rest.1 {
            [out, w, h, picture] => (vec![out.clone(), w.clone(), h.clone()], Some(picture)),
            other => (other.to_vec(), None),
        };
        let (out, width, height) = parse(&sized)?;
        let picture = picture
            .map(|p| Picture::load(std::path::Path::new(p)))
            .transpose()?;
        let pixels = render_boot(width, height, picture.as_ref())?;
        write_image(&out, &pixels, width, height)?;
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

/// Paint the boot menu's background: the picture, or the drawn backdrop if
/// there is none, with the badge above the space the menu will use.
fn render_boot(width: u32, height: u32, picture: Option<&Picture>) -> Result<Vec<u32>, String> {
    let len = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "image too large for this machine".to_string())?;
    let mut pixels = vec![0u32; len];
    let palette = Palette::alpymist();
    let size = Size::new(width, height);
    let covered = picture
        .map(|p| p.cover(width, height).ok_or("could not scale the picture"))
        .transpose()?;
    let mut canvas = Canvas::from_pixels(&mut pixels, size, width, PixelFormat::Argb8888)
        .ok_or("could not create a canvas of that size")?;
    if let Some(covered) = &covered {
        let view = PixelView::new(covered, size, width).ok_or("picture the wrong size")?;
        canvas.copy_from(&view, &[Rect::from_size(size)]);
    } else {
        let backdrop = Backdrop::compose(width, height, &palette, SCENE_SEED);
        paint_backdrop(&mut canvas, &backdrop);
    }

    let size = i32::try_from(height / BOOT_BADGE_SIZE).unwrap_or(i32::MAX);
    let left = (i32::try_from(width).unwrap_or(i32::MAX) - size) / 2;
    let top = i32::try_from(height / BOOT_BADGE_TOP).unwrap_or(0);
    paint_badge(&mut canvas, (left, top), size, &palette);
    Ok(pixels)
}

/// Pack an ARGB word buffer down to the opaque RGB bytes both encoders want.
fn to_rgb(pixels: &[u32]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(pixels.len() * 3);
    for px in pixels {
        let [_, r, g, b] = px.to_be_bytes();
        rgb.extend_from_slice(&[r, g, b]);
    }
    rgb
}

/// Write `path` as a JPEG or a PNG, whichever its extension asks for.
///
/// GRUB decodes only what it was built with, and Alpine's `grub-efi` for
/// arm64-efi ships no `png.mod` at all — 145 modules, `jpeg` among them and no
/// PNG decoder anywhere. `jpeg.mod` is there on x86_64-efi too, so a JPEG is
/// the one format both boot menus can read. syslinux's vesamenu takes either,
/// so its background stays a PNG and only GRUB's is transcoded.
fn write_image(path: &str, pixels: &[u32], width: u32, height: u32) -> Result<(), String> {
    let jpeg = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
    if jpeg {
        write_jpeg(path, pixels, width, height)
    } else {
        write_png(path, pixels, width, height)
    }
}

/// Quality for the boot background.
///
/// It is a soft gradient with one hard-edged mark on it, which is the part
/// that would show ringing first; 92 leaves none visible at the size a boot
/// menu is drawn, and the file is a fraction of the PNG either way.
const JPEG_QUALITY: u8 = 92;

fn write_jpeg(path: &str, pixels: &[u32], width: u32, height: u32) -> Result<(), String> {
    let (w, h) = (
        u16::try_from(width).map_err(|_| "image too wide for a JPEG".to_string())?,
        u16::try_from(height).map_err(|_| "image too tall for a JPEG".to_string())?,
    );
    let mut buf = Vec::new();
    jpeg_encoder::Encoder::new(&mut buf, JPEG_QUALITY)
        .encode(&to_rgb(pixels), w, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| format!("encoding {path}: {e}"))?;
    number_components_from_one(&mut buf)?;
    std::fs::write(path, &buf).map_err(|e| format!("writing {path}: {e}"))
}

/// Renumber the colour components from 0,1,2 to 1,2,3, in place.
///
/// The JPEG spec treats a component's id as an arbitrary label, and
/// jpeg-encoder numbers them from zero. JFIF numbers them from one, and that
/// is what a bootloader's reader expects: GRUB computes `id - 1` and then
/// bounds-checks the result, so a file whose first component is 0 underflows
/// and is refused outright —
/// `jpeg.c:grub_jpeg_decode_sof:372:jpeg: invalid index`. It refuses the file,
/// `background_image` fails, and since `grub.cfg` is not `set -e` the menu
/// comes up with no picture and no explanation.
///
/// Six bytes: the three ids in the frame header, and the three selectors in
/// the scan header that have to go on matching them.
fn number_components_from_one(buf: &mut [u8]) -> Result<(), String> {
    let word = |b: &[u8], i: usize| -> usize { (b[i] as usize) << 8 | b[i + 1] as usize };
    let mut i = 2; // past the start-of-image marker
    let mut renumbered = false;
    while i + 4 <= buf.len() {
        if buf[i] != 0xFF {
            return Err("not a JPEG: expected a marker".into());
        }
        let marker = buf[i + 1];
        // Standalone markers carry no length; none of them appear before the
        // scan header in what we write, but step over them rather than
        // mis-reading the next two bytes as a length.
        if matches!(marker, 0x01 | 0xD0..=0xD9) {
            i += 2;
            continue;
        }
        let len = word(buf, i + 2);
        let body = i + 4;
        // The length counts its own two bytes, so the next marker is at
        // i + 2 + len. Checked, because the length is read out of the file.
        let next = i
            .checked_add(2)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| "not a JPEG: segment length overflows".to_string())?;
        if next > buf.len() {
            return Err("not a JPEG: segment runs past the end".into());
        }
        match marker {
            // Frame header: precision, height, width, count, then id/sampling/quant each.
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                let count = buf[body + 5] as usize;
                for c in 0..count {
                    let at = body + 6 + c * 3;
                    if at >= buf.len() {
                        return Err("not a JPEG: truncated frame header".into());
                    }
                    if buf[at] == 0 {
                        renumbered = true;
                    }
                    buf[at] += 1;
                }
            }
            // Scan header: count, then selector/tables each. Its selectors name
            // the ids above, so they move with them.
            0xDA => {
                let count = buf[body] as usize;
                for c in 0..count {
                    let at = body + 1 + c * 2;
                    if at >= buf.len() {
                        return Err("not a JPEG: truncated scan header".into());
                    }
                    buf[at] += 1;
                }
                // Entropy-coded data follows; there is nothing further to walk.
                return if renumbered {
                    Ok(())
                } else {
                    Err("the encoder already numbered components from one".into())
                };
            }
            _ => {}
        }
        i = next;
    }
    Err("not a JPEG: no scan header".into())
}

fn write_png(path: &str, pixels: &[u32], width: u32, height: u32) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("creating {path}: {e}"))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    // Opaque RGB: a wallpaper has no transparency, and dropping alpha saves a
    // quarter of the size before compression.
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    let rgb = to_rgb(pixels);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .map_err(|e| format!("writing {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{number_components_from_one, parse, render, render_boot, to_rgb};
    use alpymist_ui::picture::Picture;

    /// The component ids out of a JPEG's frame header.
    fn component_ids(buf: &[u8]) -> Vec<u8> {
        let mut i = 2;
        while i + 4 <= buf.len() {
            let marker = buf[i + 1];
            let len = (buf[i + 2] as usize) << 8 | buf[i + 3] as usize;
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let count = buf[i + 9] as usize;
                return (0..count).map(|c| buf[i + 10 + c * 3]).collect();
            }
            i += 2 + len;
        }
        Vec::new()
    }

    fn encode_tiny() -> Vec<u8> {
        let pixels = vec![0x00FF_8040_u32; 16 * 16];
        let mut buf = Vec::new();
        jpeg_encoder::Encoder::new(&mut buf, 90)
            .encode(&to_rgb(&pixels), 16, 16, jpeg_encoder::ColorType::Rgb)
            .expect("the encoder writes a tiny image");
        buf
    }

    #[test]
    fn a_jpeg_leaves_the_encoder_numbered_from_zero() {
        // If this ever stops being true the renumbering must go, not stay:
        // it would be adding one to ids that were already right.
        assert_eq!(component_ids(&encode_tiny()), vec![0, 1, 2]);
    }

    #[test]
    fn renumbering_gives_the_components_the_ids_a_bootloader_expects() {
        let mut buf = encode_tiny();
        number_components_from_one(&mut buf).expect("renumbers");
        assert_eq!(component_ids(&buf), vec![1, 2, 3]);
    }

    #[test]
    fn renumbering_twice_is_refused_rather_than_silently_wrong() {
        let mut buf = encode_tiny();
        number_components_from_one(&mut buf).expect("renumbers");
        assert!(number_components_from_one(&mut buf).is_err());
    }

    #[test]
    fn something_that_is_not_a_jpeg_is_refused() {
        assert!(number_components_from_one(&mut [0xFF, 0xD8, 0x00, 0x01, 0x02]).is_err());
    }

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

    /// With a picture the boot background is that picture, and the badge
    /// still goes over it, where the menu leaves room.
    #[test]
    fn a_boot_picture_is_the_background_with_the_badge_on_it() {
        let grey = 0x40;
        let picture = Picture::from_rgb(32, 18, vec![grey; 32 * 18 * 3]).unwrap();
        let (w, h) = (640, 480);
        let px = render_boot(w, h, Some(&picture)).unwrap();
        let flat = 0xFF40_4040;
        assert_eq!(px[0], flat, "the corner is the picture");
        assert_eq!(px[px.len() - 1], flat);
        // The badge's centre, a quarter of the height wide from an eighth down.
        let (cx, cy) = (w / 2, h / 8 + h / 8);
        assert_ne!(
            px[(cy * w + cx) as usize],
            flat,
            "no badge over the picture"
        );
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

//! Screenshots: fetched when an entry's page is opened, kept on disk and in
//! memory, and cut to the size they are shown at.
//!
//! The addresses come from the catalogue Flathub signs, and only `https`
//! ones are kept. They are fetched with busybox `wget`, which every Alpine
//! system has, into `$XDG_CACHE_HOME/alpymist-store/screenshots` under the
//! SHA-1 of the address, so a page opened twice fetches nothing the second
//! time. What comes back is untrusted: it must be a PNG of at most
//! [`MAX_BYTES`] bytes and [`MAX_PIXELS`] pixels, decoded by the `png` crate,
//! which is memory-safe Rust.

use crate::icons::{self, Rgba};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

/// The largest file fetched.
pub const MAX_BYTES: u64 = 12 << 20;

/// The most pixels a screenshot may have: a 4K picture.
pub const MAX_PIXELS: u64 = 3840 * 2160;

/// Seconds to wait for a server before giving up.
const TIMEOUT: &str = "20";

/// Where fetched screenshots are kept.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("alpymist-store/screenshots"))
}

/// Whether `url` is one this module will fetch: `https`, and nothing a
/// command line could read as anything but an address.
#[must_use]
pub fn fetchable(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() < 2048
        && url
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'\\' | b'`'))
}

/// Fetch `url` into the cache, unless it is there, and decode it.
///
/// # Errors
/// Not an address to fetch, no network, not a PNG, or too big.
pub fn fetch(url: &str) -> Result<Rgba, String> {
    if !fetchable(url) {
        return Err("not an https address".into());
    }
    let dir = cache_dir().ok_or("no home directory to keep screenshots in")?;
    let name = sha1_smol::Sha1::from(url.as_bytes()).digest().to_string();
    let path = dir.join(format!("{name}.png"));
    if !path.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let partial = dir.join(format!("{name}.part"));
        let status = Command::new("wget")
            .args(["-q", "-T", TIMEOUT, "-O"])
            .arg(&partial)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("could not run wget: {e}"))?;
        let size = std::fs::metadata(&partial).map_or(0, |m| m.len());
        if !status.success() || size == 0 || size > MAX_BYTES {
            std::fs::remove_file(&partial).ok();
            return Err(if size > MAX_BYTES {
                "the screenshot is too big".into()
            } else {
                "could not fetch the screenshot".into()
            });
        }
        std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_or(0, |m| m.len()) > MAX_BYTES {
        return Err("the screenshot is too big".into());
    }
    icons::decode(std::io::BufReader::new(file), MAX_PIXELS).ok_or_else(|| {
        // A picture that cannot be shown is not kept to be tried again.
        std::fs::remove_file(&path).ok();
        "the screenshot is not a picture the store can show".to_owned()
    })
}

/// Where a screenshot is.
#[derive(Debug, Clone)]
pub enum Picture {
    /// Being fetched.
    Loading,
    /// Fetched and decoded.
    Ready(Arc<Rgba>),
    /// Not to be had.
    Failed(String),
}

/// Screenshots by address, decoded, and cut to the sizes drawn.
#[derive(Debug, Default)]
pub struct Pictures {
    pictures: HashMap<String, Picture>,
    scaled: HashMap<(String, u32, u32), Arc<Vec<u32>>>,
}

/// How many decoded screenshots are kept before the oldest pages' go.
const KEEP: usize = 24;

impl Pictures {
    /// Whether `url` has been asked for. Marks it as being fetched when not.
    pub fn request(&mut self, url: &str) -> bool {
        if self.pictures.contains_key(url) {
            return false;
        }
        if self.pictures.len() >= KEEP {
            // Failed ones are tried again after this; so be it.
            self.pictures.retain(|_, p| matches!(p, Picture::Loading));
            self.scaled.clear();
        }
        self.pictures.insert(url.to_owned(), Picture::Loading);
        true
    }

    /// A fetch finished.
    pub fn arrived(&mut self, url: String, result: Result<Rgba, String>) {
        let picture = match result {
            Ok(image) => Picture::Ready(Arc::new(image)),
            Err(e) => Picture::Failed(e),
        };
        self.pictures.insert(url, picture);
    }

    /// Where `url` is.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&Picture> {
        self.pictures.get(url)
    }

    /// `url` cut to fit `max_w` by `max_h`: its pixels, width and height.
    pub fn fitted(
        &mut self,
        url: &str,
        max_w: u32,
        max_h: u32,
    ) -> Option<(Arc<Vec<u32>>, u32, u32)> {
        let Some(Picture::Ready(image)) = self.pictures.get(url) else {
            return None;
        };
        let (w, h) = icons::fit(image.width, image.height, max_w, max_h);
        let key = (url.to_owned(), w, h);
        if let Some(pixels) = self.scaled.get(&key) {
            return Some((Arc::clone(pixels), w, h));
        }
        let pixels = Arc::new(icons::scale(image, w, h));
        if self.scaled.len() >= KEEP {
            self.scaled.clear();
        }
        self.scaled.insert(key, Arc::clone(&pixels));
        Some((pixels, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::{Picture, Pictures, fetchable};
    use crate::icons::Rgba;

    #[test]
    fn only_plain_https_addresses_are_fetched() {
        assert!(fetchable(
            "https://dl.flathub.org/media/app/x/1248x787@1.png"
        ));
        assert!(!fetchable("http://dl.flathub.org/x.png"));
        assert!(!fetchable("-O/etc/passwd"));
        assert!(!fetchable("https://a b"));
        assert!(!fetchable("https://example.org/`reboot`"));
        assert!(!fetchable("file:///etc/shadow"));
    }

    #[test]
    fn a_picture_is_asked_for_once_and_cut_once_per_size() {
        let mut pictures = Pictures::default();
        assert!(pictures.request("https://x/a.png"));
        assert!(!pictures.request("https://x/a.png"));
        assert!(matches!(
            pictures.get("https://x/a.png"),
            Some(Picture::Loading)
        ));
        pictures.arrived(
            "https://x/a.png".into(),
            Ok(Rgba {
                pixels: vec![[255, 0, 0, 255]; 40 * 20],
                width: 40,
                height: 20,
            }),
        );
        let (pixels, w, h) = pictures.fitted("https://x/a.png", 20, 20).unwrap();
        assert_eq!((w, h, pixels.len()), (20, 10, 200));
        assert!(pictures.fitted("https://x/missing.png", 20, 20).is_none());
    }
}

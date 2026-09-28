//! What was copied, newest first.
//!
//! An entry is the bytes of one type — text, or an image — as the program that
//! copied it offered them. Copying the same thing again moves it to the top
//! rather than adding it twice, which is also what happens when an entry is
//! pasted from the history: putting it back on the clipboard copies it again.
//! Pinned entries are never trimmed and never forgotten at a lock; only the
//! person forgets them.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;

/// The largest text kept, in bytes: a copied log or file is not what a
/// history is for, and holding it would cost every later list.
pub const MAX_TEXT: usize = 1 << 20;
/// The largest image kept, in bytes.
pub const MAX_IMAGE: usize = 16 << 20;

/// One thing copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Stable for as long as the history is: what the picker names it by.
    pub id: u64,
    /// Its type: `text/plain;charset=utf-8`, `image/png`.
    pub mime: String,
    /// The bytes.
    pub data: Vec<u8>,
    /// Kept whatever happens.
    pub pinned: bool,
}

/// How an entry reads in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// The entry.
    pub id: u64,
    /// Its type.
    pub mime: String,
    /// One line: the text's first, or what the image is.
    pub label: String,
    /// Pinned.
    pub pinned: bool,
}

impl Entry {
    /// Whether it is text.
    #[must_use]
    pub fn is_text(&self) -> bool {
        is_text(&self.mime)
    }

    /// How it reads in a list.
    #[must_use]
    pub fn summary(&self) -> Summary {
        Summary {
            id: self.id,
            mime: self.mime.clone(),
            label: self.label(),
            pinned: self.pinned,
        }
    }

    fn label(&self) -> String {
        if self.is_text() {
            let text = String::from_utf8_lossy(&self.data);
            let words: Vec<&str> = text.split_whitespace().collect();
            let mut line = words.join(" ");
            if line.chars().count() > 120 {
                line = line.chars().take(119).collect::<String>() + "…";
            }
            return line;
        }
        let kind = self
            .mime
            .strip_prefix("image/")
            .unwrap_or(&self.mime)
            .to_uppercase();
        match png_size(&self.data) {
            Some((w, h)) => format!("Image · {w}×{h} · {kind}"),
            None => format!("Image · {kind} · {} KiB", self.data.len().div_ceil(1024)),
        }
    }
}

/// Whether a type is text.
#[must_use]
pub fn is_text(mime: &str) -> bool {
    mime.starts_with("text/") || mime == "UTF8_STRING" || mime == "STRING" || mime == "TEXT"
}

/// A PNG's width and height, from its header.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
    Some((w, h))
}

/// The history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    entries: Vec<Entry>,
    next: u64,
}

impl History {
    /// Every entry, newest first.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// An entry by id.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Something was copied. Nothing, or too much, is not kept; the same
    /// thing again goes back to the top, pinned or not, rather than twice.
    /// Returns whether anything changed.
    pub fn add(&mut self, mime: &str, data: Vec<u8>, limit: usize) -> bool {
        let empty = if is_text(mime) {
            String::from_utf8_lossy(&data).trim().is_empty()
        } else {
            data.is_empty()
        };
        let max = if is_text(mime) { MAX_TEXT } else { MAX_IMAGE };
        if empty || data.len() > max {
            return false;
        }
        if let Some(at) = self
            .entries
            .iter()
            .position(|e| e.mime == mime && e.data == data)
        {
            if at == 0 {
                return false;
            }
            let entry = self.entries.remove(at);
            self.entries.insert(0, entry);
            return true;
        }
        self.next += 1;
        self.entries.insert(
            0,
            Entry {
                id: self.next,
                mime: mime.to_owned(),
                data,
                pinned: false,
            },
        );
        self.trim(limit);
        true
    }

    /// Keep at most `limit` unpinned entries, the newest.
    pub fn trim(&mut self, limit: usize) {
        let mut unpinned = 0;
        self.entries.retain(|e| {
            if e.pinned {
                return true;
            }
            unpinned += 1;
            unpinned <= limit
        });
    }

    /// Pin or unpin one. Returns whether it was there.
    pub fn pin(&mut self, id: u64, pinned: bool) -> bool {
        self.entries
            .iter_mut()
            .find(|e| e.id == id)
            .map(|e| e.pinned = pinned)
            .is_some()
    }

    /// Forget one, pinned or not. Returns whether it was there.
    pub fn forget(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// Forget everything but what is pinned.
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.pinned);
    }

    /// Write it: a line per entry saying what follows, then its bytes.
    ///
    /// # Errors
    /// Writing failed.
    pub fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        for e in &self.entries {
            writeln!(
                out,
                "{} {} {} {}",
                e.id,
                u8::from(e.pinned),
                e.mime,
                e.data.len()
            )?;
            out.write_all(&e.data)?;
        }
        Ok(())
    }

    /// Read what [`History::write`] wrote. Whatever cannot be read ends it:
    /// a damaged file loses its tail, not the history.
    pub fn read(input: &mut impl BufRead) -> Self {
        let mut history = Self::default();
        let mut line = String::new();
        loop {
            line.clear();
            if input.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let fields: Vec<&str> = line.trim_end().splitn(4, ' ').collect();
            let [id, pinned, mime, len] = fields.as_slice() else {
                break;
            };
            let (Ok(id), Ok(len)) = (id.parse::<u64>(), len.parse::<usize>()) else {
                break;
            };
            if len > MAX_IMAGE {
                break;
            }
            let mut data = vec![0; len];
            if input.read_exact(&mut data).is_err() {
                break;
            }
            history.next = history.next.max(id);
            history.entries.push(Entry {
                id,
                mime: (*mime).to_owned(),
                data,
                pinned: *pinned == "1",
            });
        }
        history
    }

    /// Save to `path`, readable by this account only.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt as _;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = path.with_extension("new");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&partial)?;
        self.write(&mut file)?;
        file.sync_all()?;
        std::fs::rename(&partial, path)
    }

    /// Load from `path`, or nothing.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::File::open(path)
            .map(|f| Self::read(&mut std::io::BufReader::new(f)))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::History;

    const TEXT: &str = "text/plain;charset=utf-8";

    fn add(h: &mut History, text: &str) {
        h.add(TEXT, text.as_bytes().to_vec(), 3);
    }

    fn labels(h: &History) -> Vec<String> {
        h.entries().iter().map(|e| e.summary().label).collect()
    }

    #[test]
    fn newest_first_the_same_twice_moves_up_and_pins_are_kept() {
        let mut h = History::default();
        for t in ["one", "two", "three"] {
            add(&mut h, t);
        }
        add(&mut h, "one");
        assert_eq!(labels(&h), ["one", "three", "two"]);
        let two = h.entries()[2].id;
        assert!(h.pin(two, true));
        add(&mut h, "four");
        add(&mut h, "five");
        // Three unpinned, and the pinned one besides.
        assert_eq!(labels(&h), ["five", "four", "one", "two"]);
        h.clear();
        assert_eq!(labels(&h), ["two"]);
        assert!(h.forget(two));
        assert!(h.entries().is_empty());
    }

    #[test]
    fn blank_or_huge_is_not_kept() {
        let mut h = History::default();
        assert!(!h.add(TEXT, b"  \n".to_vec(), 10));
        assert!(!h.add(TEXT, vec![b'a'; super::MAX_TEXT + 1], 10));
        assert!(h.entries().is_empty());
    }

    #[test]
    fn a_label_is_one_line_and_an_image_says_its_size() {
        let mut h = History::default();
        add(&mut h, "  first\n  second\tline ");
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(1280u32.to_be_bytes());
        png.extend(720u32.to_be_bytes());
        h.add("image/png", png, 10);
        assert_eq!(labels(&h), ["Image · 1280×720 · PNG", "first second line"]);
    }

    #[test]
    fn written_and_read_back_it_is_the_same() {
        let mut h = History::default();
        add(&mut h, "a b\nc");
        h.add("image/png", vec![0, 1, 2, 10, 13], 10);
        let first = h.entries()[1].id;
        h.pin(first, true);
        let mut bytes = Vec::new();
        h.write(&mut bytes).unwrap();
        let back = History::read(&mut bytes.as_slice());
        assert_eq!(back, h);
        // A damaged tail loses the tail only.
        let back = History::read(&mut &bytes[..bytes.len() - 3]);
        assert_eq!(back.entries().len(), 1);
    }
}

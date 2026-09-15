//! Files written from settings, and telling them from files edited by hand.
//!
//! A generated file begins with two comment lines: what wrote it, and a hash
//! of everything after them. If the rest no longer hashes to that, someone
//! changed it, and it is left alone (ADR 0007). A file from before Alpymist
//! wrote it this way, such as the installer's keyboard layout, is taken over
//! when it still has exactly the shape Alpymist would have written.

use std::path::Path;

/// What a generated file's second line starts with.
const HASH: &str = "alpymist-hash:";

/// What is on disk at a generated file's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Nothing yet.
    Missing,
    /// Written by Alpymist and unchanged since, or in a shape Alpymist wrote.
    Ours,
    /// Changed by hand, or never Alpymist's.
    HandEdited,
}

/// FNV-1a, 64 bits: enough to notice an edit, not a defence against anyone.
fn hash(body: &str) -> u64 {
    body.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A generated file's whole text: the two header lines, commented with
/// `comment`, then `body`.
#[must_use]
pub fn render(comment: &str, source: &str, body: &str) -> String {
    format!(
        "{comment} Written by alpymist from {source}. Change it with Settings or `alpymist set`;\n\
         {comment} {HASH} {:016x} (an edit below keeps this file as you leave it)\n{body}",
        hash(body)
    )
}

/// What `text`, read from a generated file's path, is.
pub fn classify(text: &str, comment: &str, adopt: impl Fn(&str) -> bool) -> State {
    let mut lines = text.splitn(3, '\n');
    let (first, second, rest) = (lines.next(), lines.next(), lines.next());
    if let (Some(first), Some(second), Some(body)) = (first, second, rest)
        && first.starts_with(comment)
        && let Some(recorded) = second
            .strip_prefix(comment)
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix(HASH))
            .and_then(|s| s.split_whitespace().next())
    {
        return if u64::from_str_radix(recorded, 16).ok() == Some(hash(body)) {
            State::Ours
        } else {
            State::HandEdited
        };
    }
    if adopt(text) {
        State::Ours
    } else {
        State::HandEdited
    }
}

/// What is at `path`.
///
/// # Errors
/// The file is there but cannot be read.
pub fn state(path: &Path, comment: &str, adopt: impl Fn(&str) -> bool) -> Result<State, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(classify(&text, comment, adopt)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::Missing),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Write `body` to `path` as a generated file, unless it was edited by hand
/// and `force` is not given. Returns whether it was written.
///
/// # Errors
/// Edited by hand, or the directory or file could not be written.
pub fn write(
    path: &Path,
    comment: &str,
    source: &str,
    body: &str,
    adopt: impl Fn(&str) -> bool,
    force: bool,
) -> Result<(), String> {
    if !force && state(path, comment, adopt)? == State::HandEdited {
        return Err(format!(
            "{} was edited by hand, so it was left as it is; \
             `--force` replaces it with what the settings say",
            path.display()
        ));
    }
    replace(path, &render(comment, source, body))
}

/// Replace `path` with `text` in one step.
///
/// # Errors
/// The directory or file could not be written.
pub fn replace(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| crate::io_error(dir, &e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".alpymist-new");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| crate::io_error(path, &e))
}

#[cfg(test)]
mod tests {
    use super::{State, classify, render};

    #[test]
    fn a_file_alpymist_wrote_is_ours_until_edited() {
        let text = render("#", "settings.toml", "input {\n    sensitivity = 0\n}\n");
        assert_eq!(classify(&text, "#", |_| false), State::Ours);
        let edited = text.replace("= 0", "= 0.5");
        assert_eq!(classify(&edited, "#", |_| false), State::HandEdited);
    }

    #[test]
    fn a_file_from_before_is_taken_over_only_in_the_expected_shape() {
        let installer = "# Written by the Alpymist installer.\ninput {\n}\n";
        let adopt = |t: &str| t.starts_with("# Written by the Alpymist installer");
        assert_eq!(classify(installer, "#", adopt), State::Ours);
        assert_eq!(classify("my own\n", "#", adopt), State::HandEdited);
    }
}

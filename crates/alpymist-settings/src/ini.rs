//! One key in a group of a file other programs write too: a desktop entry,
//! `mimeapps.list`. Everything else in it is left exactly as it was, comments
//! and order and all, since those other programs may be counting on it.

use std::fmt::Write as _;

/// `text` with `key` in `[group]` set to `value`, or taken out with `None`.
///
/// A key that is set goes straight under the group's heading, and any line
/// that set it before is dropped; a group that is not there is added at the
/// end. Every other line is left as it was.
pub(crate) fn with_key(text: &str, group: &str, key: &str, value: Option<&str>) -> String {
    let heading = format!("[{group}]");
    let mut out = String::with_capacity(text.len() + key.len() + 8);
    let mut in_group = false;
    let mut done = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_group = trimmed == heading;
            out.push_str(line);
            if in_group
                && !done
                && let Some(v) = value
            {
                if !line.ends_with('\n') {
                    out.push('\n');
                }
                let _ = writeln!(out, "{key}={v}");
                done = true;
            }
            continue;
        }
        if in_group
            && trimmed
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == key)
        {
            continue;
        }
        out.push_str(line);
    }
    if !done && let Some(v) = value {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        let _ = write!(out, "{heading}\n{key}={v}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::with_key;

    #[test]
    fn a_key_is_set_in_its_group_only() {
        let text = "# hi\n[Desktop Entry]\nName=X\nHidden=false\n[Desktop Action a]\nHidden=keep\n";
        assert_eq!(
            with_key(text, "Desktop Entry", "Hidden", Some("true")),
            "# hi\n[Desktop Entry]\nHidden=true\nName=X\n[Desktop Action a]\nHidden=keep\n"
        );
        assert_eq!(
            with_key(text, "Desktop Entry", "Hidden", None),
            "# hi\n[Desktop Entry]\nName=X\n[Desktop Action a]\nHidden=keep\n"
        );
    }

    #[test]
    fn a_missing_group_is_added_at_the_end() {
        assert_eq!(
            with_key(
                "[Added Associations]\na=b",
                "Default Applications",
                "k",
                Some("v")
            ),
            "[Added Associations]\na=b\n[Default Applications]\nk=v\n"
        );
        assert_eq!(with_key("", "G", "k", Some("v")), "[G]\nk=v\n");
        assert_eq!(with_key("", "G", "k", None), "");
    }
}

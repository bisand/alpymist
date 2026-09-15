//! A Waybar custom module's side of things.
//!
//! Waybar runs a command as a `custom/…` module with `"return-type": "json"`
//! and reads a line of JSON each time something changes: the text, a tooltip,
//! and a class the stylesheet can colour. An empty text hides the module.

use std::io::Write as _;

/// One line for Waybar.
#[must_use]
pub fn line(text: &str, tooltip: &str, class: &str) -> String {
    serde_json::json!({
        "text": text,
        "tooltip": tooltip,
        "class": class,
        "alt": class,
    })
    .to_string()
}

/// Print `read()` now, and again after every `wait()` whenever it differs
/// from the last line printed. Returns when Waybar stops reading.
pub fn follow(mut read: impl FnMut() -> String, mut wait: impl FnMut()) {
    let mut last = String::new();
    loop {
        let line = read();
        if line != last {
            let mut out = std::io::stdout().lock();
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                return;
            }
            last = line;
        }
        wait();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_line_is_one_line_of_json() {
        let l = super::line("x", "two\nlines", "on");
        assert!(!l.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["tooltip"], "two\nlines");
        assert_eq!(v["alt"], "on");
    }
}

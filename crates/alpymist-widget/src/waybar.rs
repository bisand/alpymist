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
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    /// Where the packaged bar lives on a system, and where it is in the tree.
    const INSTALLED: &str = "/usr/share/alpymist/waybar/";
    const TREE: &str = "../../desktop/waybar/";
    const SKEL: &str = "../../desktop/skel/wayland/.config/waybar/";

    /// JSON with `//` comments, as Waybar reads it.
    fn jsonc(path: &Path) -> Value {
        let text =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (mut out, mut in_string, mut escaped) = (String::new(), false, false);
        for line in text.lines() {
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                if in_string {
                    escaped = !escaped && c == '\\';
                    in_string = escaped || c != '"';
                } else if c == '"' {
                    in_string = true;
                } else if c == '/' && chars.peek() == Some(&'/') {
                    break;
                }
                out.push(c);
            }
            out.push('\n');
        }
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// A config with its includes merged in, as Waybar 0.15 merges them: what
    /// is already set wins, objects merge key by key, and an include's own
    /// includes come first. Installed paths are read from the tree.
    fn resolve(path: &Path) -> Value {
        let mut config = jsonc(path);
        let includes: Vec<String> = match &config["include"] {
            Value::String(s) => vec![s.clone()],
            Value::Array(a) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        };
        for include in includes {
            let name = include.strip_prefix(INSTALLED).unwrap_or_else(|| {
                panic!(
                    "{}: includes {include}, not a packaged file",
                    path.display()
                )
            });
            merge(&mut config, resolve(&PathBuf::from(TREE).join(name)));
        }
        config
    }

    fn merge(into: &mut Value, from: Value) {
        if let (Value::Object(into), Value::Object(from)) = (into, from) {
            for (key, value) in from {
                match into.get_mut(&key) {
                    Some(existing @ Value::Object(_)) if value.is_object() => {
                        merge(existing, value);
                    }
                    Some(_) => {}
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
    }

    /// Each account's bar is the packaged one, whole: every module it lists is
    /// defined, and the ones Alpymist runs are Alpymist's.
    #[test]
    fn an_accounts_bar_is_the_packaged_bar() {
        for (compositor, launcher, session) in [
            ("hyprland", "alpymist-menu", "alpymist-menu system"),
            ("labwc", "fuzzel", "labwc --exit"),
        ] {
            let bar = resolve(&PathBuf::from(SKEL).join(format!("{compositor}.jsonc")));
            let listed: Vec<&str> = ["modules-left", "modules-center", "modules-right"]
                .iter()
                .flat_map(|k| {
                    bar[k]
                        .as_array()
                        .unwrap_or_else(|| panic!("{compositor}: no {k}"))
                })
                .filter_map(Value::as_str)
                .collect();
            for module in listed
                .iter()
                .filter(|m| m.starts_with("custom/") || m.starts_with("group/"))
            {
                assert!(
                    bar[module].is_object(),
                    "{compositor}: {module} is listed but not defined"
                );
            }
            assert_eq!(bar["custom/battery"]["exec"], "alpymist-power --waybar");
            assert_eq!(bar["custom/wifi"]["exec"], "alpymist-wifi --waybar");
            assert_eq!(bar["custom/alpymist"]["on-click"], launcher);
            let click = bar["custom/session"]["on-click"]
                .as_str()
                .unwrap_or_default();
            assert!(
                click.contains(session),
                "{compositor}: session runs {click}"
            );
        }
    }

    /// An account's own setting wins, and leaves the rest of the module to
    /// Alpymist.
    #[test]
    fn an_accounts_setting_overrides_only_itself() {
        let mut mine: Value = serde_json::json!({ "clock": { "format": "{:%H:%M}" } });
        merge(
            &mut mine,
            resolve(&PathBuf::from(TREE).join("hyprland.jsonc")),
        );
        assert_eq!(mine["clock"]["format"], "{:%H:%M}");
        assert_eq!(mine["clock"]["tooltip-format"], "<tt>{calendar}</tt>");
    }

    #[test]
    fn an_accounts_style_imports_the_packaged_style() {
        let style = std::fs::read_to_string(PathBuf::from(SKEL).join("style.css")).unwrap();
        let import = format!("@import url(\"file://{INSTALLED}style.css\");");
        assert!(style.contains(&import), "{style}");
        assert!(PathBuf::from(TREE).join("style.css").is_file());
    }

    #[test]
    fn a_line_is_one_line_of_json() {
        let l = super::line("x", "two\nlines", "on");
        assert!(!l.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["tooltip"], "two\nlines");
        assert_eq!(v["alt"], "on");
    }
}

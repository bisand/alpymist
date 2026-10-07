//! The folders and places a file manager shows, drawn in the theme's accent.
//!
//! Adwaita's are a blue of its own, whatever the accent chosen in Settings.
//! These are a small icon theme, `Alpymist`, written into the account's
//! `~/.local/share/icons` from its theme and inheriting Adwaita for
//! everything it does not draw: a folder, each folder a home starts with,
//! and what stands in a file manager's sidebar. Each is one flat shape in
//! the accent, a darker part behind or below it, and a mark in the page's
//! colour.
//!
//! In a drawing, `@F` is the accent, `@B` the accent towards the page, and
//! `@I` the page.

use alpymist_theme::{ThemeFile, hex, mix};
use std::fmt::Write as _;

/// The icon theme's name, which GTK is told.
pub const THEME: &str = "Alpymist";

/// Where the theme is kept, under the account's data directory.
pub const DIRECTORY: &str = "icons/Alpymist";

/// The directories its icons are in, and what GTK calls each kind.
const KINDS: [(&str, &str); 3] = [
    ("places", "Places"),
    ("devices", "Devices"),
    ("actions", "Actions"),
];

/// Each drawing, and the icon names it is: `places/folder`.
const DRAWINGS: &[(&[&str], &str)] = &[
    (
        &["actions/document-open-recent", "places/folder-recent"],
        r#"<circle cx="32" cy="32" r="25" fill="@F"/><circle cx="32" cy="32" r="19" fill="@I" opacity="0.62"/><path d="M32 19v13.5l9 5.5" fill="none" stroke="@F" stroke-width="3.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
    ),
    (
        &["actions/go-home"],
        r#"<path d="M32 7L6 30h7v24a3 3 0 0 0 3 3h32a3 3 0 0 0 3-3V30h7z" fill="@F"/><path d="M32 7L6 30h7l19-16.5L51 30h7z" fill="@B"/><rect x="26.5" y="38" width="11" height="19" rx="1.5" fill="@I" opacity="0.62"/>"#,
    ),
    (
        &["devices/computer"],
        r#"<rect x="6" y="10" width="52" height="34" rx="5" fill="@F"/><rect x="11" y="15" width="42" height="24" rx="2" fill="@I" opacity="0.62"/><path d="M27 44h10l2 8H25z" fill="@B"/><rect x="18" y="51" width="28" height="5" rx="2.5" fill="@F"/>"#,
    ),
    (
        &[
            "devices/drive-harddisk-solidstate",
            "devices/drive-harddisk-system",
            "devices/drive-harddisk",
            "devices/drive-multidisk",
        ],
        r#"<rect x="7" y="16" width="50" height="32" rx="6" fill="@F"/><path d="M7 38h50v4a6 6 0 0 1-6 6H13a6 6 0 0 1-6-6z" fill="@B"/><circle cx="47" cy="43" r="2.4" fill="@F"/><rect x="14" y="41.5" width="16" height="3" rx="1.5" fill="@I" opacity="0.62"/>"#,
    ),
    (
        &[
            "devices/drive-harddisk-usb",
            "devices/drive-removable-media-usb",
            "devices/drive-removable-media",
            "devices/media-flash",
            "devices/media-removable",
        ],
        r#"<rect x="22" y="6" width="20" height="14" rx="2" fill="@B"/><rect x="26.5" y="10" width="4" height="4" fill="@I" opacity="0.62"/><rect x="33.5" y="10" width="4" height="4" fill="@I" opacity="0.62"/><rect x="16" y="18" width="32" height="40" rx="6" fill="@F"/>"#,
    ),
    (
        &["devices/drive-optical", "devices/media-optical"],
        r#"<circle cx="32" cy="32" r="25" fill="@F"/><circle cx="32" cy="32" r="10" fill="@B"/><circle cx="32" cy="32" r="4" fill="@I" opacity="0.8"/>"#,
    ),
    (
        &["devices/phone"],
        r#"<rect x="18" y="6" width="28" height="52" rx="6" fill="@F"/><rect x="22" y="12" width="20" height="36" rx="2" fill="@I" opacity="0.62"/><circle cx="32" cy="53" r="2.2" fill="@B"/>"#,
    ),
    (
        &["places/folder-documents"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M25 26h10l5 5v17H25z"/><path d="M28 36h9M28 40h9M28 44h6" stroke="@F" stroke-width="1.8" fill="none" opacity="1"/></g>"#,
    ),
    (
        &["places/folder-download", "places/folder-downloads"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M29.5 26h5v9h5.5L32 43l-8-8h5.5z"/><rect stroke-width="0" x="23" y="45" width="18" height="2.6" rx="1.3"/></g>"#,
    ),
    (
        &["places/folder-drag-accept", "places/folder-open"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><path d="M11.5 26a4 4 0 0 1 3.9-3h44.1a3 3 0 0 1 2.9 3.8l-6.6 24.5a5 5 0 0 1-4.8 3.7H10a5 5 0 0 1-4.8-6.3z" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"></g>"#,
    ),
    (
        &["places/folder-home", "places/user-home"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M32 26l11 9.5h-3V46h-5.5v-6.5h-5V46H24V35.5h-3z"/></g>"#,
    ),
    (
        &["places/folder-music"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M30 27l11-2.5v16.2a3.6 3.6 0 1 1-2.6-3.4V29.6L32.6 31v12.2a3.6 3.6 0 1 1-2.6-3.4z"/></g>"#,
    ),
    (
        &["places/folder-pictures"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M20 47l8.5-12 5 6.5 3.5-4.5L44 47z"/><circle stroke-width="0" cx="39.5" cy="29.5" r="3"/></g>"#,
    ),
    (
        &["places/folder-projects"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path d="M28.5 31l-6 6 6 6M35.5 31l6 6-6 6" stroke-width="2.6" fill="none" stroke-linecap="round" stroke-linejoin="round"/></g>"#,
    ),
    (
        &["places/folder-publicshare"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><circle stroke-width="0" cx="25" cy="37" r="3.4"/><circle stroke-width="0" cx="39" cy="29.5" r="3.4"/><circle stroke-width="0" cx="39" cy="44.5" r="3.4"/><path d="M25 37l14-7.5M25 37l14 7.5" stroke-width="2" fill="none"/></g>"#,
    ),
    (
        &["places/folder-remote"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><circle cx="32" cy="37" r="9" fill="none" stroke-width="2.2"/><path d="M23 37h18M32 28c-5 5-5 13 0 18M32 28c5 5 5 13 0 18" fill="none" stroke-width="1.8"/></g>"#,
    ),
    (
        &["places/folder-templates"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path d="M25.5 26.5h9.5l4.5 4.5v16.5h-14z" fill="none" stroke-width="2.2" stroke-dasharray="3 2.2"/></g>"#,
    ),
    (
        &["places/folder-videos"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><path stroke-width="0" d="M27 28l14 9-14 9z"/></g>"#,
    ),
    (
        &["places/folder", "places/inode-directory"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"></g>"#,
    ),
    (
        &["places/network-server"],
        r#"<rect x="9" y="9" width="46" height="13" rx="4" fill="@F"/><circle cx="47" cy="15.5" r="2.2" fill="@I" opacity="0.7"/><rect x="15" y="14" width="16" height="3" rx="1.5" fill="@I" opacity="0.62"/><rect x="9" y="25" width="46" height="13" rx="4" fill="@B"/><circle cx="47" cy="32" r="2.2" fill="@I" opacity="0.7"/><rect x="15" y="30.5" width="16" height="3" rx="1.5" fill="@I" opacity="0.62"/><rect x="9" y="42" width="46" height="13" rx="4" fill="@F"/><circle cx="47" cy="48.5" r="2.2" fill="@I" opacity="0.7"/><rect x="15" y="47" width="16" height="3" rx="1.5" fill="@I" opacity="0.62"/>"#,
    ),
    (
        &["places/network-workgroup"],
        r#"<path d="M32 22v14M14 44V36h36v8" fill="none" stroke="@B" stroke-width="3.4" stroke-linejoin="round"/><rect x="23" y="8" width="18" height="15" rx="3.5" fill="@F"/><rect x="5" y="42" width="18" height="15" rx="3.5" fill="@F"/><rect x="41" y="42" width="18" height="15" rx="3.5" fill="@F"/>"#,
    ),
    (
        &["places/user-desktop"],
        r#"<path d="M5 15a5 5 0 0 1 5-5h13.5a4 4 0 0 1 2.9 1.2L31 16h23a5 5 0 0 1 5 5v8H5z" fill="@B"/><rect x="5" y="19" width="54" height="36" rx="5" fill="@F"/><g fill="@I" stroke="@I" opacity="0.62"><rect stroke-width="0" x="21" y="27" width="22" height="14" rx="2"/><rect stroke-width="0" x="29.5" y="41" width="5" height="4"/><rect stroke-width="0" x="25" y="45" width="14" height="2.4" rx="1.2"/></g>"#,
    ),
    (
        &["places/user-trash-full"],
        r#"<path d="M20 12l5-5 6 4 7-5 5 6" fill="none" stroke="@F" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/><rect x="12" y="15" width="40" height="7" rx="3.5" fill="@B"/><rect x="26" y="10" width="12" height="6" rx="2" fill="none"/><path d="M16 22h32l-2.6 32a5 5 0 0 1-5 4.6H23.6a5 5 0 0 1-5-4.6z" fill="@F"/><path d="M26 28v20M32 28v20M38 28v20" stroke="@I" stroke-width="2.6" stroke-linecap="round" opacity="0.62"/>"#,
    ),
    (
        &["places/user-trash"],
        r#"<rect x="12" y="15" width="40" height="7" rx="3.5" fill="@B"/><rect x="26" y="10" width="12" height="6" rx="2" fill="@B"/><path d="M16 22h32l-2.6 32a5 5 0 0 1-5 4.6H23.6a5 5 0 0 1-5-4.6z" fill="@F"/><path d="M26 28v20M32 28v20M38 28v20" stroke="@I" stroke-width="2.6" stroke-linecap="round" opacity="0.62"/>"#,
    ),
];

/// The theme's `index.theme`.
fn index() -> String {
    let directories: Vec<String> = KINDS.iter().map(|(d, _)| format!("scalable/{d}")).collect();
    let mut out = format!(
        "[Icon Theme]\nName={THEME}\nComment=Alpymist's folders and places, over Adwaita\n\
         Inherits=Adwaita,hicolor\nDirectories={}\n",
        directories.join(",")
    );
    for (directory, context) in KINDS {
        let _ = write!(
            out,
            "\n[scalable/{directory}]\nContext={context}\nSize=64\nMinSize=8\nMaxSize=512\nType=Scalable\n"
        );
    }
    out
}

/// Every file of the icon theme for `theme`, by its path under
/// [`DIRECTORY`], with what is in it.
#[must_use]
pub fn files(theme: &ThemeFile) -> Vec<(String, String)> {
    let p = theme.palette();
    let front = format!("#{}", hex(p.accent));
    let back = format!("#{}", hex(mix(p.accent, p.page, 38)));
    let ink = format!("#{}", hex(p.page));
    let mut out = vec![("index.theme".to_owned(), index())];
    for (names, drawing) in DRAWINGS {
        let body = drawing
            .replace("@F", &front)
            .replace("@B", &back)
            .replace("@I", &ink);
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" \
             viewBox=\"0 0 64 64\">{body}</svg>\n"
        );
        for name in *names {
            out.push((format!("scalable/{name}.svg"), svg.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{DRAWINGS, KINDS, files};
    use alpymist_theme::{Accent, Scheme, ThemeFile, hex};

    #[test]
    fn every_icon_is_in_a_directory_the_index_names_and_named_once() {
        let mut seen = std::collections::BTreeSet::new();
        for (names, _) in DRAWINGS {
            assert!(!names.is_empty());
            for name in *names {
                let (kind, icon) = name.split_once('/').expect(name);
                assert!(KINDS.iter().any(|(k, _)| *k == kind), "{name}");
                assert!(!icon.is_empty() && !icon.contains('/'), "{name}");
                assert!(seen.insert(*name), "{name} is drawn twice");
            }
        }
        // What a file manager shows before anything is plugged in.
        for name in [
            "places/folder",
            "places/user-home",
            "places/user-trash",
            "places/network-workgroup",
            "devices/computer",
            "devices/drive-harddisk",
            "actions/go-home",
            "actions/document-open-recent",
        ] {
            assert!(seen.contains(name), "{name}");
        }
    }

    #[test]
    fn the_icons_are_the_accents_colour_and_whole() {
        for scheme in [Scheme::Dark, Scheme::Light] {
            for accent in Accent::ALL {
                let theme = ThemeFile {
                    scheme,
                    accent,
                    ..ThemeFile::default()
                };
                let p = theme.palette();
                let all = files(&theme);
                assert_eq!(all[0].0, "index.theme");
                assert!(all[0].1.contains("Inherits=Adwaita,hicolor"));
                for (path, text) in &all[1..] {
                    assert!(path.starts_with("scalable/"), "{path}");
                    assert_eq!(std::path::Path::new(path).extension().unwrap(), "svg");
                    assert!(
                        text.starts_with("<svg ") && text.ends_with("</svg>\n"),
                        "{path}"
                    );
                    assert!(!text.contains('@'), "{path}: a colour left unnamed");
                    assert!(text.contains(&format!("#{}", hex(p.accent))), "{path}");
                    // As many closed as opened, and no element left open.
                    let opened = text.matches('<').count();
                    let closed = text.matches("/>").count() + 2 * text.matches("</").count();
                    assert_eq!(opened, closed, "{path}");
                }
            }
        }
    }
}

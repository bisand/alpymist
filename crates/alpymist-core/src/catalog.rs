//! The keyboard layouts and time zones the installer and Settings offer.
//!
//! Both lists are generated from Alpine's own `kbd-bkeymaps` and `tzdata` by
//! `cargo xtask installer-data` and compiled in, so every entry is one that
//! `setup-keymap` and `setup-timezone` accept, and the lists work the same in the
//! desktop preview as on the live image.

use std::sync::OnceLock;

/// A console keymap, as `setup-keymap <layout> <variant>` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    /// The `kbd-bkeymaps` directory, e.g. `no`.
    pub layout: &'static str,
    /// The keymap file without `.bmap.gz`, e.g. `no-nodeadkeys`.
    pub variant: &'static str,
    /// Who the layout is for, e.g. `Norway`.
    pub name: &'static str,
}

/// A time zone, as `setup-timezone -z <zone>` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// IANA name, e.g. `Europe/Oslo`.
    pub zone: &'static str,
    /// Country code, lower case, empty for UTC.
    pub country: &'static str,
    /// Country name, e.g. `Norway`.
    pub name: &'static str,
}

/// The layout chosen when nobody has said otherwise, as Alpine's setup does.
pub const DEFAULT_KEYMAP: (&str, &str) = ("us", "us");
/// The time zone chosen when nobody has said otherwise, as Alpine's setup does.
pub const DEFAULT_ZONE: &str = "UTC";

const KEYMAPS_TSV: &str = include_str!("../data/keymaps.tsv");
const ZONES_TSV: &str = include_str!("../data/timezones.tsv");

/// Every keymap, sorted by layout then variant.
#[must_use]
pub fn keymaps() -> &'static [Keymap] {
    static LIST: OnceLock<Vec<Keymap>> = OnceLock::new();
    LIST.get_or_init(|| {
        records(KEYMAPS_TSV)
            .map(|[layout, variant, name]| Keymap {
                layout,
                variant,
                name,
            })
            .collect()
    })
}

/// Every time zone, sorted by name.
#[must_use]
pub fn zones() -> &'static [Zone] {
    static LIST: OnceLock<Vec<Zone>> = OnceLock::new();
    LIST.get_or_init(|| {
        records(ZONES_TSV)
            .map(|[zone, country, name]| Zone {
                zone,
                country,
                name,
            })
            .collect()
    })
}

fn records(tsv: &'static str) -> impl Iterator<Item = [&'static str; 3]> {
    tsv.lines().filter(|l| !l.starts_with('#')).filter_map(|l| {
        let mut f = l.split('\t');
        Some([f.next()?, f.next()?, f.next()?])
    })
}

/// The xkb variant for a console keymap from `kbd-bkeymaps`.
///
/// Those keymaps are generated from xkb and named `<layout>-<variant>`, so
/// `no-mac` is layout `no`, variant `mac`; the plain `no` has no variant.
#[must_use]
pub fn xkb_variant<'a>(layout: &str, keymap: &'a str) -> &'a str {
    keymap
        .strip_prefix(layout)
        .and_then(|rest| rest.strip_prefix('-'))
        .unwrap_or("")
}

impl Keymap {
    /// The row text.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{:<24}{}", self.variant, self.name)
    }

    /// Whether this keymap matches a search.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        matches(query, &[self.variant, self.name])
    }
}

impl Zone {
    /// The row text. Underscores are how IANA spells spaces.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{:<32}{}", self.zone.replace('_', " "), self.name)
    }

    /// Whether this zone matches a search.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        matches(query, &[self.zone, self.country, self.name])
    }
}

/// Whether every word of `query` appears somewhere in `fields`.
///
/// Case-insensitive, and blind to the punctuation the names happen to use, so
/// `new york`, `new_york` and `America/New` all find `America/New_York`. Words
/// may match different fields: `oslo norway` finds Oslo.
#[must_use]
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let haystack = fold(&fields.join(" "));
    fold(query)
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '_' | '-' | '/' | '(' | ')' | ',' => ' ',
            c => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// The position of a keymap in [`keymaps`].
#[must_use]
pub fn keymap_index(layout: &str, variant: &str) -> Option<usize> {
    keymaps()
        .iter()
        .position(|k| k.layout == layout && k.variant == variant)
}

/// The position of a zone in [`zones`].
#[must_use]
pub fn zone_index(zone: &str) -> Option<usize> {
    zones().iter().position(|z| z.zone == zone)
}

#[cfg(test)]
mod tests {
    use super::xkb_variant;
    use super::{DEFAULT_KEYMAP, DEFAULT_ZONE, keymap_index, keymaps, matches, zone_index, zones};

    #[test]
    fn the_lists_are_as_complete_as_alpines() {
        assert!(keymaps().len() > 500, "{}", keymaps().len());
        assert!(zones().len() > 400, "{}", zones().len());
    }

    #[test]
    fn the_defaults_exist() {
        assert!(keymap_index(DEFAULT_KEYMAP.0, DEFAULT_KEYMAP.1).is_some());
        assert!(zone_index(DEFAULT_ZONE).is_some());
    }

    #[test]
    fn a_country_name_finds_its_layouts_and_zones() {
        let layouts: Vec<_> = keymaps().iter().filter(|k| k.matches("norway")).collect();
        assert!(layouts.iter().any(|k| k.variant == "no"));
        assert!(layouts.iter().all(|k| k.layout == "no"));
        assert!(
            zones()
                .iter()
                .any(|z| z.matches("norway") && z.zone == "Europe/Oslo")
        );
    }

    #[test]
    fn searching_ignores_case_and_punctuation() {
        for q in ["new york", "NEW_YORK", "america/new", "york us"] {
            assert!(
                matches(q, &["America/New_York", "us", "United States"]),
                "{q}"
            );
        }
        assert!(!matches(
            "new jersey",
            &["America/New_York", "us", "United States"]
        ));
        assert!(matches("", &["anything"]));
    }

    #[test]
    fn every_entry_is_complete() {
        assert!(
            keymaps()
                .iter()
                .all(|k| !k.layout.is_empty() && !k.variant.is_empty())
        );
        assert!(
            zones()
                .iter()
                .all(|z| !z.zone.is_empty() && !z.name.is_empty())
        );
    }

    #[test]
    fn console_keymaps_map_onto_xkb_variants() {
        assert_eq!(xkb_variant("no", "no"), "");
        assert_eq!(xkb_variant("no", "no-mac"), "mac");
        assert_eq!(xkb_variant("us", "us-altgr-intl"), "altgr-intl");
        assert_eq!(xkb_variant("gb", "gb-colemak_dh"), "colemak_dh");
        assert_eq!(xkb_variant("no", "nodeadkeys"), "");
    }
}

//! Whether the installer can type the way the chosen keyboard layout does.
//!
//! On the live image there is no X or console keymap between the keyboard and
//! the installer: Denise turns key positions into characters itself, and has
//! tables for only a few layouts. Anything else is typed as US.
//!
//! That matters most for secrets. A passphrase typed here as US and later at the
//! boot prompt on a French keymap is a different passphrase, and an encrypted
//! disk nobody can open. So the installer follows the chosen layout where it can
//! and says plainly where it cannot.

use denise_layout::Layout;

/// Denise's table for this keymap, if it has one that types identically.
///
/// Only the plain variant of a layout counts. `no-nodeadkeys`, `us-dvorak` and
/// the like put characters elsewhere, so treating them as `no` or `us` would be
/// exactly the silent mismatch this exists to prevent.
#[must_use]
pub fn layout_for(layout: &str, variant: &str) -> Option<&'static Layout> {
    if layout != variant {
        return None;
    }
    denise_layout::by_name(layout)
}

/// The layout the installer will actually type with for these answers.
///
/// The exact table where there is one; otherwise the base layout's, because
/// `no-mac` typed as Norwegian gets the letters right and only some symbols
/// wrong, where typed as US almost nothing would match; otherwise US.
#[must_use]
pub fn effective_layout(layout: Option<&str>, variant: Option<&str>) -> &'static Layout {
    let Some(layout) = layout else {
        return &denise_layout::US;
    };
    variant
        .and_then(|v| layout_for(layout, v))
        .or_else(|| denise_layout::by_name(layout))
        .unwrap_or(&denise_layout::US)
}

/// The keymap a system is already configured for, from `/etc/conf.d/loadkmap`.
///
/// Alpine writes `KEYMAP=/etc/keymap/<variant>.bmap.gz`. Returns
/// `(layout, variant)` when that names a keymap the installer offers, which
/// makes it the right default: it is how this machine's keyboard already types.
#[must_use]
pub fn configured_keymap(loadkmap: &str) -> Option<(&'static str, &'static str)> {
    let value = loadkmap.lines().find_map(|line| {
        let line = line.trim();
        let (key, value) = line.split_once('=')?;
        (key.trim() == "KEYMAP" && !line.starts_with('#')).then(|| value.trim())
    })?;
    let value = value.trim_matches(['"', '\'']);
    let file = value.rsplit('/').next()?;
    let variant = file
        .strip_suffix(".bmap.gz")
        .or_else(|| file.strip_suffix(".bmap"))?;
    crate::catalog::keymaps()
        .iter()
        .find(|k| k.variant == variant)
        .map(|k| (k.layout, k.variant))
}

#[cfg(test)]
mod tests {
    use super::{configured_keymap, effective_layout, layout_for};

    #[test]
    fn plain_layouts_with_a_table_are_followed() {
        for name in ["us", "no", "de"] {
            assert_eq!(layout_for(name, name).map(|l| l.name), Some(name));
        }
    }

    /// Dvorak is not US with a different name, and nodeadkeys is not Norwegian.
    #[test]
    fn variants_are_never_mistaken_for_their_base_layout() {
        assert!(layout_for("us", "us-dvorak").is_none());
        assert!(layout_for("no", "no-nodeadkeys").is_none());
        assert!(layout_for("fr", "fr").is_none());
    }

    #[test]
    fn anything_unknown_types_as_us() {
        assert_eq!(effective_layout(Some("fr"), Some("fr")).name, "us");
        assert_eq!(effective_layout(None, None).name, "us");
        assert_eq!(effective_layout(Some("no"), Some("no")).name, "no");
    }

    /// Closer beats exact-or-nothing: a Norwegian Mac keyboard typed as
    /// Norwegian gets æ, ø and å right.
    #[test]
    fn a_variant_without_a_table_types_as_its_base_layout() {
        assert_eq!(effective_layout(Some("no"), Some("no-mac")).name, "no");
        assert_eq!(effective_layout(Some("us"), Some("us-dvorak")).name, "us");
    }

    #[test]
    fn alpines_loadkmap_names_the_default() {
        assert_eq!(
            configured_keymap("KEYMAP=/etc/keymap/no.bmap.gz\n"),
            Some(("no", "no"))
        );
        assert_eq!(
            configured_keymap("# KEYMAP=x\nKEYMAP=\"/etc/keymap/gb-extd.bmap.gz\"\n"),
            Some(("gb", "gb-extd"))
        );
        assert_eq!(
            configured_keymap("KEYMAP=/etc/keymap/nonsense.bmap.gz"),
            None
        );
        assert_eq!(configured_keymap(""), None);
    }
}

//! Queries OpenGL ES capability by talking to EGL.
//!
//! # Why this crate exists separately
//!
//! Every other crate in Alpymist is `#![forbid(unsafe_code)]`. Asking EGL what
//! the GPU can do requires calling into a C library, and there is no safe way
//! to `dlopen` one. Rather than weaken the rule everywhere, all `unsafe` in the
//! project is confined to the [`ffi`] module below — a few dozen lines that can
//! be audited in one sitting.
//!
//! libEGL is loaded *dynamically*. Nothing links against it, so `alpymistctl`
//! still runs on a system with no Mesa installed; the probe just reports
//! [`None`], and the tier logic treats that as "assume no acceleration".

#![deny(unsafe_code)]

use alpymist_core::GlesInfo;

mod ffi;

/// Ask EGL what this machine can render.
///
/// Returns [`None`] if libEGL is missing, if no context could be created, or if
/// the strings EGL returned made no sense. Every one of those means the same
/// thing to the caller: we learned nothing, so assume the worst.
#[must_use]
pub fn probe() -> Option<GlesInfo> {
    let raw = ffi::query()?;
    Some(GlesInfo {
        version: parse_gl_version(&raw.version)?,
        renderer: raw.renderer,
        vendor: raw.vendor,
    })
}

/// Parse the `major.minor` out of a `GL_VERSION` string.
///
/// GLES spells it `"OpenGL ES 3.2 Mesa 24.0.5"`, but the prefix is not
/// guaranteed and vendors append arbitrary text, so this scans for the first
/// `N.M` token rather than trusting any particular layout.
fn parse_gl_version(s: &str) -> Option<(u32, u32)> {
    s.split_whitespace().find_map(|token| {
        let (major, rest) = token.split_once('.')?;
        // Trailing patch level, e.g. "3.2.1" -> minor "2".
        let minor = rest.split('.').next()?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    })
}

#[cfg(test)]
mod tests {
    use super::parse_gl_version;

    #[test]
    fn parses_the_usual_mesa_strings() {
        assert_eq!(parse_gl_version("OpenGL ES 3.2 Mesa 24.0.5"), Some((3, 2)));
        assert_eq!(parse_gl_version("OpenGL ES 2.0 Mesa 21.3.9"), Some((2, 0)));
        assert_eq!(
            parse_gl_version("OpenGL ES 3.1 Mesa 23.1.9-1~bpo12+1"),
            Some((3, 1))
        );
    }

    #[test]
    fn parses_a_three_component_version() {
        assert_eq!(parse_gl_version("OpenGL ES 3.2.1 build 1907"), Some((3, 2)));
    }

    #[test]
    fn handles_vendor_strings_without_the_opengl_es_prefix() {
        assert_eq!(parse_gl_version("3.2 NVIDIA 550.54.14"), Some((3, 2)));
    }

    #[test]
    fn rejects_strings_with_no_version_at_all() {
        assert_eq!(parse_gl_version(""), None);
        assert_eq!(parse_gl_version("OpenGL ES"), None);
        assert_eq!(parse_gl_version("no digits here"), None);
    }

    #[test]
    fn does_not_mistake_a_lone_word_for_a_version() {
        assert_eq!(parse_gl_version("Mesa"), None);
    }
}

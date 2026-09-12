//! Parsing what a booting Alpymist image says over its serial console.

/// Marker the first-boot probe service prints before its report.
pub const BEGIN: &str = "=== ALPYMIST-PROBE-BEGIN ===";
/// Marker it prints after.
pub const END: &str = "=== ALPYMIST-PROBE-END ===";

/// Pull the probe report out of a serial log.
///
/// Returns the lines between the markers, exclusive. Boot output is noisy and
/// its ordering varies, so this looks for the markers anywhere rather than
/// assuming the report lands at a particular point.
#[must_use]
pub fn extract_report(lines: &[String]) -> Option<Vec<String>> {
    let begin = lines.iter().position(|l| l.contains(BEGIN))?;
    let end = lines.iter().skip(begin + 1).position(|l| l.contains(END))? + begin + 1;
    Some(lines[begin + 1..end].to_vec())
}

/// Check that a report names a tier, a backend and a metapackage.
///
/// # Errors
/// Returns the list of missing fields.
pub fn validate_report(report: &[String]) -> Result<(), Vec<&'static str>> {
    let text = report.join("\n");
    let missing: Vec<&'static str> = ["tier:", "backend:", "metapackage:", "memory:", "why:"]
        .into_iter()
        .filter(|field| !text.contains(field))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Read the tier a report names, e.g. `Potato` from `tier:        Potato`.
#[must_use]
pub fn reported_tier(report: &[String]) -> Option<String> {
    report
        .iter()
        .find_map(|l| l.split_once("tier:"))
        .map(|(_, rest)| rest.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{BEGIN, END, extract_report, validate_report};

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn extracts_the_block_between_markers() {
        let log = lines(&[
            "boot noise",
            BEGIN,
            "tier: Full",
            "backend: Hyprland",
            END,
            "more noise",
        ]);
        assert_eq!(
            extract_report(&log).unwrap(),
            lines(&["tier: Full", "backend: Hyprland"])
        );
    }

    #[test]
    fn tolerates_markers_with_serial_console_prefixes() {
        let log = lines(&[
            &format!("[    2.13] {BEGIN}"),
            "tier: Potato",
            &format!("[    2.14] {END}"),
        ]);
        assert_eq!(extract_report(&log).unwrap(), lines(&["tier: Potato"]));
    }

    #[test]
    fn returns_none_when_the_boot_never_reached_the_probe() {
        assert_eq!(extract_report(&lines(&["kernel panic"])), None);
        assert_eq!(
            extract_report(&lines(&[BEGIN, "tier: Full"])),
            None,
            "unterminated"
        );
    }

    #[test]
    fn an_empty_report_is_extracted_but_fails_validation() {
        let log = lines(&[BEGIN, END]);
        assert_eq!(extract_report(&log).unwrap(), Vec::<String>::new());
        assert!(validate_report(&[]).is_err());
    }

    #[test]
    fn validation_names_every_missing_field() {
        let report = lines(&["tier: Full", "backend: Hyprland"]);
        let missing = validate_report(&report).unwrap_err();
        assert_eq!(missing, ["metapackage:", "memory:", "why:"]);
    }

    #[test]
    fn reads_the_tier_out_of_a_report() {
        let report = lines(&["tier:        Potato", "backend:     Labwc"]);
        assert_eq!(super::reported_tier(&report).as_deref(), Some("Potato"));
    }

    #[test]
    fn a_report_without_a_tier_has_no_tier() {
        assert_eq!(super::reported_tier(&lines(&["backend: I3"])), None);
        assert_eq!(super::reported_tier(&lines(&["tier:   "])), None);
    }

    /// `metapackage:` also ends in the substring `tier:`-adjacent text; make
    /// sure the match is on the real field.
    #[test]
    fn does_not_match_a_different_field() {
        let report = lines(&["metapackage: alpymist-desktop-lite", "tier: Lite"]);
        assert_eq!(super::reported_tier(&report).as_deref(), Some("Lite"));
    }

    #[test]
    fn a_complete_report_validates() {
        let report = lines(&[
            "tier:        Potato",
            "backend:     Labwc { software_render: true }",
            "metapackage: alpymist-desktop-lite",
            "memory:      2048 MiB",
            "cpus:        2",
            "why:",
            "  - only a firmware framebuffer is available",
        ]);
        assert!(validate_report(&report).is_ok());
    }
}

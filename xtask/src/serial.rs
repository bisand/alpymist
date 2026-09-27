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

/// Check that a report says how Hyprland will do, and why.
///
/// # Errors
/// Returns the list of missing fields.
pub fn validate_report(report: &[String]) -> Result<(), Vec<&'static str>> {
    let text = report.join("\n");
    let missing: Vec<&'static str> = ["hyprland:", "memory:", "why:"]
        .into_iter()
        .filter(|field| !text.contains(field))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Read the verdict a report gives, e.g. `Slow` from
/// `hyprland:    Slow — Hyprland runs here, but slowly`.
#[must_use]
pub fn reported_verdict(report: &[String]) -> Option<String> {
    report
        .iter()
        .find_map(|l| l.trim_start().strip_prefix("hyprland:"))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_string)
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
            "hyprland:    Runs — Hyprland runs well here",
            "memory:      4096 MiB",
            END,
            "more noise",
        ]);
        assert_eq!(
            extract_report(&log).unwrap(),
            lines(&[
                "hyprland:    Runs — Hyprland runs well here",
                "memory:      4096 MiB"
            ])
        );
    }

    #[test]
    fn tolerates_markers_with_serial_console_prefixes() {
        let log = lines(&[
            &format!("[    2.13] {BEGIN}"),
            "hyprland: Slow",
            &format!("[    2.14] {END}"),
        ]);
        assert_eq!(extract_report(&log).unwrap(), lines(&["hyprland: Slow"]));
    }

    #[test]
    fn returns_none_when_the_boot_never_reached_the_probe() {
        assert_eq!(extract_report(&lines(&["kernel panic"])), None);
        assert_eq!(
            extract_report(&lines(&[BEGIN, "hyprland: Runs"])),
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
        let report = lines(&["hyprland: Runs"]);
        let missing = validate_report(&report).unwrap_err();
        assert_eq!(missing, ["memory:", "why:"]);
    }

    #[test]
    fn reads_the_verdict_out_of_a_report() {
        let report = lines(&[
            "memory:      2048 MiB",
            "hyprland:    Slow — Hyprland runs here, but slowly",
        ]);
        assert_eq!(super::reported_verdict(&report).as_deref(), Some("Slow"));
    }

    #[test]
    fn a_report_without_a_verdict_has_none() {
        assert_eq!(super::reported_verdict(&lines(&["memory: 1 MiB"])), None);
        assert_eq!(super::reported_verdict(&lines(&["hyprland:   "])), None);
    }

    #[test]
    fn a_complete_report_validates() {
        let report = lines(&[
            "hyprland:    Slow — Hyprland runs here, but slowly",
            "memory:      2048 MiB",
            "cpus:        2",
            "why:",
            "  - renderer \"llvmpipe\" is a CPU rasteriser, not the GPU",
        ]);
        assert!(validate_report(&report).is_ok());
    }
}

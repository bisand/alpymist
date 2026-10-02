//! `alpymist report`: what this machine is made of, for whoever has it to
//! send, or not.
//!
//! Hardware Alpymist does not drive is only found by somebody with the
//! hardware, and what they would be asked for — which devices, which drivers,
//! which firmware was not there — is the same every time. This gathers it.
//!
//! It sends nothing. Alone it prints the report; `--save` writes it to a file,
//! for a machine with no network; `--issue` shows it, asks, and on a yes opens
//! the browser at a new GitHub issue with the report in it, which is posted
//! when the person presses the button there and not before. No program of
//! Alpymist's runs this for anyone (ADR 0020).
//!
//! What may be in it is [`alpymist_hwprobe::report`]'s to say, and its test's
//! to hold.

use crate::{firmware, launch};
use std::error::Error;
use std::fmt::Write as _;
use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};

/// Where an issue is opened.
const ISSUES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new");
/// The longest address handed to the browser. GitHub answers a longer one
/// with an error page and not a form; this leaves room under what it takes.
const LONGEST_URL: usize = 7000;
/// Where the report goes when it is too long for an address and no file was
/// named.
const FALLBACK: &str = "alpymist-report.txt";

/// How Hyprland will do here, as `alpymist probe` says it.
fn hyprland(out: &mut String) {
    let caps = match alpymist_hwprobe::probe() {
        Ok(caps) => caps,
        Err(e) => {
            let _ = writeln!(out, "not probed: {e}");
            return;
        }
    };
    let check = alpymist_core::hyprland::check(&caps);
    let _ = writeln!(out, "{:<15}{:?}", "verdict", check.verdict);
    for reason in &check.reasons {
        let _ = writeln!(out, "{:<15}{reason}", "");
    }
    let _ = writeln!(out, "{:<15}{} MiB", "memory", caps.memory_mib);
    let _ = writeln!(out, "{:<15}{}", "cpus", caps.cpus);
    match &caps.gles {
        Some(g) => {
            let _ = writeln!(
                out,
                "{:<15}{}.{}, {} ({})",
                "gles", g.version.0, g.version.1, g.renderer, g.vendor
            );
        }
        None => {
            let _ = writeln!(
                out,
                "{:<15}not detected ({})",
                "gles",
                caps.gles_error.as_deref().unwrap_or("not probed")
            );
        }
    }
    let _ = writeln!(out, "{:<15}{:?}", "virtualisation", caps.virtualisation);
}

/// The firmware drivers asked for and did not find: the driver and the file,
/// and not the kernel log they were read from.
fn missing_firmware(out: &mut String) {
    match firmware::missing_so_far() {
        None => out.push_str(
            "not read: the kernel log is root's here, and `doas alpymist report` has it\n",
        ),
        Some(missing) if missing.is_empty() => out.push_str("none asked for and not found\n"),
        Some(missing) => {
            // A driver asks again for each device and at each try.
            let mut seen = Vec::new();
            for m in missing {
                let line = format!("{:<15}{}", m.driver, m.file);
                if !seen.contains(&line) {
                    let _ = writeln!(out, "{line}");
                    seen.push(line);
                }
            }
        }
    }
}

/// The whole report.
fn compose() -> String {
    let mut out = format!(
        "Alpymist hardware report\n\n## Alpymist\n{:<15}{}\n",
        "version",
        env!("CARGO_PKG_VERSION")
    );
    out.push_str("\n## Hyprland\n");
    hyprland(&mut out);
    out.push_str(&alpymist_hwprobe::report::hardware(
        &alpymist_hwprobe::report::Roots::default(),
    ));
    out.push_str("\n## Firmware not found: driver, file\n");
    missing_firmware(&mut out);
    out
}

/// `text` as it goes into the query of an address: everything but letters,
/// digits and `-._~` as `%XX`.
fn percent(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// What the issue is called: the machine's make and model, if it says.
fn title(report: &str) -> String {
    let field = |name: &str| {
        report
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    match (field("vendor"), field("product")) {
        (Some(vendor), Some(product)) => format!("Hardware report: {vendor} {product}"),
        (_, Some(one)) | (Some(one), _) => format!("Hardware report: {one}"),
        (None, None) => "Hardware report".to_string(),
    }
}

/// The address of a new issue holding `report`, or asking for it to be pasted
/// from `saved` where the report is too long for an address.
fn issue_url(report: &str, saved: Option<&Path>) -> String {
    let asked = "What does not work on this machine:\n\n\n";
    let body = match saved {
        // The file's name and not its path, which has the account's name in it.
        Some(file) => format!(
            "{asked}(Paste the report here: it is in {} and was too long to fill in.)\n",
            file.file_name().unwrap_or(file.as_os_str()).display()
        ),
        None => format!("{asked}```\n{report}```\n"),
    };
    format!(
        "{ISSUES}?title={}&body={}",
        percent(&title(report)),
        percent(&body)
    )
}

/// Ask on the terminal, and take only a yes for one.
fn agreed(question: &str) -> Result<bool, Box<dyn Error>> {
    if !std::io::stdin().is_terminal() {
        return Err(
            "--issue asks before it opens anything, and there is no terminal to ask on".into(),
        );
    }
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

/// `alpymist report`.
pub fn run(save: Option<&Path>, issue: bool) -> Result<(), Box<dyn Error>> {
    let report = compose();
    if let Some(file) = save {
        std::fs::write(file, &report).map_err(|e| format!("{}: {e}", file.display()))?;
        eprintln!("Saved to {}. Nothing was sent.", file.display());
    }
    // Shown before anything is offered, whatever else was asked for.
    if save.is_none() || issue {
        print!("{report}");
    }
    if !issue {
        if save.is_none() {
            eprintln!(
                "\nNothing was sent. `alpymist report --issue` offers to open this as a \
                 GitHub issue; `--save FILE` writes it to a file."
            );
        }
        return Ok(());
    }

    eprintln!();
    if !agreed(
        "Open a new GitHub issue holding this report in the browser? \
         It is public once you submit it there.",
    )? {
        eprintln!("Nothing was opened, and nothing was sent.");
        return Ok(());
    }
    let mut url = issue_url(&report, None);
    if url.len() > LONGEST_URL {
        let file = save.map_or_else(|| PathBuf::from(FALLBACK), Path::to_path_buf);
        if save.is_none() {
            std::fs::write(&file, &report).map_err(|e| format!("{}: {e}", file.display()))?;
        }
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        eprintln!(
            "The report is too long to fill in: it is in {}, to paste into the issue.",
            file.display()
        );
        url = issue_url(&report, Some(&file));
    }
    for argv in alpymist_settings::default_apps::open("browser", &[url])? {
        launch::spawn(&argv).map_err(|e| format!("{}: {e}", argv[0]))?;
    }
    eprintln!("Opened in the browser. Nothing is posted until you submit it there.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ISSUES, issue_url, percent, title};
    use std::path::Path;

    #[test]
    fn an_address_keeps_only_what_needs_no_escaping() {
        assert_eq!(percent("a-b_c.d~9"), "a-b_c.d~9");
        assert_eq!(percent("a b&c=d\n#"), "a%20b%26c%3Dd%0A%23");
        assert_eq!(percent("å"), "%C3%A5");
    }

    #[test]
    fn the_issue_is_named_after_the_machine() {
        let report = "## Machine\nvendor         Apple Inc.\nproduct        MacBookPro5,5\n";
        assert_eq!(title(report), "Hardware report: Apple Inc. MacBookPro5,5");
        assert_eq!(title("product   X1\n"), "Hardware report: X1");
        assert_eq!(title("nothing\n"), "Hardware report");
    }

    #[test]
    fn the_issue_holds_the_report_and_goes_to_this_repository() {
        let url = issue_url("vendor  QEMU\n", None);
        assert!(url.starts_with("https://github.com/bisand/alpymist/issues/new?title="));
        assert!(url.starts_with(ISSUES));
        assert!(url.contains("vendor%20%20QEMU%0A"), "{url}");
    }

    /// Too long for an address, the report stays in its file and the issue
    /// says where: the address must not carry it anyway.
    #[test]
    fn a_report_left_in_a_file_is_not_in_the_address() {
        let url = issue_url("vendor  QEMU\n", Some(Path::new("/home/a/report.txt")));
        assert!(!url.contains("QEMU%0A"), "{url}");
        assert!(url.contains("it%20is%20in%20report.txt"), "{url}");
        assert!(!url.contains("home"), "the path names the account: {url}");
    }
}

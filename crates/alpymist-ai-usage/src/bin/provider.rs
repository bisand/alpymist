//! `alpymist-ai-provider` — the providers Alpymist ships, by name.
//!
//! ```text
//! alpymist-ai-provider openrouter      what this `OpenRouter` key has left
//! alpymist-ai-provider anthropic-api   what the Anthropic API cost this month
//! alpymist-ai-provider openai-api      what the `OpenAI` API cost this month
//! alpymist-ai-provider claude          a Claude subscription's limits
//! ```
//!
//! Each reads its keys as JSON on its standard input, prints a report as
//! JSON, and says why not on its standard error. `alpymist-ai-usage` runs
//! them as the files in `/usr/share/alpymist/ai-usage` say; so can anyone,
//! by hand, to see what a provider answers.
//!
//! The Claude provider has three more, which Claude Code and
//! `alpymist-ai-usage enable` run: `claude-statusline`, `claude-enable` and
//! `claude-disable`.

#![forbid(unsafe_code)]

use alpymist_ai_usage::providers::{anthropic, claude, openai, openrouter};
use alpymist_ai_usage::report::Report;
use alpymist_ai_usage::run::Input;
use alpymist_ai_usage::when;
use std::io::{Read as _, Write as _};
use std::process::{Command, ExitCode, Stdio};

const USAGE: &str = "\
usage: alpymist-ai-provider openrouter | anthropic-api | openai-api | claude

Reads {\"credentials\": {…}} on standard input and prints a report as JSON.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("openrouter") => print(openrouter::fetch(&Input::read())),
        Some("anthropic-api") => print(anthropic::fetch(&Input::read())),
        Some("openai-api") => print(openai::fetch(&Input::read())),
        Some("claude") => print(claude_report()),
        Some("claude-statusline") => {
            statusline();
            Ok(())
        }
        Some("claude-enable") => claude_enable(),
        Some("claude-disable") => claude_disable(),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

fn print(report: Result<Report, String>) -> Result<(), String> {
    let report = report?;
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn claude_report() -> Result<Report, String> {
    let kept = claude::kept_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .ok_or("Claude Code has not said yet: its limits show after it is next used")?;
    claude::report(&kept, when::now())
}

/// Claude Code's status line command: keep the limits, then be the status
/// line that was there before. Never fails, and never prints anything of its
/// own: a status line that breaks is worse than a figure that is late.
fn statusline() {
    let mut document = String::new();
    let _ = std::io::stdin().read_to_string(&mut document);
    if let (Some(kept), Some(path)) = (claude::capture(&document, when::now()), claude::kept_path())
    {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.new");
        if std::fs::write(&tmp, kept).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    let Some(command) = claude::before_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|before| claude::before_command(&before))
    else {
        return;
    };
    let Ok(mut child) = Command::new("sh")
        .args(["-c", &command])
        .stdin(Stdio::piped())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(document.as_bytes());
    }
    let _ = child.wait();
}

fn claude_enable() -> Result<(), String> {
    let path = claude::settings_path().ok_or("no home directory")?;
    let settings = std::fs::read_to_string(&path).ok();
    let (text, before) = claude::enable(settings.as_deref())?;
    if let (Some(before), Some(keep)) = (before, claude::before_path()) {
        if let Some(dir) = keep.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&keep, before.to_string())
            .map_err(|e| format!("{}: {e}", keep.display()))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "Claude Code's status line now keeps its limits for the bar ({}).",
        path.display()
    );
    Ok(())
}

fn claude_disable() -> Result<(), String> {
    let path = claude::settings_path().ok_or("no home directory")?;
    let Ok(settings) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let keep = claude::before_path();
    let before = keep
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok());
    let text = claude::disable(Some(&settings), before)?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(keep) = keep {
        let _ = std::fs::remove_file(keep);
    }
    Ok(())
}

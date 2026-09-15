//! `alpymist-store` — software from Flathub and Alpine, in a window or from
//! the command line.
//!
//! ```text
//! alpymist-store                        open the store, or bring it forward
//! alpymist-store installed | updates     open the store at what is installed, or updates
//! alpymist-store show SOURCE ID         open the store at an entry's page
//! alpymist-store open FILE.flatpakref   open the store at what the file names
//! alpymist-store search WORDS…          print what a search finds
//! alpymist-store install SOURCE ID      install, printing progress
//! alpymist-store remove SOURCE ID       remove
//! alpymist-store update [SOURCE]        update everything, or one source's
//! alpymist-store refresh [SOURCE]       fetch the catalogues again
//! alpymist-store prepare                add missing remotes, quietly; for login
//! alpymist-store --print-config         print the built-in configuration
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod app;

use alpymist_store::catalog::{Catalog, human_size};
use alpymist_store::config::{self, Config};
use alpymist_store::search::{self, Query};
use alpymist_store::source::{self, Op};
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "\
usage: alpymist-store [COMMAND]

With no command, opens the store, or brings the open one forward.
  installed, updates    open the store at what is installed, or has updates
  show SOURCE ID        open the store at an entry's page
  open FILE             open the store at what a .flatpakref file names
  search WORDS...       print what a search finds
  install SOURCE ID     install an entry, printing progress
  remove SOURCE ID      remove an entry
  update [SOURCE]       update everything, or everything from one source
  refresh [SOURCE]      fetch the catalogues again
  prepare               add missing Flatpak remotes; run at login
  --config FILE         use FILE instead of store.toml
  --print-config        print the built-in configuration

SOURCE is an id from store.toml: flathub or alpine as shipped.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let loaded = match take_option(&mut args, "--config") {
        Some(path) => config::load_from([std::path::PathBuf::from(path)]),
        None => config::load(),
    };
    for problem in &loaded.problems {
        eprintln!("alpymist-store: {problem}");
    }
    let config = loaded.config;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["-h" | "--help"] => {
            println!("{USAGE}");
            Ok(())
        }
        ["-V" | "--version"] => {
            println!("alpymist-store {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["--print-config"] => {
            print!("{}", config::DEFAULT);
            Ok(())
        }
        ["search", query @ ..] if !query.is_empty() => {
            search(&config, &query.join(" "));
            Ok(())
        }
        ["install", source, id] => operate(&config, source, &Op::Install((*id).to_owned())),
        ["remove", source, id] => operate(&config, source, &Op::Remove((*id).to_owned())),
        ["update"] => every(&config, &Op::Update(None)),
        ["update", source] => operate(&config, source, &Op::Update(None)),
        ["refresh"] => every(&config, &Op::Refresh),
        ["refresh", source] => operate(&config, source, &Op::Refresh),
        ["prepare"] => {
            prepare(&config);
            Ok(())
        }
        [] => window(&config, loaded.problems, None),
        [view @ ("installed" | "updates")] => {
            window(&config, loaded.problems, Some(format!("view {view}")))
        }
        ["show", source, id] => window(
            &config,
            loaded.problems,
            Some(format!("show {source} {id}")),
        ),
        ["open", file] => match flatpakref(file) {
            Ok(id) => {
                let source = first_flatpak(&config).unwrap_or_else(|| "flathub".into());
                window(
                    &config,
                    loaded.problems,
                    Some(format!("show {source} {id}")),
                )
            }
            Err(e) => Err(e),
        },
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-store: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Remove `--name VALUE` from `args`, returning the value.
fn take_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    if at + 1 >= args.len() {
        args.remove(at);
        return None;
    }
    let value = args.remove(at + 1);
    args.remove(at);
    Some(value)
}

fn source_index(config: &Config, id: &str) -> Result<usize, String> {
    config
        .sources
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| {
            let ids: Vec<&str> = config.sources.iter().map(|s| s.id.as_str()).collect();
            format!(
                "there is no source called {id}; there is {}",
                ids.join(", ")
            )
        })
}

fn first_flatpak(config: &Config) -> Option<String> {
    config
        .sources
        .iter()
        .find(|s| matches!(s.kind, config::Kind::Flatpak(_)))
        .map(|s| s.id.clone())
}

/// The application a `.flatpakref` file names: its `Name=` line.
fn flatpakref(path: &str) -> Result<String, String> {
    let path = path.strip_prefix("file://").unwrap_or(path);
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .find_map(|l| l.strip_prefix("Name="))
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("{path}: names no application"))
}

fn search(config: &Config, query: &str) {
    let started = Instant::now();
    let mut catalog = Catalog::new(config.sources.len());
    for (i, src) in config.sources.iter().enumerate() {
        let s = u16::try_from(i).unwrap_or(u16::MAX);
        let loaded = Instant::now();
        let source = source::open(src);
        match source.load() {
            Ok(entries) => {
                eprintln!(
                    "{}: {} entries in {} ms",
                    src.label,
                    entries.len(),
                    loaded.elapsed().as_millis()
                );
                catalog.replace(s, entries);
            }
            Err(e) => eprintln!("{}: {e}", src.label),
        }
        if let Ok(installed) = source.installed() {
            catalog.set_installed(s, &installed);
        }
    }
    let searched = Instant::now();
    let all: Vec<_> = catalog.iter().map(|(at, _)| at).collect();
    let results = search::rank(&catalog, &Query::new(query), all.into_iter());
    eprintln!(
        "{} results in {} µs, {} ms in all",
        results.len(),
        searched.elapsed().as_micros(),
        started.elapsed().as_millis()
    );
    for at in results.iter().take(20) {
        let Some(e) = catalog.get(*at) else { continue };
        let label = &config.sources[usize::from(at.source)].label;
        let mark = if e.state.has_update() {
            "↑"
        } else if e.state.installed() {
            "✓"
        } else {
            " "
        };
        let size = e.download_size.map(human_size).unwrap_or_default();
        println!(
            "{mark} {:<8} {:<34} {:<16} {:>8}  {}",
            label, e.id, e.version, size, e.summary
        );
    }
}

fn operate(config: &Config, source_id: &str, op: &Op) -> Result<(), String> {
    let index = source_index(config, source_id)?;
    let source = source::open(&config.sources[index]);
    source.prepare()?;
    source.run(op, &mut |line| println!("{line}"))
}

fn every(config: &Config, op: &Op) -> Result<(), String> {
    let mut failed = Vec::new();
    for src in &config.sources {
        println!("{}:", src.label);
        let source = source::open(src);
        if let Err(e) = source
            .prepare()
            .and_then(|()| source.run(op, &mut |line| println!("  {line}")))
        {
            eprintln!("  {e}");
            failed.push(src.label.clone());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("failed: {}", failed.join(", ")))
    }
}

/// Add missing remotes, saying nothing unless something went wrong: run at
/// every login, and quick when there is nothing to do.
fn prepare(config: &Config) {
    for src in &config.sources {
        if let Err(e) = source::open(src).prepare() {
            eprintln!("alpymist-store: {}: {e}", src.label);
        }
    }
}

#[cfg(target_os = "linux")]
fn window(config: &Config, problems: Vec<String>, message: Option<String>) -> Result<(), String> {
    app::run(config, problems, message)
}

#[cfg(not(target_os = "linux"))]
fn window(_: &Config, _: Vec<String>, _: Option<String>) -> Result<(), String> {
    Err("the window needs Wayland; try `alpymist-store search`".into())
}

#[cfg(test)]
mod tests {
    use super::take_option;

    #[test]
    fn options_come_out_of_the_arguments() {
        let mut args: Vec<String> = ["search", "--config", "x.toml", "gimp"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            take_option(&mut args, "--config").as_deref(),
            Some("x.toml")
        );
        assert_eq!(args, ["search", "gimp"]);
    }
}

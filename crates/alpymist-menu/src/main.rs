//! `alpymist-menu` — open the Alpymist menu.
//!
//! ```text
//! alpymist-menu               the whole menu
//! alpymist-menu apps          straight to the applications
//! alpymist-menu system        straight to any [menu.NAME]
//! alpymist-menu --check       say whether the configuration parses
//! alpymist-menu --print-config  print the built-in configuration
//! ```
//!
//! Running it while it is open closes it, so one key both opens and dismisses
//! the menu. Set `ALPYMIST_MENU_TRACE=1` to have it report how long each stage
//! of starting up took.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod host;

use alpymist_menu::config::{self, Config};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "\
usage: alpymist-menu [MENU]
       alpymist-menu --check [FILE]
       alpymist-menu --print-config

Opens the Alpymist menu, or the submenu called MENU. Run again to close it.
The menu is read from ~/.config/alpymist/menu.toml, else /etc/alpymist/menu.toml,
else the built-in copy that --print-config shows.";

fn main() -> ExitCode {
    let trace = std::env::var_os("ALPYMIST_MENU_TRACE").map(|_| Instant::now());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("alpymist-menu {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--print-config") => {
            print!("{}", config::DEFAULT);
            ExitCode::SUCCESS
        }
        Some("--check") => check(args.get(1).map(PathBuf::from)),
        Some(flag) if flag.starts_with('-') => {
            eprintln!("alpymist-menu: unknown option {flag}\n\n{USAGE}");
            ExitCode::from(2)
        }
        start => open(start.unwrap_or(config::ROOT), trace),
    }
}

/// `--check`: parse a file and say what is wrong with it.
fn check(path: Option<PathBuf>) -> ExitCode {
    let Some(path) = path.or_else(|| {
        config::user_path()
            .filter(|p| p.exists())
            .or_else(|| Some(PathBuf::from(config::SYSTEM_PATH)).filter(|p| p.exists()))
    }) else {
        println!("no configuration file; the built-in menu is in use");
        return ExitCode::SUCCESS;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match Config::parse(&text) {
        Ok(config) => {
            let entries: usize = config.menu.values().map(|m| m.items.len()).sum();
            println!(
                "{}: fine — {} menus, {entries} entries",
                path.display(),
                config.menu.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

/// Where a running menu listens for another invocation.
fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    Some(PathBuf::from(dir).join(format!("alpymist-menu-{display}.sock")))
}

/// Either become the running menu, or tell the running one to close.
///
/// `Err(())` means another menu was open and has been asked to close.
fn claim(path: Option<&PathBuf>) -> Result<Option<UnixListener>, ()> {
    let Some(path) = path else {
        return Ok(None);
    };
    if UnixStream::connect(path).is_ok() {
        // Connecting is the whole message.
        return Err(());
    }
    // Nobody answered: whatever is there is left over from a menu that did
    // not exit cleanly.
    std::fs::remove_file(path).ok();
    Ok(UnixListener::bind(path).ok())
}

#[cfg(target_os = "linux")]
fn stage(trace: Option<Instant>, what: &str) {
    if let Some(start) = trace {
        eprintln!("trace: {what} at {:?}", start.elapsed());
    }
}

fn open(start: &str, trace: Option<Instant>) -> ExitCode {
    let socket = socket_path();
    let Ok(listener) = claim(socket.as_ref()) else {
        return ExitCode::SUCCESS;
    };
    let result = run(start, trace, listener);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result
}

#[cfg(target_os = "linux")]
fn run(start: &str, trace: Option<Instant>, listener: Option<UnixListener>) -> ExitCode {
    use alpymist_menu::history::History;
    use alpymist_menu::menu::Menu;
    use alpymist_menu::tree::{Action, Tree};
    use alpymist_menu::{apps, exec, view};

    let loaded = config::load();
    for problem in &loaded.problems {
        eprintln!("alpymist-menu: {problem}");
    }
    stage(trace, "configuration read");

    // The surface first: the compositor answers while the rest loads.
    let pending = match host::connect(loaded.config.appearance.clone(), trace) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("alpymist-menu: {e}");
            return ExitCode::FAILURE;
        }
    };
    stage(trace, "surface requested");

    let fonts = view::Fonts::load(&loaded.config.appearance);
    for problem in &fonts.problems {
        eprintln!("alpymist-menu: font {problem}");
    }
    stage(trace, "fonts loaded");
    let found = apps::discover();
    stage(trace, &format!("{} applications found", found.len()));

    let history_path = History::path();
    let history = History::load(history_path.as_deref());
    let tree = Tree::build(&loaded.config, found);
    let Some(start_id) = tree.find(start) else {
        let names: Vec<&str> = tree.menus.iter().map(|m| m.name.as_str()).collect();
        eprintln!(
            "alpymist-menu: there is no menu called {start}; there are: {}",
            names.join(", ")
        );
        return ExitCode::from(2);
    };
    let rows = usize::try_from(loaded.config.appearance.rows).unwrap_or(9);
    let menu = Menu::new(tree, history, start_id, rows.max(1));
    let terminal = loaded
        .config
        .terminal
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(exec::default_terminal);
    let notice = loaded.problems.first().cloned();
    stage(trace, "menu built");

    let (chosen, mut menu) = match pending.run(menu, fonts, notice, listener) {
        Ok(done) => done,
        Err(e) => {
            eprintln!("alpymist-menu: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(entry) = chosen else {
        return ExitCode::SUCCESS;
    };
    let Action::Run(launch) = menu.tree.entries[entry].action.clone() else {
        return ExitCode::SUCCESS;
    };
    if let Err(e) = launch.spawn(&terminal) {
        eprintln!(
            "alpymist-menu: could not start {}: {e}",
            menu.tree.entries[entry].name
        );
        return ExitCode::FAILURE;
    }
    menu.record_launch(entry);
    if let Some(path) = history_path {
        menu.history().save(&path);
    }
    ExitCode::SUCCESS
}

#[cfg(not(target_os = "linux"))]
fn run(_: &str, _: Option<Instant>, _: Option<UnixListener>) -> ExitCode {
    eprintln!(
        "alpymist-menu draws on a Wayland layer surface, which needs Linux.\n\
         To see it here: cargo run -p alpymist-menu --example snapshot -- out/"
    );
    ExitCode::FAILURE
}

//! `alpymist-ai-usage` — how much of an AI subscription or API budget is
//! left, from the bar or the command line.
//!
//! ```text
//! alpymist-ai-usage                 what every provider turned on says
//! alpymist-ai-usage list            the providers there are, and which are on
//! alpymist-ai-usage enable ID       turn one on: its tool, its key, its setup
//! alpymist-ai-usage disable ID      turn it off
//! alpymist-ai-usage key ID [NAME]   give it a key, kept in the keyring
//! alpymist-ai-usage forget ID       forget its keys and what it last said
//! alpymist-ai-usage refresh         ask every one again, now
//! alpymist-ai-usage --waybar        a line of JSON for Waybar at every change
//! ```

#![forbid(unsafe_code)]

use alpymist_ai_usage::bar::{self, Entry};
use alpymist_ai_usage::config::Config;
use alpymist_ai_usage::definition::{self, Credential, Definition};
use alpymist_ai_usage::store::{self, Kept};
use alpymist_ai_usage::{run, secrets, when};
use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "\
usage: alpymist-ai-usage [status | list | enable ID | disable ID | key ID [NAME]
                          | forget ID | refresh | --waybar]

With no command, says what every provider turned on last reported.
  list         the providers there are, and which are turned on
  enable ID    turn one on: install its vendor's tool if it needs one, ask
               for its key, and do its setup
  disable ID   turn it off; its keys stay in the keyring
  key ID       give a provider its key, typed unseen and kept in the keyring
  forget ID    forget a provider's keys and what it last said
  refresh      ask every provider turned on again, now
  --waybar     print a line of JSON for a Waybar custom module at every change";

/// How often the bar looks: whether anything is due, and whether what is
/// kept changed under it, as after `refresh` or `enable` elsewhere.
const LOOK_EVERY: Duration = Duration::from_secs(20);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [] | ["status"] => {
            status();
            Ok(())
        }
        ["list"] => {
            list();
            Ok(())
        }
        ["enable", id] => enable(id),
        ["disable", id] => disable(id),
        ["key", id] => key(id, None),
        ["key", id, name] => key(id, Some(name)),
        ["forget", id] => forget(id),
        ["refresh"] => {
            refresh_all(true);
            status();
            Ok(())
        }
        ["--waybar"] => {
            waybar();
            Ok(())
        }
        ["-h" | "--help"] => {
            println!("{USAGE}");
            Ok(())
        }
        ["-V" | "--version"] => {
            println!("alpymist-ai-usage {}", env!("CARGO_PKG_VERSION"));
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
            eprintln!("alpymist-ai-usage: {why}");
            ExitCode::FAILURE
        }
    }
}

/// The providers turned on that are installed, in the order they are listed.
fn turned_on(config: &Config) -> Vec<Definition> {
    definition::discover()
        .into_iter()
        .filter(|d| config.is_enabled(&d.id))
        .collect()
}

fn find(id: &str) -> Result<Definition, String> {
    definition::discover()
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("there is no provider called {id}: see alpymist-ai-usage list"))
}

/// Ask what is due, or everything when `force`d, unless this is no time to.
fn refresh_all(force: bool) -> Vec<(Definition, Kept)> {
    let config = Config::load();
    let quiet = if force { None } else { run::quiet(&config) };
    let now = when::now();
    turned_on(&config)
        .into_iter()
        .map(|def| {
            let kept = if quiet.is_some() {
                store::load(&def.id)
            } else {
                run::refresh(&def, &config, now, force)
            };
            (def, kept)
        })
        .collect()
}

fn entries<'a>(kept: &'a [(Definition, Kept)], config: &Config) -> Vec<Entry<'a>> {
    kept.iter()
        .map(|(definition, kept)| Entry {
            definition,
            kept,
            every: run::every(definition, config),
        })
        .collect()
}

fn status() {
    let config = Config::load();
    let kept: Vec<(Definition, Kept)> = turned_on(&config)
        .into_iter()
        .map(|def| {
            let kept = store::load(&def.id);
            (def, kept)
        })
        .collect();
    println!("{}", bar::summary(&entries(&kept, &config), when::now()));
}

fn list() {
    let config = Config::load();
    let found = definition::discover();
    if found.is_empty() {
        println!("No provider is installed.");
        return;
    }
    for def in found {
        let state = if config.is_enabled(&def.id) {
            "on "
        } else {
            "off"
        };
        let note = if def.unofficial {
            " (undocumented by its vendor: it may stop working)"
        } else {
            ""
        };
        println!(
            "{state}  {:<14} {}: {}{note}",
            def.id, def.name, def.description
        );
    }
}

/// The bar's line, now and whenever it changes.
fn waybar() {
    alpymist_widget::waybar::follow(
        || {
            let config = Config::load();
            let kept = refresh_all(false);
            bar::waybar(&entries(&kept, &config), &config, when::now())
        },
        || std::thread::sleep(LOOK_EVERY),
    );
}

/// Whether `program` is there to run: on `PATH`, or where vendors' own
/// installers put things for one account.
fn installed(program: &str) -> bool {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs.iter().any(|dir| dir.join(program).is_file())
}

/// A yes or no, asked at the terminal. No is the answer to anything else.
fn agreed(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().lock().read_line(&mut answer);
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes")
}

/// A secret typed at the terminal without showing, or read from a pipe.
fn secret(prompt: &str) -> Result<String, String> {
    let stdin = std::io::stdin();
    let shown = stdin.is_terminal();
    if shown {
        eprint!("{prompt}: ");
        let _ = Command::new("stty").arg("-echo").status();
    }
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if shown {
        let _ = Command::new("stty").arg("echo").status();
        eprintln!();
    }
    read.map_err(|e| e.to_string())?;
    let line = line.trim().to_owned();
    if line.is_empty() {
        return Err("nothing was typed".into());
    }
    Ok(line)
}

fn ask_for(def: &Definition, credential: &Credential) -> Result<(), String> {
    if !credential.description.is_empty() {
        eprintln!("{}", credential.description);
    }
    let typed = secret(&format!("{} for {}", credential.title, def.name))?;
    secrets::store(
        &def.id,
        &credential.key,
        &format!("{} {}", def.name, credential.title),
        &typed,
    )
}

fn key(id: &str, name: Option<&str>) -> Result<(), String> {
    let def = find(id)?;
    let credential = match name {
        Some(name) => def.credentials.iter().find(|c| c.key == name),
        None => def.credentials.first(),
    }
    .ok_or_else(|| match name {
        Some(name) => format!("{} has no credential called {name}", def.name),
        None => format!("{} needs no key", def.name),
    })?;
    ask_for(&def, credential)?;
    println!("Kept in the keyring.");
    Ok(())
}

fn enable(id: &str) -> Result<(), String> {
    let def = find(id)?;
    if def.unofficial {
        println!(
            "{} is read in a way its vendor does not document. It works today and \
             may stop without notice.",
            def.name
        );
    }
    // The vendor's own tool, the vendor's own way, where it can be watched.
    if let Some(requires) = def.requires.as_ref().filter(|r| !installed(&r.program)) {
        println!(
            "{} is read through {}, which is not installed.\n\
             Its vendor's installer is:\n\n    {}\n",
            def.name, requires.about, requires.install
        );
        if !std::io::stdin().is_terminal() {
            return Err(format!(
                "run this in a terminal to install it: alpymist-ai-usage enable {id}"
            ));
        }
        if !agreed("Run it now?") {
            return Err(format!("{} was not turned on", def.name));
        }
        let status = Command::new("sh")
            .args(["-c", &requires.install])
            .status()
            .map_err(|e| format!("sh: {e}"))?;
        if !status.success() || !installed(&requires.program) {
            return Err(format!(
                "{} did not install; {} was not turned on",
                requires.program, def.name
            ));
        }
    }
    for credential in &def.credentials {
        let has = secrets::lookup(&def.id, &credential.key).is_some();
        if has || (credential.optional && !std::io::stdin().is_terminal()) {
            continue;
        }
        if credential.optional
            && !agreed(&format!("Give a {} too? It is optional.", credential.title))
        {
            continue;
        }
        ask_for(&def, credential)?;
    }
    if let Some(setup) = &def.enable {
        hook(setup)?;
    }
    let mut config = Config::load();
    config.set_enabled(&def.id, true);
    config.save()?;
    let kept = run::refresh(&def, &config, when::now(), true);
    let entry = [Entry {
        definition: &def,
        kept: &kept,
        every: run::every(&def, &config),
    }];
    println!("{}", bar::summary(&entry, when::now()));
    Ok(())
}

fn disable(id: &str) -> Result<(), String> {
    let mut config = Config::load();
    // Its file may be gone with its package; it is turned off all the same.
    if let Ok(def) = find(id)
        && let Some(undo) = &def.disable
    {
        hook(undo)?;
    }
    if config.set_enabled(id, false) {
        config.save()?;
    }
    Ok(())
}

fn forget(id: &str) -> Result<(), String> {
    disable(id)?;
    if let Ok(def) = find(id) {
        for credential in &def.credentials {
            secrets::clear(&def.id, &credential.key);
        }
    }
    if let Some(path) = store::path(id) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Run a provider's setup or its undoing: a program and its arguments.
fn hook(line: &str) -> Result<(), String> {
    let mut words = line.split_whitespace();
    let program = words.next().ok_or("an empty setup command")?;
    match Command::new(program).args(words).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("{program} failed")),
        Err(e) => Err(format!("{program}: {e}")),
    }
}

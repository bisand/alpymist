//! `alpymist-ai-usage` — how much of an AI subscription or API budget is
//! left, from the bar or the command line.
//!
//! ```text
//! alpymist-ai-usage                 open the popup (run again to close it)
//! alpymist-ai-usage setup ID        open the popup and turn one on there
//! alpymist-ai-usage status          what every provider turned on says
//! alpymist-ai-usage list            the providers there are, and which are on
//! alpymist-ai-usage enable ID       turn one on: its tool, its key, its setup
//! alpymist-ai-usage disable ID      turn it off
//! alpymist-ai-usage key ID [NAME]   give it a key, kept in the keyring
//! alpymist-ai-usage forget ID       forget its keys and what it last said
//! alpymist-ai-usage tool run ID     start a provider's vendor's tool
//! alpymist-ai-usage tool install ID install it, or update it
//! alpymist-ai-usage tool remove ID  take it away again
//! alpymist-ai-usage refresh         ask every one again, now
//! alpymist-ai-usage --waybar        a line of JSON for Waybar at every change
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod widget;

use alpymist_ai_usage::bar::{self, Entry};
use alpymist_ai_usage::config::Config;
use alpymist_ai_usage::definition::{self, Credential, Definition};
use alpymist_ai_usage::store::{self, Kept};
use alpymist_ai_usage::{run, secrets, when};
use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "\
usage: alpymist-ai-usage [setup ID | status | list | enable ID | disable ID
                          | key ID [NAME] | forget ID | refresh | --waybar]

With no command, opens the popup under the bar; run it again to close it.
  setup ID     open the popup and turn a provider on there: its key is typed
               in the popup, and its vendor's tool installed in a terminal
  status       what every provider turned on last reported
  list         the providers there are, and which are turned on
  enable ID    turn one on: install its vendor's tool if it needs one, ask
               for its key, and do its setup
  disable ID   turn it off; its keys stay in the keyring
  key ID       give a provider its key, typed unseen and kept in the keyring
  forget ID    forget a provider's keys and what it last said
  refresh      ask every provider turned on again, now
  tool run ID      start the vendor's tool a provider is read through, such
                   as Claude Code for claude, offering to install it first
  tool install ID  install that tool its vendor's way, or update it
  tool remove ID   take it away again; its login and settings stay
  --waybar     print a line of JSON for a Waybar custom module at every change";

/// The popup's name: its socket, and its layer surface's namespace.
const NAME: &str = "alpymist-ai-usage";

/// How often the bar looks: whether anything is due, and whether what is
/// kept changed under it, as after `refresh` or `enable` elsewhere.
const LOOK_EVERY: Duration = Duration::from_secs(20);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [] => alpymist_widget::instance::toggle(NAME, |listener| open(listener, None)),
        ["setup", id] => {
            find(id).and_then(|_| alpymist_widget::instance::toggle(NAME, |l| open(l, Some(id))))
        }
        ["status"] => {
            status();
            Ok(())
        }
        ["list"] => {
            list();
            Ok(())
        }
        ["enable", id] => enable(id, std::io::stdin().is_terminal()),
        ["disable", id] => disable(id),
        ["key", id] => key(id, None),
        ["key", id, name] => key(id, Some(name)),
        ["forget", id] => forget(id),
        ["tool", "run", id] => tool_run(id),
        ["tool", "install", id] => tool_install(id),
        ["tool", "remove", id] => tool_remove(id),
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

/// What is kept for every provider turned on, without asking any.
fn kept(config: &Config) -> Vec<(Definition, Kept)> {
    turned_on(config)
        .into_iter()
        .map(|def| {
            let kept = store::load(&def.id);
            (def, kept)
        })
        .collect()
}

fn status() {
    let config = Config::load();
    let kept = kept(&config);
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

/// What the popup's watcher and worker do.
#[cfg(target_os = "linux")]
mod work {
    use super::{Config, Definition, Kept, definition, disable, enable, find, installed};
    use super::{run, secrets, store, when};
    use alpymist_ai_usage::popup::{Reading, Reply};

    pub fn reading() -> Reading {
        let config = Config::load();
        let installed: Vec<(Definition, Kept)> = definition::discover()
            .into_iter()
            .map(|def| {
                let kept = store::load(&def.id);
                (def, kept)
            })
            .collect();
        Reading::of(&installed, &config, when::now(), |def| {
            run::every(def, &config)
        })
    }

    /// Turn `id` on, or say what it still needs: its vendor's tool, which
    /// is installed in a terminal, or a key, which the popup asks for.
    pub fn turn_on(id: &str) -> Reply {
        let def = match find(id) {
            Ok(def) => def,
            Err(why) => return Reply::Failed(why),
        };
        if def
            .requires
            .as_ref()
            .is_some_and(|r| !installed(&r.program))
        {
            return Reply::NeedsTool;
        }
        if !def.credentials.is_empty() {
            if let Some(why) = secrets::state().refusal() {
                return Reply::Failed(why.into());
            }
            let mut missing = Vec::new();
            for credential in def.credentials.iter().filter(|c| !c.optional) {
                match secrets::lookup(&def.id, &credential.key) {
                    Ok(Some(_)) => {}
                    Ok(None) => missing.push(credential.key.clone()),
                    Err(why) => return Reply::Failed(why),
                }
            }
            if !missing.is_empty() {
                return Reply::NeedsKeys(missing);
            }
        }
        done(enable(id, false))
    }

    pub fn keep(id: &str, key: &str, secret: &str) -> Result<(), String> {
        let def = find(id)?;
        let credential = def
            .credentials
            .iter()
            .find(|c| c.key == key)
            .ok_or_else(|| format!("{} has no credential called {key}", def.name))?;
        if let Some(why) = secrets::state().refusal() {
            return Err(why.into());
        }
        secrets::store(
            &def.id,
            &credential.key,
            &format!("{} {}", def.name, credential.title),
            secret,
        )
    }

    pub fn set(change: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut config = Config::load();
        change(&mut config);
        config.save()
    }

    pub fn done(result: Result<(), String>) -> Reply {
        result.map_or_else(Reply::Failed, |()| Reply::Done)
    }

    pub fn turn_off(id: &str) -> Reply {
        done(disable(id))
    }
}

#[cfg(target_os = "linux")]
fn open(listener: Option<UnixListener>, setting_up: Option<&str>) -> Result<bool, String> {
    use alpymist_ai_usage::popup::{Command, Popup, Reply};
    use widget::Event;
    use work::{done, keep, reading, set, turn_off, turn_on};

    /// How often the popup looks at what is kept: the bar does the asking,
    /// and the time until a limit resets moves on by itself.
    const LOOK_EVERY: Duration = Duration::from_secs(5);

    let appearance = alpymist_widget::appearance();
    let fonts = alpymist_ai_usage::view::Fonts::load(&appearance);
    for problem in &fonts.problems {
        eprintln!("alpymist-ai-usage: font {problem}");
    }
    let (events, channel) = alpymist_widget::host::events::<Event>();

    // The watcher: what is kept, now and every few seconds.
    {
        let events = events.clone();
        std::thread::spawn(move || {
            while events.send(Event::Reading(reading())).is_ok() {
                std::thread::sleep(LOOK_EVERY);
            }
        });
    }

    // The worker: one thing at a time, a fresh reading after each.
    let (commands, queue) = std::sync::mpsc::channel::<Command>();
    std::thread::spawn(move || {
        for command in queue {
            let reply = match &command {
                Command::Refresh => {
                    drop(refresh_all(true));
                    Reply::Done
                }
                Command::TurnOn(id) => turn_on(id),
                Command::TurnOff(id) => turn_off(id),
                Command::Keep { id, key, secret } => done(keep(id, key, secret)),
                Command::Notify(on) => done(set(|c| c.notify = *on)),
                Command::WarnAt(percent) => done(set(|c| c.warn_at = *percent)),
            };
            if events.send(Event::Reading(reading())).is_err()
                || events.send(Event::Done(command, reply)).is_err()
            {
                return;
            }
        }
    });

    alpymist_widget::host::run(
        widget::UsagePopup::new(
            appearance,
            fonts,
            setting_up.map_or_else(Popup::new, Popup::setting_up),
            commands,
        ),
        &alpymist_widget::host::Options::new(NAME),
        channel,
        listener,
    )
}

#[cfg(not(target_os = "linux"))]
fn open(_: Option<UnixListener>, _: Option<&str>) -> Result<bool, String> {
    Err(
        "the popup draws on a Wayland layer surface, which needs Linux.\n\
         To see it here: cargo run -p alpymist-ai-usage --example snapshot -- out/"
            .into(),
    )
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

/// Where `program` is: on `PATH`, or where vendors' own installers put
/// things for one account, which the desktop's `PATH` does not have.
fn tool_path(program: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(local_bin());
    dirs.into_iter()
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

/// `~/.local/bin`.
fn local_bin() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin"))
}

/// `PATH` with `~/.local/bin` in front: what a vendor's tool, and its
/// installer, expect to find themselves on.
fn path_with_local_bin() -> std::ffi::OsString {
    let known = std::env::var_os("PATH").unwrap_or_default();
    let dirs = local_bin().into_iter().chain(std::env::split_paths(&known));
    std::env::join_paths(dirs).unwrap_or(known)
}

/// Show a vendor's command line, ask, and run it where it can be watched.
/// `Ok(false)` is a no.
fn run_shown(line: &str) -> Result<bool, String> {
    println!("\n    {line}\n");
    if !agreed("Run it now?") {
        return Ok(false);
    }
    let status = Command::new("sh")
        .args(["-c", line])
        .env("PATH", path_with_local_bin())
        .status()
        .map_err(|e| format!("sh: {e}"))?;
    if status.success() {
        Ok(true)
    } else {
        Err(format!("it failed ({status})"))
    }
}

/// Install a provider's vendor's tool, its vendor's way.
fn install_tool(requires: &definition::Requires) -> Result<(), String> {
    println!("Its vendor's installer is:");
    match run_shown(&requires.install) {
        Ok(true) if installed(&requires.program) => Ok(()),
        Ok(true) | Err(_) => Err(format!("{} did not install", requires.program)),
        Ok(false) => Err(format!("{} was not installed", requires.program)),
    }
}

/// The provider `id`, and the vendor's tool it is read through.
fn tool_of(id: &str) -> Result<(Definition, definition::Requires), String> {
    let def = find(id)?;
    let requires = def
        .requires
        .clone()
        .ok_or_else(|| format!("{} is not read through a tool of its vendor's", def.name))?;
    Ok((def, requires))
}

/// Start a provider's vendor's tool here, in this terminal, offering to
/// install it first where it is not there.
fn tool_run(id: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let (_, requires) = tool_of(id)?;
    if !installed(&requires.program) {
        println!("{} is not installed.", requires.about);
        install_tool(&requires)?;
        println!();
    }
    let path = tool_path(&requires.program)
        .ok_or_else(|| format!("{} is not where it was installed", requires.program))?;
    let failed = Command::new(&path)
        .env("PATH", path_with_local_bin())
        .exec();
    Err(format!("{}: {failed}", path.display()))
}

/// Install a provider's vendor's tool, or run its installer again, which
/// is how each of them updates.
fn tool_install(id: &str) -> Result<(), String> {
    let (def, requires) = tool_of(id)?;
    if installed(&requires.program) {
        println!(
            "{} is installed. Its installer also updates it.",
            requires.about
        );
    }
    install_tool(&requires)?;
    println!(
        "\n{} is installed. To see its limits in the bar: alpymist-ai-usage enable {}",
        requires.about, def.id
    );
    Ok(())
}

/// Take a provider's vendor's tool away again, and turn the provider off,
/// since it cannot be read without it.
fn tool_remove(id: &str) -> Result<(), String> {
    let (def, requires) = tool_of(id)?;
    if !installed(&requires.program) {
        println!("{} is not installed.", requires.about);
        return Ok(());
    }
    let line = requires
        .remove
        .as_deref()
        .ok_or_else(|| format!("{} does not say how it is removed", def.name))?;
    println!(
        "This removes {}. Its login and its settings stay where it keeps them.",
        requires.about
    );
    match run_shown(line) {
        Ok(true) if !installed(&requires.program) => {}
        Ok(true) => return Err(format!("{} is still there", requires.program)),
        Ok(false) => return Err(format!("{} was not removed", requires.program)),
        Err(why) => return Err(why),
    }
    if Config::load().is_enabled(&def.id) {
        disable(&def.id)?;
        println!(
            "{} is turned off in the bar: it was read through it.",
            def.name
        );
    }
    println!("Removed.");
    Ok(())
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

/// Turn `id` on. When `asking`, someone is at a terminal to say yes to an
/// installer and to type a key; when not, what is missing is an error.
fn enable(id: &str, asking: bool) -> Result<(), String> {
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
            "{} is read through {}, which is not installed.",
            def.name, requires.about
        );
        if !asking {
            return Err(format!(
                "run this in a terminal to install it: alpymist-ai-usage enable {id}"
            ));
        }
        install_tool(requires).map_err(|why| format!("{why}; {} was not turned on", def.name))?;
    }
    if !def.credentials.is_empty()
        && let Some(why) = secrets::state().refusal()
    {
        return Err(why.into());
    }
    for credential in &def.credentials {
        let has = secrets::lookup(&def.id, &credential.key)?.is_some();
        if has || (credential.optional && !asking) {
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
            secrets::clear(&def.id, &credential.key)?;
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

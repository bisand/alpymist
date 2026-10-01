//! Asking a provider, and deciding when to.
//!
//! The provider's program is started with its credentials as JSON on its
//! standard input — `{"credentials": {"api-key": "…"}}` — and has a while to
//! print a [`Report`]. What it says, or why it could not, is kept
//! ([`crate::store`]) with the time, which paces the next asking: no more
//! often than the provider's file allows, nor than Settings asks for; not at
//! all while the screen is locked, or on a battery that is running low.

use crate::config::Config;
use crate::definition::Definition;
use crate::report::Report;
use crate::store::{self, Kept};
use crate::{notify, secrets};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a provider's program may take.
const PATIENCE: Duration = Duration::from_secs(45);

/// What a provider's program is handed on its standard input.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// Its secrets, by the keys its file declares. One not given is absent.
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
}

impl Input {
    /// Read it, in a provider's program. Nothing on the input is no
    /// credentials, which a provider run by hand gets.
    #[must_use]
    pub fn read() -> Self {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// A credential, or a sentence saying it is missing.
    ///
    /// # Errors
    /// It was not given.
    pub fn need(&self, key: &str) -> Result<&str, String> {
        self.credentials
            .get(key)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("no {key} was given"))
    }
}

/// The credentials `def` declares, from the keyring.
///
/// # Errors
/// One it cannot do without is not there, and how to give it.
pub fn credentials(def: &Definition) -> Result<Input, String> {
    let mut input = Input::default();
    if def.credentials.is_empty() {
        return Ok(input);
    }
    // Asked once, before any key is looked for: a keyring that is locked or
    // not there would otherwise look like a key never given, a few seconds
    // at a time.
    if let Some(why) = secrets::state().refusal() {
        return Err(why.into());
    }
    for credential in &def.credentials {
        match secrets::lookup(&def.id, &credential.key)? {
            Some(secret) => {
                input.credentials.insert(credential.key.clone(), secret);
            }
            None if credential.optional => {}
            None => {
                return Err(format!(
                    "No {} yet. Give it with: alpymist-ai-usage key {}",
                    credential.title, def.id
                ));
            }
        }
    }
    Ok(input)
}

/// Run `def`'s program with `input`, and read what it reports.
///
/// # Errors
/// It could not be run, took too long, failed, or printed no report: in its
/// own words where it had any.
pub fn ask(def: &Definition, input: &Input) -> Result<Report, String> {
    let line = def.argv();
    let (program, args) = line.split_first().ok_or("nothing to run")?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let text = serde_json::to_string(input).unwrap_or_default();
        // A program that does not read its input is not a failure yet.
        let _ = stdin.write_all(text.as_bytes());
    }
    let deadline = Instant::now() + PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it took too long to answer".into());
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        return Report::parse(&String::from_utf8_lossy(&output.stdout));
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    Err(if said.is_empty() {
        format!("{program} failed without saying why")
    } else {
        said.trim().to_owned()
    })
}

/// Why nothing should be asked just now, if anything: the screen is locked,
/// or the battery is low and nothing is charging it.
#[must_use]
pub fn quiet(config: &Config) -> Option<&'static str> {
    if locked(Path::new("/proc")) {
        return Some("the screen is locked");
    }
    low_battery(Path::new("/sys/class/power_supply"), config.battery_floor)
        .then_some("the battery is low")
}

/// Whether the lock screen is up: a process called `alpymist-lock`.
fn locked(proc: &Path) -> bool {
    std::fs::read_dir(proc).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
                && std::fs::read_to_string(entry.path().join("comm"))
                    .is_ok_and(|comm| comm.trim() == "alpymist-lock")
        })
    })
}

/// Whether a battery under `supplies` is discharging below `floor` percent.
fn low_battery(supplies: &Path, floor: u8) -> bool {
    std::fs::read_dir(supplies).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let read = |name: &str| std::fs::read_to_string(entry.path().join(name)).ok();
            read("type").is_some_and(|t| t.trim() == "Battery")
                && read("status").is_some_and(|s| s.trim() == "Discharging")
                && read("capacity")
                    .and_then(|c| c.trim().parse::<u8>().ok())
                    .is_some_and(|c| c < floor)
        })
    })
}

/// How many seconds apart `def` is asked: as often as Settings says, and no
/// more often than its file allows. A provider that only reads this machine
/// is asked as often as its file allows whatever Settings says: Settings'
/// figure is how often a vendor is asked, and this asks no one.
#[must_use]
pub fn every(def: &Definition, config: &Config) -> i64 {
    if def.local {
        return def.refresh;
    }
    def.refresh.max(config.refresh_minutes.saturating_mul(60))
}

/// Ask `def` if it is time to, or regardless when `force`d, and keep what
/// came of it; and say so, once, if that took it past the warning. Returns
/// what is kept either way.
///
/// Only one process asks at a time ([`store::claim`]): the bar runs once for
/// every screen, and they all find the same provider due at the same moment.
#[must_use]
pub fn refresh(def: &Definition, config: &Config, now: i64, force: bool) -> Kept {
    let every = every(def, config);
    let kept = store::load(&def.id);
    if !force && !kept.due(now, every) {
        return kept;
    }
    let Some(_claim) = store::claim(&def.id) else {
        return kept;
    };
    // Read again now that it is ours: whoever had it may just have asked.
    let kept = store::load(&def.id);
    if !force && !kept.due(now, every) {
        return kept;
    }
    let outcome = credentials(def).and_then(|input| ask(def, &input));
    let mut kept = kept.after(now, outcome);
    let (level, say) = notify::due(def, &kept, config, now);
    kept.told = level;
    store::save(&def.id, &kept);
    if let Some((summary, body)) = say {
        notify::send(&summary, &body, level >= 2);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::{Input, ask, every, locked, low_battery};
    use crate::config::Config;
    use crate::definition::Definition;

    fn provider(script: &str) -> Definition {
        let dir = std::env::temp_dir().join(format!("alpymist-ai-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = format!("p{}", script.len());
        let path = dir.join(&name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        let _ = std::process::Command::new("chmod")
            .arg("+x")
            .arg(&path)
            .status();
        Definition::parse(
            "test",
            &format!(
                "name = \"Test\"\ndescription = \"\"\nexec = \"{}\"\nrefresh = 300\n",
                path.display()
            ),
        )
        .unwrap()
    }

    #[test]
    fn a_program_is_handed_its_keys_on_its_input_and_reports_or_says_why_not() {
        let mut input = Input::default();
        input.credentials.insert("api-key".into(), "sesame".into());

        // It reports only if it was handed the key, and not as an argument.
        let good = provider(
            "[ $# = 0 ] && grep -q '\"api-key\":\"sesame\"' && \
             echo '{\"meters\":[{\"kind\":\"window\",\"label\":\"x\",\"used\":0.5}]}'",
        );
        assert_eq!(ask(&good, &input).unwrap().worst(), Some(0.5));

        let bad = provider(
            "cat >/dev/null; echo 'warming up' >&2; echo 'the key was refused' >&2; exit 3",
        );
        assert_eq!(ask(&bad, &input).unwrap_err(), "the key was refused");

        let silent = provider("cat >/dev/null; echo not json");
        assert!(ask(&silent, &input).unwrap_err().contains("not a report"));
    }

    #[test]
    fn nothing_is_asked_more_often_than_the_provider_allows() {
        let def = provider("true");
        let mut config = Config::default();
        assert_eq!(every(&def, &config), 600, "Settings' ten minutes");
        config.refresh_minutes = 1;
        assert_eq!(every(&def, &config), 300, "the provider's five");
    }

    /// Claude's limits are a file Claude Code's status line keeps. Held to
    /// Settings' ten minutes, the bar said "Claude Code has not said yet"
    /// for ten minutes after it had.
    #[test]
    fn a_provider_that_reads_this_machine_is_not_held_to_settings_interval() {
        let mut def = provider("true");
        def.local = true;
        let mut config = Config::default();
        assert_eq!(every(&def, &config), 300, "its own, not Settings' ten");
        config.refresh_minutes = 60;
        assert_eq!(every(&def, &config), 300);
    }

    #[test]
    fn a_locked_screen_and_a_low_battery_are_told() {
        let d = std::env::temp_dir().join(format!("alpymist-ai-quiet-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let file = |path: &str, text: &str| {
            let path = d.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        file("proc/41/comm", "waybar\n");
        file("proc/self/comm", "alpymist-lock\n");
        assert!(!locked(&d.join("proc")));
        file("proc/42/comm", "alpymist-lock\n");
        assert!(locked(&d.join("proc")));

        file("power/AC/type", "Mains\n");
        file("power/BAT0/type", "Battery\n");
        file("power/BAT0/status", "Discharging\n");
        file("power/BAT0/capacity", "15\n");
        assert!(low_battery(&d.join("power"), 20));
        assert!(!low_battery(&d.join("power"), 10));
        file("power/BAT0/status", "Charging\n");
        assert!(!low_battery(&d.join("power"), 20));
        std::fs::remove_dir_all(&d).ok();
    }
}

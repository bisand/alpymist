//! AI usage: which providers the bar asks, when it warns, and how often it
//! asks.
//!
//! The providers are whatever is installed: a file in
//! `/usr/share/alpymist/ai-usage` and the program it names (ADR 0017), so
//! the switches here are read from those files and nothing names a provider.
//! The values are `alpymist-ai-usage`'s own file, which the bar reads.
//!
//! Turning a provider on may need things a switch cannot ask for: a key,
//! typed unseen, or the vendor's installer for the tool it reads through,
//! which the person should see run. Then the switch opens a terminal on
//! `alpymist-ai-usage enable`, which asks; a provider with everything it
//! needs already is simply turned on. Keys are never a setting's value: they
//! would be an argument, which any process can read.

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_ai_usage::config::{Config, FILE};
use alpymist_ai_usage::definition::{Definition, discover};
use alpymist_ai_usage::secrets;
use std::fmt::Write as _;
use std::sync::OnceLock;

/// What a provider's switch is called, before its id.
const PROVIDER: &str = "ai.provider-";

/// A string that lives as long as the process, for a `Setting` to carry.
fn kept(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The providers installed, found once.
fn providers() -> &'static [Definition] {
    static FOUND: OnceLock<&'static [Definition]> = OnceLock::new();
    FOUND.get_or_init(|| Box::leak(discover().into_boxed_slice()))
}

/// The settings: a switch for each provider installed, then when to warn and
/// how often to ask.
pub fn settings() -> Vec<Setting> {
    let defaults = Config::default();
    let account = |id, title, description, keywords, kind, default| Setting {
        id,
        title,
        description,
        keywords,
        kind,
        default,
        scope: Scope::Account,
        applies: Applies::Now,
    };
    let mut all: Vec<Setting> = providers()
        .iter()
        .map(|def| {
            let mut description = def.description.clone();
            if def.unofficial {
                description
                    .push_str(". Read in a way its vendor does not document: it may stop working");
            }
            if let Some(requires) = &def.requires {
                let _ = write!(description, ". Needs {}", requires.about);
            }
            account(
                kept(format!("{PROVIDER}{}", def.id)),
                kept(def.name.clone()),
                kept(description),
                Box::leak(
                    vec![kept(def.id.clone()), "provider", "usage", "limit"].into_boxed_slice(),
                ) as &'static [&'static str],
                Kind::Switch,
                Value::Bool(false),
            )
        })
        .collect();
    all.extend([
        account(
            "ai.warn-at",
            "Warn from",
            "How much of a limit is used before the bar changes colour.",
            &["threshold", "warning", "percent", "limit"],
            Kind::Number {
                min: 50,
                max: 95,
                step: 5,
                unit: "%",
            },
            Value::Number(i64::from(defaults.warn_at)),
        ),
        account(
            "ai.notify",
            "Notify when a limit is near",
            "A notification when a provider passes that, and one more when it is nearly used up.",
            &["notification", "alert", "mako"],
            Kind::Switch,
            Value::Bool(defaults.notify),
        ),
        account(
            "ai.refresh",
            "Ask every",
            "Minutes between askings. A provider that allows less often is asked less often, \
             and none is asked while the screen is locked.",
            &["interval", "refresh", "poll", "minutes"],
            Kind::Number {
                min: 1,
                max: 120,
                step: 1,
                unit: "min",
            },
            Value::Number(defaults.refresh_minutes),
        ),
        account(
            "ai.battery-floor",
            "Stop asking on battery below",
            "On battery with less than this left, nothing is asked and the last answers stay. \
             Zero asks regardless.",
            &["battery", "power", "save"],
            Kind::Number {
                min: 0,
                max: 100,
                step: 5,
                unit: "%",
            },
            Value::Number(i64::from(defaults.battery_floor)),
        ),
    ]);
    all
}

fn load(env: &Env) -> Config {
    Config::load_from(&env.account(FILE))
}

/// The provider a switch is for, by the setting's id.
fn provider(id: &str) -> Option<&'static Definition> {
    let id = id.strip_prefix(PROVIDER)?;
    providers().iter().find(|d| d.id == id)
}

/// A setting's value.
pub fn get(env: &Env, s: &Setting) -> Value {
    let c = load(env);
    match s.id {
        "ai.warn-at" => Value::Number(i64::from(c.warn_at)),
        "ai.notify" => Value::Bool(c.notify),
        "ai.refresh" => Value::Number(c.refresh_minutes),
        "ai.battery-floor" => Value::Number(i64::from(c.battery_floor)),
        id => Value::Bool(
            id.strip_prefix(PROVIDER)
                .is_some_and(|provider| c.is_enabled(provider)),
        ),
    }
}

/// Whether turning `def` on needs someone at a terminal: a tool of its
/// vendor's to install, or a key the keyring does not have.
fn needs_asking(def: &Definition, has_program: impl Fn(&str) -> bool) -> bool {
    if def
        .requires
        .as_ref()
        .is_some_and(|r| !has_program(&r.program))
    {
        return true;
    }
    def.credentials
        .iter()
        .filter(|c| !c.optional)
        .any(|c| !matches!(secrets::lookup(&def.id, &c.key), Ok(Some(_))))
}

/// Whether `program` is there to run, on `PATH` or in `~/.local/bin`.
fn installed(program: &str) -> bool {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/bin"));
    }
    dirs.iter().any(|dir| dir.join(program).is_file())
}

/// Change one. Says what else it did.
///
/// # Errors
/// The file could not be written, or a provider could not be turned on.
pub fn set(env: &Env, s: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    let value = value.unwrap_or(&s.default);
    if let Some(def) = provider(s.id) {
        return turn(env, def, value.as_bool().unwrap_or(false));
    }
    let mut c = load(env);
    let number = |least: i64, most: i64| value.as_number().unwrap_or(least).clamp(least, most);
    match s.id {
        "ai.warn-at" => c.warn_at = u8::try_from(number(50, 95)).unwrap_or(80),
        "ai.notify" => c.notify = value.as_bool().unwrap_or(true),
        "ai.refresh" => c.refresh_minutes = number(1, 120),
        "ai.battery-floor" => c.battery_floor = u8::try_from(number(0, 100)).unwrap_or(20),
        other => return Err(format!("no setting `{other}`")),
    }
    c.save_to(&env.account(FILE))?;
    Ok(Vec::new())
}

/// Turn a provider on or off, through `alpymist-ai-usage`, which also does
/// its setup and its undoing.
fn turn(env: &Env, def: &Definition, on: bool) -> Result<Vec<String>, String> {
    if !on {
        env.run(&["alpymist-ai-usage", "disable", &def.id])?;
        return Ok(Vec::new());
    }
    if secrets::state().refusal().is_none() && !needs_asking(def, installed) {
        env.run(&["alpymist-ai-usage", "enable", &def.id])?;
        return Ok(Vec::new());
    }
    // Something to type or to watch: in a terminal, which stays until read.
    let command = [
        "sh".to_owned(),
        "-c".to_owned(),
        format!(
            "alpymist-ai-usage enable {}; printf '\\nPress Enter to close. '; read -r _",
            def.id
        ),
    ];
    let argv = alpymist_core::defaults::terminal_argv(
        &alpymist_core::defaults::Places::current(),
        &command,
    );
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .spawn()
        .map_err(|e| format!("{}: {e}", argv[0]))?;
    Ok(vec![format!(
        "{} is turned on in the terminal that opened: it has something to ask.",
        def.name
    )])
}

#[cfg(test)]
mod tests {
    use super::FILE;
    use crate::env::Env;
    use crate::{Settings, Value};
    use alpymist_ai_usage::config::Config;
    use std::sync::Mutex;

    #[test]
    fn the_thresholds_are_the_bars_own_file_and_stay_in_range() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-ai-settings-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();

        assert_eq!(settings.get(&env, "ai.warn-at"), Ok(Value::Number(80)));
        assert_eq!(settings.get(&env, "ai.notify"), Ok(Value::Bool(true)));
        settings.set(&env, "ai.warn-at", "90", false).unwrap();
        settings.set(&env, "ai.notify", "off", false).unwrap();
        settings.set(&env, "ai.refresh", "30", false).unwrap();
        assert!(
            settings.set(&env, "ai.warn-at", "20", false).is_err(),
            "below the least"
        );

        let written = Config::load_from(&env.account(FILE));
        assert_eq!(
            (written.warn_at, written.notify, written.refresh_minutes),
            (90, false, 30)
        );
        assert!(written.enabled.is_empty(), "nothing is on until turned on");

        settings.reset(&env, "ai.warn-at", false).unwrap();
        assert_eq!(settings.get(&env, "ai.warn-at"), Ok(Value::Number(80)));
        std::fs::remove_dir_all(&d).ok();
    }
}

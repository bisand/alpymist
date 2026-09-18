//! `alpymist list | get | set | reset`: every setting on the command line.

use crate::root;
use alpymist_settings::{Env, Error, Kind, Scope, Setting, Settings, Value};
use serde_json::json;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn kind_json(kind: &Kind) -> serde_json::Value {
    match kind {
        Kind::Action { label } => json!({ "type": "action", "label": label }),
        Kind::Switch => json!({ "type": "switch" }),
        Kind::Number {
            min,
            max,
            step,
            unit,
        } => json!({ "type": "number", "min": min, "max": max, "step": step, "unit": unit }),
        Kind::Choice(choices) => json!({
            "type": "choice",
            "choices": choices
                .iter()
                .map(|c| json!({ "value": c.value, "label": c.label }))
                .collect::<Vec<_>>(),
        }),
    }
}

fn value_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => json!(b),
        Value::Number(n) => json!(n),
        Value::Text(t) => json!(t),
    }
}

fn setting_json(s: &Setting, value: std::result::Result<Value, Error>) -> serde_json::Value {
    let mut doc = json!({
        "id": s.id,
        "area": s.area(),
        "title": s.title,
        "description": s.description,
        "keywords": s.keywords,
        "kind": kind_json(&s.kind),
        "default": value_json(&s.default),
        "scope": s.scope.id(),
        "applies": s.applies.id(),
    });
    match value {
        Ok(v) => doc["value"] = value_json(&v),
        Err(e) => doc["error"] = json!(e.to_string()),
    }
    doc
}

/// Every setting whose id starts with `filter`, or one setting's choices.
pub fn list(settings: &Settings, env: &Env, filter: Option<&str>, as_json: bool) -> Result<()> {
    let chosen: Vec<&Setting> = settings
        .all()
        .iter()
        .filter(|s| filter.is_none_or(|f| s.id == f || s.area() == f || s.id.starts_with(f)))
        .collect();
    if chosen.is_empty() {
        return Err(format!(
            "nothing matches `{}`; areas: {}",
            filter.unwrap_or_default(),
            settings
                .areas()
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    if as_json {
        let docs: Vec<_> = chosen
            .iter()
            .map(|s| setting_json(s, settings.get(env, s.id)))
            .collect();
        println!("{}", serde_json::to_string_pretty(&docs)?);
        return Ok(());
    }
    // One setting asked for by id: its choices too.
    if let [s] = chosen.as_slice()
        && filter == Some(s.id)
    {
        let value = settings.get(env, s.id);
        println!("{}  {}", s.id, s.title);
        println!("  {}", s.description);
        match &value {
            Ok(v) => println!("  now: {} ({v})", s.describe(v)),
            Err(e) => println!("  now: unknown ({e})"),
        }
        println!("  default: {}", s.default);
        if s.scope == Scope::System {
            println!("  system setting: changing it asks for an administrator's password");
        }
        match &s.kind {
            Kind::Action { label } => println!("  does: {label}"),
            Kind::Switch => println!("  values: on, off"),
            Kind::Number {
                min,
                max,
                step,
                unit,
            } => {
                println!("  values: {min} to {max}{unit}, in steps of {step}");
            }
            Kind::Choice(choices) => {
                println!("  values:");
                for c in choices {
                    println!("    {:<24} {}", c.value, c.label);
                }
            }
        }
        return Ok(());
    }
    let wide = chosen.iter().map(|s| s.id.len()).max().unwrap_or(0);
    for s in chosen {
        let now = settings
            .get(env, s.id)
            .map_or_else(|_| "—".to_owned(), |v| v.to_string());
        println!("{:<wide$}  {:<12}  {}", s.id, now, s.title);
    }
    Ok(())
}

/// Print one setting's value.
pub fn get(settings: &Settings, env: &Env, id: &str, as_json: bool) -> Result<()> {
    let value = settings.get(env, id)?;
    if as_json {
        println!("{}", value_json(&value));
    } else {
        println!("{value}");
    }
    Ok(())
}

/// Do a setting that is a thing to do rather than a thing to be.
///
/// `alpymist set <id>` with no value. That is not a reset: it is how an action
/// — a screensaver's preview — is asked for, and anything else says it needs a
/// value rather than quietly undoing what was there.
///
/// # Errors
/// An unknown id, or a setting that takes a value.
pub fn act(settings: &Settings, env: &Env, id: &str, force: bool, live: bool) -> Result<()> {
    let setting = settings
        .all()
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| Error::Unknown(id.to_owned()))?;
    if !matches!(setting.kind, Kind::Action { .. }) {
        return Err(format!("{id} needs a value; `alpymist show {id}` says which it takes").into());
    }
    change(settings, env, id, None, force, live)
}

/// Set, or reset with `value` of `None`. A system setting goes through
/// pkexec; the running session is told from this process, as the person.
///
/// # Errors
/// An unknown id, a value it does not take, or the change could not be made.
pub fn change(
    settings: &Settings,
    env: &Env,
    id: &str,
    value: Option<&str>,
    force: bool,
    live: bool,
) -> Result<()> {
    let setting = settings
        .find(id)
        .ok_or_else(|| Error::Unknown(id.to_owned()))?;
    root::refuse_account_settings(setting)?;
    let result = match value {
        Some(text) => settings.set(env, id, text, force),
        None => settings.reset(env, id, force),
    };
    let changed = match result {
        Ok(changed) => changed,
        Err(Error::NeedsRoot(_)) => {
            // Check the value here, before a password is asked for it.
            let checked = match value {
                Some(text) => setting.parse(text).map_err(Error::Invalid)?,
                None => setting.default.clone(),
            };
            let mut args = match value {
                Some(text) => vec!["set", id, text],
                None => vec!["reset", id],
            };
            if force {
                args.push("--force");
            }
            args.push("--no-live");
            root::again(&args)?;
            if live {
                settings.live(env, id, &checked)?;
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    if live
        && !env.is_root
        && let Err(e) = settings.live(env, id, &changed.value)
    {
        eprintln!("alpymist: saved, but the running session was not told: {e}");
    }
    println!("{}: {}", id, setting.describe(&changed.value));
    for note in changed.notes {
        println!("  {note}");
    }
    Ok(())
}

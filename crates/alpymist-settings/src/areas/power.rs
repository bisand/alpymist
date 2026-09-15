//! The lid, the power button, what the bar shows, the power mode and the
//! charge limit: the same `power.toml` and helper as the power popup.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_power::battery::Power;
use alpymist_power::config::{Action, Config};
use alpymist_power::profile::{Knobs, Profile};

/// The account's power file.
pub const CONFIG: &str = "alpymist/power.toml";
/// The charge limits the helper accepts.
pub const LIMITS: [u8; 4] = [100, 90, 80, 60];

fn actions(order: &[Action]) -> Kind {
    Kind::Choice(
        order
            .iter()
            .map(|a| Choice::new(a.id(), a.label()))
            .collect(),
    )
}

/// The settings.
#[allow(clippy::too_many_lines)] // a table
pub fn settings() -> Vec<Setting> {
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
    let defaults = Config::default();
    let action = |a: Action| Value::Text(a.id().into());
    vec![
        account(
            "power.lid",
            "Closing the lid",
            "What happens when the lid closes on battery.",
            &["suspend", "sleep", "laptop"],
            actions(&Action::LID),
            action(defaults.actions.lid),
        ),
        account(
            "power.lid-on-power",
            "Closing the lid on the charger",
            "What happens when the lid closes while plugged in.",
            &["suspend", "sleep", "laptop", "plugged"],
            actions(&Action::LID),
            action(defaults.actions.lid_on_power),
        ),
        account(
            "power.lid-docked",
            "Closing the lid with a screen connected",
            "What happens when the lid closes with another screen plugged in.",
            &["dock", "external monitor", "clamshell"],
            actions(&Action::LID),
            action(defaults.actions.lid_docked),
        ),
        account(
            "power.button",
            "Power button",
            "What pressing the power button does.",
            &["shut down", "suspend", "power off"],
            actions(&Action::BUTTON),
            action(defaults.actions.power_button),
        ),
        account(
            "power.bar-percentage",
            "Battery percentage in the bar",
            "Show the charge beside the battery icon.",
            &["battery", "top bar", "waybar"],
            Kind::Switch,
            Value::Bool(defaults.bar.percentage),
        ),
        account(
            "power.bar-time",
            "Time left in the bar",
            "Show how long the battery lasts, or until it is full.",
            &["battery", "remaining", "top bar"],
            Kind::Switch,
            Value::Bool(defaults.bar.time),
        ),
        account(
            "power.bar-power",
            "Power draw in the bar",
            "Show how many watts go in or out.",
            &["battery", "watts", "top bar"],
            Kind::Switch,
            Value::Bool(defaults.bar.power),
        ),
        account(
            "power.bar-profile",
            "Power mode in the bar",
            "Show the power mode beside the battery icon.",
            &["battery", "profile", "top bar"],
            Kind::Switch,
            Value::Bool(defaults.bar.profile),
        ),
        account(
            "power.mode",
            "Power mode",
            "Save battery, balance, or give the processor everything.",
            &["profile", "performance", "battery saver", "cpu governor"],
            Kind::Choice(
                Profile::ALL
                    .iter()
                    .map(|p| Choice::new(p.id(), p.label()))
                    .collect(),
            ),
            Value::Text(Profile::Balanced.id().into()),
        ),
        account(
            "power.charge-limit",
            "Charge limit",
            "Stop charging below full, which keeps a battery that is mostly plugged in healthier.",
            &["battery health", "threshold", "80 percent"],
            Kind::Choice(
                LIMITS
                    .iter()
                    .map(|l| {
                        Choice::new(
                            l.to_string(),
                            if *l == 100 {
                                "Full".to_owned()
                            } else {
                                format!("{l}%")
                            },
                        )
                    })
                    .collect(),
            ),
            Value::Text("100".into()),
        ),
    ]
}

fn load(env: &Env) -> Result<Config, String> {
    let path = env.account(CONFIG);
    match std::fs::read_to_string(&path) {
        Ok(text) => Config::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(crate::io_error(&path, &e)),
    }
}

/// A setting's value.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    let c = load(env)?;
    Ok(match setting.id {
        "power.lid" => Value::Text(c.actions.lid.id().into()),
        "power.lid-on-power" => Value::Text(c.actions.lid_on_power.id().into()),
        "power.lid-docked" => Value::Text(c.actions.lid_docked.id().into()),
        "power.button" => Value::Text(c.actions.power_button.id().into()),
        "power.bar-percentage" => Value::Bool(c.bar.percentage),
        "power.bar-time" => Value::Bool(c.bar.time),
        "power.bar-power" => Value::Bool(c.bar.power),
        "power.bar-profile" => Value::Bool(c.bar.profile),
        "power.mode" => Value::Text(
            Knobs::read(&env.system("sys"))
                .current()
                .ok_or("this computer has no power modes to choose from")?
                .id()
                .into(),
        ),
        _ => Value::Text(
            Power::read(&env.system("sys/class/power_supply"))
                .batteries
                .iter()
                .find_map(|b| b.charge_limit)
                .ok_or("no battery here takes a charge limit")?
                .to_string(),
        ),
    })
}

/// Change a setting. The mode and limit go through alpymist-power's helper,
/// which asks for a password where polkit says to.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let value = value.unwrap_or(&setting.default);
    let text = value.as_text().unwrap_or_default();
    let action = || {
        [Action::LID.as_slice(), Action::BUTTON.as_slice()]
            .concat()
            .into_iter()
            .find(|a| a.id() == text)
            .ok_or_else(|| format!("no action `{text}`"))
    };
    match setting.id {
        "power.mode" => {
            return env
                .run(&[alpymist_power::actions::HELPER, "profile", text])
                .map(drop);
        }
        "power.charge-limit" => {
            return env
                .run(&[alpymist_power::actions::HELPER, "charge-limit", text])
                .map(drop);
        }
        _ => {}
    }
    let mut c = load(env)?;
    let on = value.as_bool().unwrap_or(false);
    match setting.id {
        "power.lid" => c.actions.lid = action()?,
        "power.lid-on-power" => c.actions.lid_on_power = action()?,
        "power.lid-docked" => c.actions.lid_docked = action()?,
        "power.button" => c.actions.power_button = action()?,
        "power.bar-percentage" => c.bar.percentage = on,
        "power.bar-time" => c.bar.time = on,
        "power.bar-power" => c.bar.power = on,
        _ => c.bar.profile = on,
    }
    crate::generated::replace(&env.account(CONFIG), &c.to_toml())
}

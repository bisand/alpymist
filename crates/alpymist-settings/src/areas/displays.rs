//! Displays: how the screens are arranged is its own page in the settings
//! app, drawn from `alpymist-displays`, and has no value to keep here. What
//! is a setting is what a closed lid does to the laptop's panel while another
//! screen is on, kept with the layouts in `~/.config/alpymist/displays.toml`
//! (ADR 0015).

use crate::env::Env;
use crate::model::{Applies, Kind, Scope, Setting, Value};
use alpymist_displays::layout::{FILE, Layouts};

/// The lid's setting.
pub const LID: &str = "displays.lid";

/// The settings.
pub fn settings() -> Vec<Setting> {
    vec![Setting {
        id: LID,
        title: "Turn the laptop's screen off when the lid closes",
        description: "While another screen is on, closing the lid turns the built-in \
                      screen off and moves what was on it to the others.",
        keywords: &["lid", "clamshell", "dock", "laptop", "built-in", "external"],
        kind: Kind::Switch,
        default: Value::Bool(true),
        scope: Scope::Account,
        applies: Applies::Now,
    }]
}

/// Whether a closed lid turns the panel off.
pub fn get(env: &Env) -> Value {
    Value::Bool(Layouts::load(&env.account(FILE)).lid_off)
}

/// Say whether it does, keeping every layout as it is.
pub fn set(env: &Env, value: Option<&Value>) -> Result<(), String> {
    let path = env.account(FILE);
    let mut all = Layouts::load(&path);
    all.lid_off = value.and_then(Value::as_bool).unwrap_or(true);
    all.save(&path)
}

/// Put the screens as the lid now has them, in case it is closed.
pub fn live(env: &Env) -> Result<(), String> {
    env.run(&["alpymist", "displays", "apply"]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::{get, set};
    use crate::env::Env;
    use crate::model::Value;
    use std::sync::Mutex;

    #[test]
    fn the_lid_is_kept_beside_the_layouts() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let dir =
            std::env::temp_dir().join(format!("alpymist-displays-area-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let env = Env::test(&dir, false, &RAN);
        assert_eq!(get(&env), Value::Bool(true), "on until turned off");
        set(&env, Some(&Value::Bool(false))).unwrap();
        assert_eq!(get(&env), Value::Bool(false));
        let text = std::fs::read_to_string(env.account(super::FILE)).unwrap();
        assert!(text.contains("lid-turns-panel-off = false"), "{text}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

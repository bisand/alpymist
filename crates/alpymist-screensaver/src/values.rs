//! What a screensaver has been set to.
//!
//! A screensaver's own settings live in a file of its own,
//! `~/.config/alpymist/screensavers/<id>.toml`, holding only the keys its
//! [`Definition`] declares. The program reads it; Settings writes it; neither
//! has to know what the other will do with a key it has not heard of.
//!
//! A missing file, a missing key, or a value of the wrong shape is the default
//! the definition gives. That is deliberate: a screensaver is what a machine
//! shows when nobody is at it, and refusing to draw because a number was
//! mistyped is the one failure nobody would be there to see.

use crate::definition::{Definition, Dial};
use std::collections::BTreeMap;

/// A value a screensaver setting can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A switch.
    Bool(bool),
    /// A number.
    Number(i64),
    /// A choice's value.
    Text(String),
}

impl Value {
    /// As the file writes it.
    #[must_use]
    pub fn to_toml(&self) -> String {
        match self {
            Self::Bool(v) => v.to_string(),
            Self::Number(v) => v.to_string(),
            Self::Text(v) => quote(v),
        }
    }
}

/// A string as TOML writes it.
///
/// Written out here rather than through toml's own formatter: this crate parses
/// TOML and does not otherwise print it, and a choice's value is a short name
/// from a definition file, not arbitrary text.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Everything one screensaver has been set to, with its defaults filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Values {
    held: BTreeMap<String, Value>,
}

impl Values {
    /// The defaults its definition gives, with whatever the account has set
    /// over the top.
    #[must_use]
    pub fn read(def: &Definition) -> Self {
        let text = std::fs::read_to_string(crate::definition::values_path(&def.id));
        Self::of(def, text.as_deref().unwrap_or(""))
    }

    /// The same, from text rather than from a file.
    #[must_use]
    pub fn of(def: &Definition, text: &str) -> Self {
        let mut held = BTreeMap::new();
        for knob in &def.knobs {
            held.insert(knob.key.clone(), knob.default());
        }
        let Ok(table) = text.parse::<toml::Table>() else {
            return Self { held };
        };
        for knob in &def.knobs {
            let Some(found) = table.get(&knob.key) else {
                continue;
            };
            if let Some(value) = knob.read(found) {
                held.insert(knob.key.clone(), value);
            }
        }
        Self { held }
    }

    /// What `key` is set to, or `None` when the screensaver has no such setting.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.held.get(key)
    }

    /// `key` as a number, or `fallback` when it is not one.
    #[must_use]
    pub fn number(&self, key: &str, fallback: i64) -> i64 {
        match self.held.get(key) {
            Some(Value::Number(v)) => *v,
            _ => fallback,
        }
    }

    /// `key` as a switch, or `fallback` when it is not one.
    #[must_use]
    pub fn switch(&self, key: &str, fallback: bool) -> bool {
        match self.held.get(key) {
            Some(Value::Bool(v)) => *v,
            _ => fallback,
        }
    }

    /// `key` as a choice, or `fallback` when it is not one.
    #[must_use]
    pub fn text<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        match self.held.get(key) {
            Some(Value::Text(v)) => v,
            _ => fallback,
        }
    }

    /// The file these would be written as.
    #[must_use]
    pub fn to_toml(&self, def: &Definition) -> String {
        use std::fmt::Write as _;
        let mut out = format!(
            "# {}: Settings writes this file; edit it too.\n\
             # Every key here is one {} declares; anything else is ignored.\n\n",
            def.name, def.name
        );
        for knob in &def.knobs {
            let Some(value) = self.held.get(&knob.key) else {
                continue;
            };
            if !knob.description.is_empty() {
                let _ = writeln!(out, "# {}", knob.description);
            }
            let _ = writeln!(out, "{} = {}", knob.key, value.to_toml());
        }
        out
    }

    /// The same, with `key` set to `value` — which is dropped when the
    /// screensaver has no such setting, or the value is the wrong shape for it.
    #[must_use]
    pub fn with(mut self, def: &Definition, key: &str, value: Value) -> Self {
        if let Some(knob) = def.knobs.iter().find(|k| k.key == key)
            && knob.accepts(&value)
        {
            self.held.insert(key.to_owned(), knob.clamp(value));
        }
        self
    }
}

impl crate::definition::Knob {
    /// Its value when nobody has set it.
    #[must_use]
    pub fn default(&self) -> Value {
        match &self.dial {
            Dial::Switch { default } => Value::Bool(*default),
            Dial::Number { default, .. } => Value::Number(*default),
            Dial::Choice { default, .. } => Value::Text(default.clone()),
        }
    }

    /// Whether `value` is the shape this knob takes at all.
    #[must_use]
    pub fn accepts(&self, value: &Value) -> bool {
        matches!(
            (&self.dial, value),
            (Dial::Switch { .. }, Value::Bool(_))
                | (Dial::Number { .. }, Value::Number(_))
                | (Dial::Choice { .. }, Value::Text(_))
        )
    }

    /// `value` brought inside what this knob allows.
    ///
    /// A number outside its range is clamped and a choice that is not one of the
    /// choices becomes the default, rather than either being refused: a
    /// hand-edited file should still draw something.
    #[must_use]
    pub fn clamp(&self, value: Value) -> Value {
        match (&self.dial, value) {
            (Dial::Number { min, max, .. }, Value::Number(v)) => Value::Number(v.clamp(*min, *max)),
            (Dial::Choice { option, .. }, Value::Text(v)) => {
                if option.iter().any(|o| o.value == v) {
                    Value::Text(v)
                } else {
                    self.default()
                }
            }
            (_, value) => value,
        }
    }

    /// Read this knob's value out of what the file held there.
    #[must_use]
    fn read(&self, found: &toml::Value) -> Option<Value> {
        let value = match &self.dial {
            Dial::Switch { .. } => Value::Bool(found.as_bool()?),
            Dial::Number { .. } => Value::Number(found.as_integer()?),
            Dial::Choice { .. } => Value::Text(found.as_str()?.to_owned()),
        };
        Some(self.clamp(value))
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, Values};
    use crate::definition::Definition;

    const DEF: &str = r#"
name = "Mountains"
exec = "alpymist-saver-mountains"

[[setting]]
key = "block"
title = "How chunky"
kind = "number"
min = 1
max = 16
default = 6

[[setting]]
key = "stars"
title = "Stars"
kind = "switch"
default = true

[[setting]]
key = "weather"
title = "Weather"
kind = "choice"
default = "mist"
[[setting.option]]
value = "mist"
label = "Mist"
[[setting.option]]
value = "clear"
label = "Clear"
"#;

    fn def() -> Definition {
        Definition::parse("mountains", DEF).unwrap()
    }

    #[test]
    fn an_empty_file_is_every_default() {
        let v = Values::of(&def(), "");
        assert_eq!(v.number("block", 0), 6);
        assert!(v.switch("stars", false));
        assert_eq!(v.text("weather", ""), "mist");
    }

    #[test]
    fn what_the_file_says_wins_over_the_default() {
        let v = Values::of(&def(), "block = 3\nstars = false\nweather = \"clear\"\n");
        assert_eq!(v.number("block", 0), 3);
        assert!(!v.switch("stars", true));
        assert_eq!(v.text("weather", ""), "clear");
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_the_default_rather_than_a_refusal() {
        let v = Values::of(&def(), "block = \"lots\"\nstars = 3\n");
        assert_eq!(v.number("block", 0), 6, "nobody is there to see an error");
        assert!(v.switch("stars", false));
    }

    #[test]
    fn a_number_outside_its_range_comes_back_into_it() {
        let v = Values::of(&def(), "block = 400\n");
        assert_eq!(v.number("block", 0), 16);
        let v = Values::of(&def(), "block = -5\n");
        assert_eq!(v.number("block", 0), 1);
    }

    #[test]
    fn a_choice_that_is_not_one_of_the_choices_is_the_default() {
        let v = Values::of(&def(), "weather = \"hurricane\"\n");
        assert_eq!(v.text("weather", ""), "mist");
    }

    #[test]
    fn a_file_that_does_not_parse_at_all_is_every_default() {
        let v = Values::of(&def(), "this is not toml {{{");
        assert_eq!(v.number("block", 0), 6);
    }

    #[test]
    fn a_key_the_screensaver_does_not_have_is_ignored() {
        let v = Values::of(&def(), "block = 3\nnonsense = 9\n");
        assert_eq!(v.number("block", 0), 3);
        assert!(v.get("nonsense").is_none(), "it is not the program's");
    }

    #[test]
    fn setting_one_leaves_the_others_alone_and_refuses_the_wrong_shape() {
        let d = def();
        let v = Values::of(&d, "").with(&d, "block", Value::Number(9));
        assert_eq!(v.number("block", 0), 9);
        assert!(v.switch("stars", false), "the others are untouched");

        let v = v.with(&d, "block", Value::Bool(true));
        assert_eq!(v.number("block", 0), 9, "a switch is not a number");

        let v = v.with(&d, "nonsense", Value::Number(1));
        assert!(v.get("nonsense").is_none());
    }

    #[test]
    fn what_it_writes_is_what_it_reads_back() {
        let d = def();
        let v = Values::of(&d, "block = 11\nstars = false\nweather = \"clear\"\n");
        assert_eq!(Values::of(&d, &v.to_toml(&d)), v);
        let defaults = Values::of(&d, "");
        assert_eq!(Values::of(&d, &defaults.to_toml(&d)), defaults);
    }
}

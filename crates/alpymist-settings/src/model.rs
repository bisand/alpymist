//! What a setting is: its id, what it is called, what values it takes, whose
//! it is and when a change shows.

use std::fmt;

/// Whose setting it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The person's own, under `~/.config/alpymist`. No password.
    Account,
    /// The whole computer's, under `/etc`. An administrator's password.
    System,
}

impl Scope {
    /// As the command line and JSON write it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::System => "system",
        }
    }
}

/// When a change shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// At once.
    Now,
    /// In windows opened after the change.
    NewWindows,
    /// At the next login.
    NextLogin,
    /// At the next update.
    NextUpdate,
}

impl Applies {
    /// As the command line and JSON write it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::NewWindows => "new-windows",
            Self::NextLogin => "next-login",
            Self::NextUpdate => "next-update",
        }
    }

    /// What a person is told after a change, if anything.
    #[must_use]
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::Now => None,
            Self::NewWindows => Some("Windows opened from now on show the change."),
            Self::NextLogin => Some("Takes effect at the next login."),
            Self::NextUpdate => Some("Takes effect at the next update."),
        }
    }
}

/// One of a choice's values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// As the command line and the files write it.
    pub value: String,
    /// As a person reads it.
    pub label: String,
}

impl Choice {
    /// A choice from a value and a label.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// What values a setting takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// On or off.
    Switch,
    /// One of a list.
    Choice(Vec<Choice>),
    /// A whole number in a range.
    Number {
        /// The smallest.
        min: i64,
        /// The largest.
        max: i64,
        /// How far one step goes.
        step: i64,
        /// What the number counts: `ms`, `%`, `px`, or nothing.
        unit: &'static str,
    },
}

/// A setting's value.
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
    /// The switch's state, if this is one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The number, if this is one.
    #[must_use]
    pub fn as_number(&self) -> Option<i64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The choice's value, if this is one.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(t) => f.write_str(t),
        }
    }
}

/// A group of settings: a page in the app, a word on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    /// The first part of its settings' ids: `touchpad`.
    pub id: &'static str,
    /// Its name: `Touchpad`.
    pub title: &'static str,
    /// One line on what it covers.
    pub description: &'static str,
    /// A Nerd Font glyph.
    pub icon: &'static str,
    /// Other words to find it by.
    pub keywords: &'static [&'static str],
}

/// One setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// `area.name`: `touchpad.natural-scroll`.
    pub id: &'static str,
    /// What it is called: `Natural scrolling`.
    pub title: &'static str,
    /// One sentence on what it does.
    pub description: &'static str,
    /// Other words to find it by.
    pub keywords: &'static [&'static str],
    /// What values it takes.
    pub kind: Kind,
    /// Its value when nobody has set it.
    pub default: Value,
    /// Whose it is.
    pub scope: Scope,
    /// When a change shows.
    pub applies: Applies,
}

impl Setting {
    /// The area's id: everything before the first dot.
    #[must_use]
    pub fn area(&self) -> &'static str {
        self.id.split_once('.').map_or(self.id, |(a, _)| a)
    }

    /// Read `text` as a value of this setting's kind.
    ///
    /// # Errors
    /// Not a value this setting takes, with what it does take.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        match &self.kind {
            Kind::Switch => match text.to_ascii_lowercase().as_str() {
                "true" | "on" | "yes" | "1" => Ok(Value::Bool(true)),
                "false" | "off" | "no" | "0" => Ok(Value::Bool(false)),
                _ => Err(format!("{} is on or off, not `{text}`", self.id)),
            },
            Kind::Number {
                min,
                max,
                step,
                unit,
            } => {
                let n: i64 = text
                    .trim_end_matches(unit)
                    .trim()
                    .parse()
                    .map_err(|_| format!("{} is a whole number, not `{text}`", self.id))?;
                if n < *min || n > *max {
                    return Err(format!("{} is from {min} to {max}{unit}", self.id));
                }
                if *step > 1 && (n - min) % step != 0 {
                    return Err(format!("{} goes in steps of {step}", self.id));
                }
                Ok(Value::Number(n))
            }
            Kind::Choice(choices) => choices
                .iter()
                .find(|c| c.value == text)
                .map(|c| Value::Text(c.value.clone()))
                .ok_or_else(|| {
                    let values: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
                    if values.len() > 12 {
                        format!(
                            "`{text}` is not one of {}'s {} choices; \
                             `alpymist list {}` shows them",
                            self.id,
                            values.len(),
                            self.id
                        )
                    } else {
                        format!("{} is one of {}, not `{text}`", self.id, values.join(", "))
                    }
                }),
        }
    }

    /// How a value reads to a person: a choice's label, `On`, `600 ms`.
    #[must_use]
    pub fn describe(&self, value: &Value) -> String {
        match (&self.kind, value) {
            (Kind::Switch, Value::Bool(true)) => "On".into(),
            (Kind::Switch, Value::Bool(false)) => "Off".into(),
            (Kind::Number { unit: "", .. }, Value::Number(n)) => n.to_string(),
            (Kind::Number { unit, .. }, Value::Number(n)) if *unit == "%" => format!("{n}%"),
            (Kind::Number { unit, .. }, Value::Number(n)) => format!("{n} {unit}"),
            (Kind::Choice(choices), Value::Text(t)) => choices
                .iter()
                .find(|c| &c.value == t)
                .map_or_else(|| t.clone(), |c| c.label.clone()),
            (_, v) => v.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Applies, Choice, Kind, Scope, Setting, Value};

    fn setting(kind: Kind, default: Value) -> Setting {
        Setting {
            id: "test.thing",
            title: "Thing",
            description: "",
            keywords: &[],
            kind,
            default,
            scope: Scope::Account,
            applies: Applies::Now,
        }
    }

    #[test]
    fn switches_take_the_usual_words() {
        let s = setting(Kind::Switch, Value::Bool(false));
        for on in ["true", "On", "yes", "1"] {
            assert_eq!(s.parse(on), Ok(Value::Bool(true)));
        }
        assert_eq!(s.parse("off"), Ok(Value::Bool(false)));
        assert!(s.parse("maybe").is_err());
        assert_eq!(s.describe(&Value::Bool(true)), "On");
    }

    #[test]
    fn numbers_keep_to_their_range_and_steps() {
        let s = setting(
            Kind::Number {
                min: 100,
                max: 1000,
                step: 50,
                unit: "ms",
            },
            Value::Number(600),
        );
        assert_eq!(s.parse("650"), Ok(Value::Number(650)));
        assert_eq!(s.parse("650ms"), Ok(Value::Number(650)));
        assert!(s.parse("660").is_err());
        assert!(s.parse("2000").is_err());
        assert_eq!(s.describe(&Value::Number(600)), "600 ms");
    }

    #[test]
    fn choices_take_only_their_values() {
        let s = setting(
            Kind::Choice(vec![
                Choice::new("dark", "Dark"),
                Choice::new("light", "Light"),
            ]),
            Value::Text("dark".into()),
        );
        assert_eq!(s.parse("light"), Ok(Value::Text("light".into())));
        assert!(s.parse("Light").unwrap_err().contains("dark, light"));
        assert_eq!(s.describe(&Value::Text("light".into())), "Light");
        assert_eq!(s.area(), "test");
    }
}

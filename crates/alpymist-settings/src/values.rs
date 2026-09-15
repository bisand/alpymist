//! The file of settings that have no other home: `settings.toml`, a flat
//! table of ids and values, for the account or the system.

use crate::model::Value;
use std::path::Path;

/// One `settings.toml`.
#[derive(Debug, Clone, Default)]
pub struct Values {
    table: toml::Table,
}

impl Values {
    /// Read `path`, or nothing when it is not there.
    ///
    /// # Errors
    /// It is there but cannot be read, or is not TOML.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => text
                .parse::<toml::Table>()
                .map(|table| Self { table })
                .map_err(|e| format!("{}: {}", path.display(), e.message())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(crate::io_error(path, &e)),
        }
    }

    /// The value set for `id`, if one is.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Value> {
        match self.table.get(id)? {
            toml::Value::Boolean(b) => Some(Value::Bool(*b)),
            toml::Value::Integer(n) => Some(Value::Number(*n)),
            toml::Value::String(s) => Some(Value::Text(s.clone())),
            _ => None,
        }
    }

    /// Set `id`.
    pub fn set(&mut self, id: &str, value: &Value) {
        let v = match value {
            Value::Bool(b) => toml::Value::Boolean(*b),
            Value::Number(n) => toml::Value::Integer(*n),
            Value::Text(t) => toml::Value::String(t.clone()),
        };
        self.table.insert(id.to_owned(), v);
    }

    /// Forget `id`, so its default applies.
    pub fn remove(&mut self, id: &str) {
        self.table.remove(id);
    }

    /// Write to `path`.
    ///
    /// # Errors
    /// The directory or file could not be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = format!(
            "# Alpymist settings: written by Settings and `alpymist set`.\n{}",
            toml::to_string(&self.table).unwrap_or_default()
        );
        crate::generated::replace(path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::Values;
    use crate::model::Value;

    #[test]
    fn values_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("alpymist-values-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let mut v = Values::default();
        v.set("touchpad.natural-scroll", &Value::Bool(true));
        v.set("keyboard.repeat-delay", &Value::Number(400));
        v.set("mouse.acceleration", &Value::Text("flat".into()));
        v.save(&path).unwrap();
        let back = Values::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.get("touchpad.natural-scroll"), Some(Value::Bool(true)));
        assert_eq!(back.get("keyboard.repeat-delay"), Some(Value::Number(400)));
        assert_eq!(
            back.get("mouse.acceleration"),
            Some(Value::Text("flat".into()))
        );
        assert_eq!(back.get("nope"), None);
    }
}

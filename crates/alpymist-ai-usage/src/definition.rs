//! A provider, as its file describes it.
//!
//! ```toml
//! # /usr/share/alpymist/ai-usage/openrouter.toml
//! name = "`OpenRouter`"
//! description = "Credits used and left, and this key's limit."
//! exec = "alpymist-ai-provider openrouter"
//! refresh = 300
//!
//! [[credential]]
//! key = "api-key"
//! title = "API key"
//! ```
//!
//! Anything that ships a file like this, and the program it names, is a
//! provider the bar can show: no rebuild, and nothing here has to know it
//! exists. The program is run with the credentials on its standard input and
//! prints a [`crate::report::Report`].

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where providers are declared, the most specific first: the
/// administrator's, then the packages'.
pub const DIRECTORIES: [&str; 2] = [
    "/usr/local/share/alpymist/ai-usage",
    "/usr/share/alpymist/ai-usage",
];

/// A directory looked in before those, when set: the development override.
pub const DIRECTORY_ENV: &str = "ALPYMIST_AI_USAGE_DIR";

/// The least a provider may be asked, whatever its file says: once a minute.
pub const LEAST_REFRESH: i64 = 60;

/// A secret a provider needs, kept in the keyring.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    /// Its name in the keyring and in what the program is handed.
    pub key: String,
    /// What it is called: "Admin API key".
    pub title: String,
    /// Where to get one, in a line.
    #[serde(default)]
    pub description: String,
    /// Whether the provider works without it, saying less.
    #[serde(default)]
    pub optional: bool,
}

/// The vendor's own tool a provider reads through, installed the vendor's
/// way when the provider is turned on, and never shipped by Alpymist.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// The program that must be on `PATH`, or in `~/.local/bin`.
    pub program: String,
    /// What it is, for the question asked before installing it.
    pub about: String,
    /// The vendor's own way to install it, as a shell command line. Shown,
    /// and run in a terminal where it can be watched.
    pub install: String,
}

/// A provider.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Its name in files and on the command line, from the file's own name.
    #[serde(skip)]
    pub id: String,
    /// What it is called.
    pub name: String,
    /// One line on what it can say.
    pub description: String,
    /// The program that reports, and its arguments, split on spaces.
    pub exec: String,
    /// The least seconds between two askings, as the provider allows.
    #[serde(default = "default_refresh")]
    pub refresh: i64,
    /// Whether what it reads comes without any documented promise, and may
    /// stop working when the vendor changes something.
    #[serde(default)]
    pub unofficial: bool,
    /// Run when the provider is turned on, after its tool is there: what it
    /// has to set up on this account.
    #[serde(default)]
    pub enable: Option<String>,
    /// Run when it is turned off: the same, undone.
    #[serde(default)]
    pub disable: Option<String>,
    /// The secrets it needs.
    #[serde(default, rename = "credential")]
    pub credentials: Vec<Credential>,
    /// The vendor's tool it reads through.
    #[serde(default)]
    pub requires: Option<Requires>,
}

fn default_refresh() -> i64 {
    600
}

impl Definition {
    /// Read one, whose id is its file's name without the extension.
    ///
    /// # Errors
    /// When it cannot be read, does not parse, or names nothing to run.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{}: no name", path.display()))?;
        Self::parse(id, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Read one from its text, under this id.
    ///
    /// # Errors
    /// When it does not parse, or names nothing to run.
    pub fn parse(id: &str, text: &str) -> Result<Self, String> {
        let mut def: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        id.clone_into(&mut def.id);
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("{id:?} is not a name a provider can carry"));
        }
        if def.exec.trim().is_empty() {
            return Err("no exec: nothing to run".into());
        }
        if def.name.trim().is_empty() {
            return Err("no name: nothing to call it".into());
        }
        if def.credentials.iter().any(|c| {
            c.key.is_empty()
                || !c
                    .key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        }) {
            return Err("a credential's key is letters, digits and dashes".into());
        }
        def.refresh = def.refresh.max(LEAST_REFRESH);
        Ok(def)
    }

    /// The program and its arguments.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        self.exec.split_whitespace().map(str::to_owned).collect()
    }
}

/// Every provider installed, by id. A file that does not parse is said on
/// standard error and left out: one badly written provider does not take
/// the others with it.
#[must_use]
pub fn discover() -> Vec<Definition> {
    let mut directories: Vec<PathBuf> = DIRECTORIES.map(PathBuf::from).to_vec();
    // For trying a provider out of a source tree, before it is installed.
    if let Some(dir) = std::env::var_os(DIRECTORY_ENV).filter(|d| !d.is_empty()) {
        directories.insert(0, PathBuf::from(dir));
    }
    discover_in(&directories)
}

/// As [`discover`], in these directories, the first to name an id winning.
#[must_use]
pub fn discover_in(directories: &[PathBuf]) -> Vec<Definition> {
    let mut found: Vec<Definition> = Vec::new();
    for dir in directories {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Not a hidden file: an editor's or an archive's leftovers.
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if hidden || path.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            match Definition::read(&path) {
                Ok(def) if found.iter().any(|d| d.id == def.id) => {}
                Ok(def) => found.push(def),
                Err(why) => eprintln!("alpymist-ai-usage: {why}"),
            }
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

#[cfg(test)]
mod tests {
    use super::{Definition, LEAST_REFRESH};

    #[test]
    fn a_definition_is_read_and_a_bad_one_is_refused() {
        let def = Definition::parse(
            "openrouter",
            r#"
            name = "OpenRouter"
            description = "Credits."
            exec = "alpymist-ai-provider openrouter"
            refresh = 5

            [[credential]]
            key = "api-key"
            title = "API key"

            [requires]
            program = "tool"
            about = "The vendor's tool"
            install = "curl https://example.org/install.sh | sh"
            "#,
        )
        .unwrap();
        assert_eq!(def.argv(), ["alpymist-ai-provider", "openrouter"]);
        assert_eq!(
            def.refresh, LEAST_REFRESH,
            "never asked more than once a minute"
        );
        assert_eq!(def.credentials[0].key, "api-key");
        assert!(!def.credentials[0].optional && !def.unofficial);
        assert_eq!(def.requires.unwrap().program, "tool");

        let no_exec = "name = \"x\"\ndescription = \"\"\nexec = \" \"\n";
        assert!(Definition::parse("x", no_exec).is_err());
        let fine = "name = \"x\"\ndescription = \"\"\nexec = \"x\"\n";
        assert!(Definition::parse("../x", fine).is_err());
        assert!(Definition::parse("x", &format!("{fine}surprise = 1\n")).is_err());
    }
}

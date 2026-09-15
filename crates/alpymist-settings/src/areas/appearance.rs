//! Light or dark, the accent and the text size: the account's `theme.toml`.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_theme::{Accent, FONT_SIZES, Scheme, ThemeFile};

/// The account's theme file.
pub const ACCOUNT: &str = "alpymist/theme.toml";
/// The system's.
pub const SYSTEM: &str = "etc/alpymist/theme.toml";

/// The settings.
pub fn settings() -> Vec<Setting> {
    let default = ThemeFile::default();
    let account = |id, title, description, keywords, kind, default| Setting {
        id,
        title,
        description,
        keywords,
        kind,
        default,
        scope: Scope::Account,
        applies: Applies::NewWindows,
    };
    vec![
        account(
            "appearance.scheme",
            "Style",
            "Light text on dark, or dark text on light.",
            &["dark mode", "light mode", "theme", "night"],
            Kind::Choice(
                Scheme::ALL
                    .iter()
                    .map(|s| Choice::new(s.id(), s.label()))
                    .collect(),
            ),
            Value::Text(default.scheme.id().into()),
        ),
        account(
            "appearance.accent",
            "Accent colour",
            "The colour of focus, selection and the main button.",
            &["color", "highlight", "theme"],
            Kind::Choice(
                Accent::ALL
                    .iter()
                    .map(|a| Choice::new(a.id(), a.label()))
                    .collect(),
            ),
            Value::Text(default.accent.id().into()),
        ),
        account(
            "appearance.text-size",
            "Text size",
            "How large text is in the menu, the popups and Alpymist's windows.",
            &["font", "size", "zoom", "bigger", "smaller"],
            Kind::Number {
                min: i64::from(FONT_SIZES.0),
                max: i64::from(FONT_SIZES.1),
                step: 1,
                unit: "px",
            },
            Value::Number(i64::from(default.font_size)),
        ),
    ]
}

fn load(env: &Env) -> ThemeFile {
    alpymist_theme::load_from(&[env.account(ACCOUNT), env.system(SYSTEM)]).file
}

/// The theme's value for a setting.
pub fn get(env: &Env, setting: &Setting) -> Value {
    let theme = load(env);
    match setting.id {
        "appearance.scheme" => Value::Text(theme.scheme.id().into()),
        "appearance.accent" => Value::Text(theme.accent.id().into()),
        _ => Value::Number(i64::from(theme.font_size)),
    }
}

/// Change the account's theme file. The rest of it is kept.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let mut theme = load(env);
    let value = value.unwrap_or(&setting.default);
    match setting.id {
        "appearance.scheme" => theme.scheme = value.as_text().unwrap_or_default().parse()?,
        "appearance.accent" => theme.accent = value.as_text().unwrap_or_default().parse()?,
        _ => {
            theme.font_size = u16::try_from(value.as_number().unwrap_or(16))
                .unwrap_or(16)
                .clamp(FONT_SIZES.0, FONT_SIZES.1);
        }
    }
    alpymist_theme::save(&theme, &env.account(ACCOUNT))
}

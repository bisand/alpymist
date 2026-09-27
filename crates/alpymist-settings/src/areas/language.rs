//! Language: what `LANG` says in the account's desktop, and whether the
//! translations that make it mean anything are installed.
//!
//! The language is the account's, in its `settings.toml`, and `alpymist
//! session` puts it in the desktop's environment as it starts it, and in the
//! environment of what D-Bus starts on the desktop's behalf. Until someone
//! chooses one, `LANG` is what Alpine's profile gives every login, `C.UTF-8`.
//! The keyboard layout is not a language, and stays under Keyboard.
//!
//! musl formats dates and messages in a language from `musl-locales`, which
//! the desktop depends on; the programs' own words come from Alpine's `-lang`
//! packages, which apk installs for every installed program once `lang` is.
//! That is every language at once, about 30 MB to download, so it is a switch
//! of its own and off until turned on.
//!
//! The languages offered are those written in Latin, Greek or Cyrillic
//! letters, which the fonts installed draw; one in another script would be
//! boxes.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use crate::values::Values;

/// The account's values.
pub const VALUES: &str = "alpymist/settings.toml";
/// The language's id.
pub const LANGUAGE: &str = "language.language";
/// The translations' id.
pub const TRANSLATIONS: &str = "language.translations";
/// apk's world, which says whether `lang` was asked for.
pub const WORLD: &str = "etc/apk/world";
/// `LANG` when nobody has chosen, as Alpine's profile sets it.
const PLAIN: &str = "C.UTF-8";

/// Each language: its `LANG`, its name in itself, and in English.
const LANGUAGES: &[(&str, &str, &str)] = &[
    ("cs_CZ.UTF-8", "Čeština", "Czech"),
    ("da_DK.UTF-8", "Dansk", "Danish"),
    ("de_DE.UTF-8", "Deutsch (Deutschland)", "German (Germany)"),
    ("de_CH.UTF-8", "Deutsch (Schweiz)", "German (Switzerland)"),
    ("el_GR.UTF-8", "Ελληνικά", "Greek"),
    ("en_GB.UTF-8", "English (United Kingdom)", ""),
    ("en_US.UTF-8", "English (United States)", ""),
    ("es_ES.UTF-8", "Español (España)", "Spanish (Spain)"),
    ("es_MX.UTF-8", "Español (México)", "Spanish (Mexico)"),
    ("et_EE.UTF-8", "Eesti", "Estonian"),
    ("fi_FI.UTF-8", "Suomi", "Finnish"),
    ("fr_FR.UTF-8", "Français (France)", "French (France)"),
    ("fr_CA.UTF-8", "Français (Canada)", "French (Canada)"),
    ("hr_HR.UTF-8", "Hrvatski", "Croatian"),
    ("hu_HU.UTF-8", "Magyar", "Hungarian"),
    ("is_IS.UTF-8", "Íslenska", "Icelandic"),
    ("it_IT.UTF-8", "Italiano", "Italian"),
    ("lt_LT.UTF-8", "Lietuvių", "Lithuanian"),
    ("lv_LV.UTF-8", "Latviešu", "Latvian"),
    ("nb_NO.UTF-8", "Norsk bokmål", "Norwegian Bokmål"),
    ("nn_NO.UTF-8", "Norsk nynorsk", "Norwegian Nynorsk"),
    ("nl_NL.UTF-8", "Nederlands", "Dutch"),
    ("pl_PL.UTF-8", "Polski", "Polish"),
    ("pt_BR.UTF-8", "Português (Brasil)", "Portuguese (Brazil)"),
    (
        "pt_PT.UTF-8",
        "Português (Portugal)",
        "Portuguese (Portugal)",
    ),
    ("ro_RO.UTF-8", "Română", "Romanian"),
    ("ru_RU.UTF-8", "Русский", "Russian"),
    ("sk_SK.UTF-8", "Slovenčina", "Slovak"),
    ("sl_SI.UTF-8", "Slovenščina", "Slovenian"),
    ("sr_RS.UTF-8", "Српски", "Serbian"),
    ("sv_SE.UTF-8", "Svenska", "Swedish"),
    ("tr_TR.UTF-8", "Türkçe", "Turkish"),
    ("uk_UA.UTF-8", "Українська", "Ukrainian"),
];

/// The settings.
pub fn settings() -> Vec<Setting> {
    let mut choices = vec![Choice::new(PLAIN, "English (no region)")];
    choices.extend(LANGUAGES.iter().map(|&(lang, native, english)| {
        if english.is_empty() {
            Choice::new(lang, native)
        } else {
            Choice::new(lang, format!("{native} — {english}"))
        }
    }));
    vec![
        Setting {
            id: LANGUAGE,
            title: "Language",
            description: "The language programs speak, and how they write dates and numbers.",
            keywords: &["locale", "lang", "region", "translation", "formats"],
            kind: Kind::Choice(choices),
            default: Value::Text(PLAIN.into()),
            scope: Scope::Account,
            applies: Applies::NextLogin,
        },
        Setting {
            id: TRANSLATIONS,
            title: "Translations",
            description: "Install every program's translations, about 30 MB, \
                          so programs can speak the language above.",
            keywords: &["lang", "translate", "localisation", "localization"],
            kind: Kind::Switch,
            default: Value::Bool(false),
            scope: Scope::System,
            applies: Applies::NextLogin,
        },
    ]
}

/// Whether `lang` is in apk's world.
fn translated(env: &Env) -> bool {
    std::fs::read_to_string(env.system(WORLD)).is_ok_and(|w| w.lines().any(|l| l.trim() == "lang"))
}

/// Its value.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    if setting.id == TRANSLATIONS {
        return Ok(Value::Bool(translated(env)));
    }
    Ok(Values::load(&env.account(VALUES))?
        .get(setting.id)
        .unwrap_or_else(|| setting.default.clone()))
}

/// Set one, or reset it with `None`. Returns notes worth telling.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<Vec<String>, String> {
    if setting.id == TRANSLATIONS {
        let on = value.and_then(Value::as_bool).unwrap_or(false);
        if on {
            env.run(&["apk", "add", "lang"])?;
        } else if translated(env) {
            env.run(&["apk", "del", "lang"])?;
        }
        return Ok(Vec::new());
    }
    let path = env.account(VALUES);
    let mut values = Values::load(&path)?;
    match value {
        Some(v) => values.set(setting.id, v),
        None => values.remove(setting.id),
    }
    values.save(&path)?;
    let chosen = value.and_then(Value::as_text).unwrap_or(PLAIN);
    Ok(if chosen != PLAIN && !translated(env) {
        vec!["Most programs stay in English until Translations is turned on.".into()]
    } else {
        Vec::new()
    })
}

/// What the desktop's environment gets from these settings: `LANG`, when a
/// language was chosen.
///
/// # Errors
/// The account's values could not be read.
pub fn session_environment(env: &Env) -> Result<Vec<(&'static str, String)>, String> {
    Ok(Values::load(&env.account(VALUES))?
        .get(LANGUAGE)
        .and_then(|v| v.as_text().map(str::to_owned))
        .map(|lang| vec![("LANG", lang)])
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{LANGUAGE, LANGUAGES, TRANSLATIONS, WORLD, session_environment};
    use crate::env::Env;
    use crate::{Settings, Value};
    use std::sync::Mutex;

    fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("alpymist-language-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// Every language musl-locales 0.1.0 has, so every one musl itself
    /// formats dates in is offered.
    #[test]
    fn every_language_musl_formats_is_offered() {
        for lang in [
            "cs_CZ", "de_CH", "de_DE", "en_GB", "en_US", "es_ES", "fi_FI", "fr_FR", "it_IT",
            "nb_NO", "nl_NL", "pt_BR", "pt_PT", "ru_RU", "sr_RS", "sv_SE",
        ] {
            let lang = format!("{lang}.UTF-8");
            assert!(LANGUAGES.iter().any(|l| l.0 == lang), "{lang}");
        }
    }

    #[test]
    fn a_language_reaches_the_session_and_says_what_it_lacks() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("session");
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();
        assert!(session_environment(&env).unwrap().is_empty());

        let changed = settings.set(&env, LANGUAGE, "nb_NO.UTF-8", false).unwrap();
        assert!(
            changed.notes[0].contains("Translations"),
            "{:?}",
            changed.notes
        );
        assert_eq!(
            session_environment(&env).unwrap(),
            [("LANG", "nb_NO.UTF-8".to_owned())]
        );

        std::fs::create_dir_all(env.system("etc/apk")).unwrap();
        std::fs::write(env.system(WORLD), "alpymist-desktop\nlang\n").unwrap();
        let changed = settings.set(&env, LANGUAGE, "de_DE.UTF-8", false).unwrap();
        assert!(
            !changed.notes.iter().any(|n| n.contains("Translations")),
            "{:?}",
            changed.notes
        );

        settings.reset(&env, LANGUAGE, false).unwrap();
        assert!(session_environment(&env).unwrap().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn translations_are_lang_in_apks_world() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("lang");
        let env = Env::test(&d, true, &RAN);
        let settings = Settings::new();
        assert_eq!(settings.get(&env, TRANSLATIONS), Ok(Value::Bool(false)));
        settings.set(&env, TRANSLATIONS, "on", false).unwrap();
        // Not asked for: nothing to take away.
        settings.set(&env, TRANSLATIONS, "off", false).unwrap();
        std::fs::create_dir_all(env.system("etc/apk")).unwrap();
        std::fs::write(env.system(WORLD), "lang\n").unwrap();
        assert_eq!(settings.get(&env, TRANSLATIONS), Ok(Value::Bool(true)));
        settings.set(&env, TRANSLATIONS, "off", false).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            ["apk add lang", "apk del lang"]
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

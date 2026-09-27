//! The desktop tier: which desktop the login screen starts.
//!
//! greetd starts the configuration `/etc/conf.d/greetd` names, and each
//! tier's package ships its own (ADR 0001). Choosing a tier installs its
//! package when it is missing, rewrites that file as the installer wrote it,
//! and starts seatd at boot for the Wayland tiers, which open the screen
//! through it. greetd reads the file when it starts, so the change comes with
//! the next boot: restarting greetd now would end the session it is running.
//!
//! Automatic is what `alpymist probe` picks for this hardware. The probe takes
//! the better part of a second, so it is asked only when Automatic is chosen,
//! and its reasons are the page's to show, from the probe itself.
//!
//! Each tier's desktop is configured by files `/etc/skel` gives an account
//! when it is made, and an account made under another tier has none for the
//! new one: labwc would start with no bar and no wallpaper. So each account
//! with a home is given what `/etc/skel` has and it lacks, and nothing it has
//! is touched. That runs as the account, so what is in the home decides
//! nothing about where root writes.

use crate::env::Env;
use crate::generated;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};
use alpymist_core::{SessionBackend, Tier};

/// The tier's id.
pub const TIER_ID: &str = "desktop.tier";
/// greetd's service configuration, which names the tier's.
pub const GREETD: &str = "etc/conf.d/greetd";
/// The accounts.
pub const PASSWD: &str = "etc/passwd";
/// What the probe picks.
const AUTO: &str = "auto";

/// How a tier reads in the list.
fn label(tier: Tier) -> &'static str {
    match tier {
        Tier::Full => "Full — Hyprland, with animations and blur",
        Tier::Lite => "Lite — labwc, drawn by the graphics card",
        Tier::Potato => "Potato — labwc, drawn by the processor",
        Tier::Legacy => "Legacy — i3 on X11, for screens without KMS",
    }
}

/// The settings.
pub fn settings() -> Vec<Setting> {
    let mut choices = vec![Choice::new(AUTO, "Automatic — what this computer suits")];
    choices.extend(Tier::ALL.iter().map(|&t| Choice::new(t.id(), label(t))));
    vec![Setting {
        id: TIER_ID,
        title: "Desktop",
        description: "Which desktop the login screen starts, for everyone. \
                      A heavier one than the computer suits may not start at all.",
        keywords: &[
            "tier",
            "hyprland",
            "labwc",
            "i3",
            "session",
            "greetd",
            "login screen",
        ],
        kind: Kind::Choice(choices),
        default: Value::Text(AUTO.into()),
        scope: Scope::System,
        applies: Applies::NextBoot,
    }]
}

/// The tier `/etc/conf.d/greetd` starts.
pub fn get(env: &Env) -> Result<Value, String> {
    let path = env.system(GREETD);
    let text = std::fs::read_to_string(&path).map_err(|e| crate::io_error(&path, &e))?;
    let file = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("cfgfile="))
        .map(|v| v.trim_matches('"'));
    Tier::ALL
        .into_iter()
        .find(|t| Some(t.greeter_config()) == file)
        .map(|t| Value::Text(t.id().into()))
        .ok_or_else(|| format!("{} starts no Alpymist desktop", path.display()))
}

/// What the installer wrote, which is taken over as it stands.
fn adopt(text: &str) -> bool {
    Tier::ALL.into_iter().any(|t| {
        text == format!(
            "# Written by the Alpymist installer: the {t:?} tier's session.\n{}",
            t.greetd_service()
        )
    })
}

/// Choose a tier, or let the probe, as root. Says which the probe chose.
pub fn set(
    env: &Env,
    setting: &Setting,
    value: Option<&Value>,
    force: bool,
) -> Result<Vec<String>, String> {
    let chosen = value
        .unwrap_or(&setting.default)
        .as_text()
        .unwrap_or(AUTO)
        .to_owned();
    let tier = if chosen == AUTO {
        probed(env)?
    } else {
        Tier::ALL
            .into_iter()
            .find(|t| t.id() == chosen)
            .ok_or_else(|| format!("no tier `{chosen}`"))?
    };
    // Refused before anything is installed, rather than after.
    if !force && generated::state(&env.system(GREETD), "#", adopt)? == generated::State::HandEdited
    {
        return Err(format!(
            "{} was edited by hand, and is left as it is; --force replaces it",
            env.system(GREETD).display()
        ));
    }
    if !env
        .system(tier.greeter_config().trim_start_matches('/'))
        .exists()
    {
        env.run(&["apk", "add", tier.metapackage()])?;
    }
    generated::write(
        &env.system(GREETD),
        "#",
        "Settings",
        &tier.greetd_service(),
        adopt,
        force,
    )?;
    if !matches!(tier.backend(), SessionBackend::I3) {
        env.run(&["rc-update", "add", "seatd", "default"])?;
    }
    for (name, home) in accounts(env) {
        // A home it cannot be given to still has its own; the tier is chosen.
        let _ = env.run(&[
            "su",
            "-s",
            "/bin/sh",
            &name,
            "-c",
            "cp -Rn /etc/skel/. \"$1\"/",
            "sh",
            &home,
        ]);
    }
    Ok(if chosen == AUTO {
        vec![format!("The login screen will start {}.", label(tier))]
    } else {
        Vec::new()
    })
}

/// What the probe picks for this hardware.
fn probed(env: &Env) -> Result<Tier, String> {
    let said = env.run(&["alpymist", "probe", "--format", "json"])?;
    let doc: serde_json::Value =
        serde_json::from_str(&said).map_err(|e| format!("the probe said something else: {e}"))?;
    serde_json::from_value(doc["tier"].clone()).map_err(|e| format!("the probe named no tier: {e}"))
}

/// The people's accounts with a home: uid 1000 up to `nobody`'s.
fn accounts(env: &Env) -> Vec<(String, String)> {
    let passwd = std::fs::read_to_string(env.system(PASSWD)).unwrap_or_default();
    passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            let home = *f.get(5)?;
            ((1000..65534).contains(&uid) && env.system(home.trim_start_matches('/')).is_dir())
                .then(|| (f[0].to_owned(), home.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{GREETD, PASSWD, TIER_ID, accounts, adopt};
    use crate::env::Env;
    use crate::{Error, Settings, Value};
    use alpymist_core::Tier;
    use std::sync::Mutex;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("alpymist-tier-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    fn installed(tier: Tier) -> String {
        format!(
            "# Written by the Alpymist installer: the {tier:?} tier's session.\n{}",
            tier.greetd_service()
        )
    }

    #[test]
    fn what_the_installer_wrote_is_taken_over_and_nothing_else() {
        for t in Tier::ALL {
            assert!(adopt(&installed(t)), "{t:?}");
        }
        assert!(!adopt(&format!(
            "{}rc_after=\"net\"\n",
            installed(Tier::Lite)
        )));
    }

    #[test]
    fn the_people_with_a_home_are_given_the_new_desktops_files() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("accounts");
        let env = Env::test(&d, true, &RAN);
        std::fs::create_dir_all(env.system("home/someone")).unwrap();
        std::fs::create_dir_all(env.system("etc")).unwrap();
        std::fs::write(
            env.system(PASSWD),
            "root:x:0:0:root:/root:/bin/sh\n\
             someone:x:1000:1000::/home/someone:/bin/zsh\n\
             gone:x:1001:1001::/home/gone:/bin/zsh\n\
             nobody:x:65534:65534:nobody:/:/sbin/nologin\n",
        )
        .unwrap();
        assert_eq!(
            accounts(&env),
            [("someone".to_owned(), "/home/someone".to_owned())]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_tier_is_installed_named_to_greetd_and_given_to_the_accounts() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = dir("set");
        let settings = Settings::new();
        let env = Env::test(&d, true, &RAN);
        std::fs::create_dir_all(env.system("etc/conf.d")).unwrap();
        std::fs::create_dir_all(env.system("etc/greetd")).unwrap();
        std::fs::create_dir_all(env.system("home/someone")).unwrap();
        std::fs::write(
            env.system(PASSWD),
            "someone:x:1000:1000::/home/someone:/bin/zsh\n",
        )
        .unwrap();
        std::fs::write(env.system(GREETD), installed(Tier::Full)).unwrap();
        std::fs::write(env.system("etc/greetd/alpymist-full.toml"), "").unwrap();
        assert_eq!(settings.get(&env, TIER_ID), Ok(Value::Text("full".into())));

        // Lite's package is not installed: it is added.
        settings.set(&env, TIER_ID, "lite", false).unwrap();
        assert_eq!(settings.get(&env, TIER_ID), Ok(Value::Text("lite".into())));
        let conf = std::fs::read_to_string(env.system(GREETD)).unwrap();
        assert!(conf.contains("rc_need=\"seatd\""), "{conf}");
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "apk add alpymist-desktop-lite",
                "rc-update add seatd default",
                "su -s /bin/sh someone -c cp -Rn /etc/skel/. \"$1\"/ sh /home/someone",
            ]
        );

        // Legacy needs no seat, and a hand edit is left alone.
        std::fs::write(env.system("etc/greetd/alpymist-legacy.toml"), "").unwrap();
        settings.set(&env, TIER_ID, "legacy", false).unwrap();
        let conf = std::fs::read_to_string(env.system(GREETD)).unwrap();
        assert!(!conf.contains("seatd"), "{conf}");
        std::fs::write(env.system(GREETD), "cfgfile=\"/etc/greetd/mine.toml\"\n").unwrap();
        assert!(matches!(
            settings.set(&env, TIER_ID, "full", false),
            Err(Error::Failed(ref m)) if m.contains("by hand")
        ));
        assert!(settings.get(&env, TIER_ID).is_err());
        std::fs::remove_dir_all(&d).ok();
    }
}

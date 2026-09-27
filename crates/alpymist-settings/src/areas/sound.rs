//! Sound: where it plays and records, and how loud, through `wireplumber`.
//!
//! Nothing here is kept by Alpymist. The session manager remembers the device
//! chosen and each device's volume itself, so every setting is read from and
//! written to the running session with `wpctl`, and is unavailable without one.
//!
//! A device is its node name, which stays the same from one boot to the next,
//! where the number `wpctl` shows does not. Automatic is no device chosen, and
//! the session manager picks.

use crate::env::Env;
use crate::model::{Applies, Choice, Kind, Scope, Setting, Value};

/// The output device's id.
pub const OUTPUT: &str = "sound.output";
/// The input device's id.
pub const INPUT: &str = "sound.input";
/// No device chosen.
const AUTO: &str = "auto";

/// Playback or recording: where `wpctl` shows it, and what it calls it.
#[derive(Clone, Copy)]
struct Way {
    /// The heading in `wpctl status`'s Audio section.
    section: &'static str,
    /// Its class under Default Configured Devices.
    class: &'static str,
    /// What `clear-default` knows it as.
    key: &'static str,
    /// The node that stands for the default in `wpctl`'s other commands.
    default: &'static str,
}

const PLAYBACK: Way = Way {
    section: "Sinks",
    class: "Audio/Sink",
    key: "0",
    default: "@DEFAULT_AUDIO_SINK@",
};

const RECORDING: Way = Way {
    section: "Sources",
    class: "Audio/Source",
    key: "1",
    default: "@DEFAULT_AUDIO_SOURCE@",
};

fn way(setting: &Setting) -> Way {
    if setting.id == INPUT || setting.id == "sound.input-volume" {
        RECORDING
    } else {
        PLAYBACK
    }
}

/// One line of a section: its number, whether it is the default now, and its
/// name or description.
#[derive(Debug, PartialEq, Eq)]
struct Node {
    id: u32,
    current: bool,
    name: String,
}

/// The nodes under `section` in the Audio part of `wpctl status`.
fn nodes(status: &str, section: &str) -> Vec<Node> {
    let mut audio = false;
    let mut under = "";
    let mut found = Vec::new();
    for line in status.lines() {
        if line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            audio = line.trim() == "Audio";
            continue;
        }
        if !audio {
            continue;
        }
        let body = line.trim_start_matches(|c: char| c.is_whitespace() || "│├└─".contains(c));
        if let Some(heading) = body.strip_suffix(':') {
            under = heading;
            continue;
        }
        if under != section || body.is_empty() {
            continue;
        }
        let (current, rest) = body
            .strip_prefix('*')
            .map_or((false, body), |r| (true, r.trim_start()));
        let Some((id, name)) = rest.split_once(". ") else {
            continue;
        };
        let Ok(id) = id.parse() else {
            continue;
        };
        let name = name.rfind(" [vol:").map_or(name, |at| &name[..at]);
        found.push(Node {
            id,
            current,
            name: name.trim().to_owned(),
        });
    }
    found
}

/// The device chosen for `class`, from `wpctl status`'s Default Configured
/// Devices.
fn configured(status: &str, class: &str) -> Option<String> {
    let (_, after) = status.split_once("Default Configured Devices:")?;
    after.lines().find_map(|line| {
        let mut words = line.split_whitespace().skip(1);
        (words.next() == Some(class))
            .then(|| words.next().map(str::to_owned))
            .flatten()
    })
}

/// A volume as `wpctl get-volume` says it, as a percentage: `Volume: 0.40`
/// is 40, and a volume above 100% reads as 100.
fn percent(said: &str) -> Option<i64> {
    let fraction: f64 = said
        .trim()
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    #[allow(clippy::cast_possible_truncation)]
    Some(((fraction * 100.0).round() as i64).clamp(0, 100))
}

/// The devices for `way` as choices: Automatic, then each by its description,
/// with its node name as the value.
fn devices(named: &str, described: &str, way: Way) -> Vec<Choice> {
    let labels = nodes(described, way.section);
    let mut choices = vec![Choice::new(AUTO, "Automatic")];
    for node in nodes(named, way.section) {
        let label = labels
            .iter()
            .find(|l| l.id == node.id)
            .map_or_else(|| node.name.clone(), |l| l.name.clone());
        choices.push(Choice::new(node.name, label));
    }
    choices
}

/// The settings.
///
/// The devices are those connected when this is called, found from the
/// running session; with none to ask, Automatic is the only choice.
pub fn settings() -> Vec<Setting> {
    let env = Env::detect();
    let named = env.run(&["wpctl", "status", "-n"]).unwrap_or_default();
    let described = if named.is_empty() {
        String::new()
    } else {
        env.run(&["wpctl", "status"]).unwrap_or_default()
    };
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
    let volume = Kind::Number {
        min: 0,
        max: 100,
        step: 1,
        unit: "%",
    };
    vec![
        account(
            OUTPUT,
            "Output device",
            "Where sound plays. Automatic leaves it to the system, which prefers \
             what it rates best of what is connected.",
            &[
                "speakers",
                "headphones",
                "headset",
                "hdmi",
                "usb",
                "sink",
                "audio",
            ],
            Kind::Choice(devices(&named, &described, PLAYBACK)),
            Value::Text(AUTO.into()),
        ),
        account(
            "sound.volume",
            "Output volume",
            "How loud the output device plays.",
            &["loud", "quiet", "speakers", "audio", "level"],
            volume.clone(),
            Value::Number(40),
        ),
        account(
            INPUT,
            "Input device",
            "Which microphone records. Automatic leaves it to the system.",
            &["microphone", "mic", "headset", "recording", "source"],
            Kind::Choice(devices(&named, &described, RECORDING)),
            Value::Text(AUTO.into()),
        ),
        account(
            "sound.input-volume",
            "Input volume",
            "How much the input device picks up.",
            &["microphone", "mic", "gain", "recording", "level"],
            volume,
            Value::Number(100),
        ),
    ]
}

/// The device chosen, while it is connected, or Automatic; or a volume.
pub fn get(env: &Env, setting: &Setting) -> Result<Value, String> {
    let way = way(setting);
    if setting.id == OUTPUT || setting.id == INPUT {
        let status = env.run(&["wpctl", "status", "-n"])?;
        let chosen = configured(&status, way.class)
            .filter(|name| nodes(&status, way.section).iter().any(|n| &n.name == name));
        return Ok(Value::Text(chosen.unwrap_or_else(|| AUTO.into())));
    }
    let said = env.run(&["wpctl", "get-volume", way.default])?;
    percent(&said)
        .map(Value::Number)
        .ok_or_else(|| format!("wpctl said `{}`, which is not a volume", said.trim()))
}

/// Choose a device, or Automatic, or set a volume, in the running session.
pub fn set(env: &Env, setting: &Setting, value: Option<&Value>) -> Result<(), String> {
    let way = way(setting);
    let value = value.unwrap_or(&setting.default);
    if setting.id == OUTPUT || setting.id == INPUT {
        let name = value.as_text().unwrap_or(AUTO);
        if name == AUTO {
            return env.run(&["wpctl", "clear-default", way.key]).map(drop);
        }
        let status = env.run(&["wpctl", "status", "-n"])?;
        let node = nodes(&status, way.section)
            .into_iter()
            .find(|n| n.name == name)
            .ok_or_else(|| format!("{name} is not connected"))?;
        return env
            .run(&["wpctl", "set-default", &node.id.to_string()])
            .map(drop);
    }
    let percent = format!("{}%", value.as_number().unwrap_or(0));
    env.run(&["wpctl", "set-volume", way.default, &percent])
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::{AUTO, Node, OUTPUT, PLAYBACK, RECORDING, configured, devices, nodes, percent};
    use crate::env::Env;
    use crate::{Settings, Value};
    use std::sync::Mutex;

    /// `wpctl status -n` on a laptop on its dock, trimmed.
    const NAMED: &str = "\
PipeWire 'pipewire-0' [1.6.8, someone@alpymist-x1, cookie:3072332955]
 └─ Clients:
        33. WirePlumber                         [1.6.8, someone@alpymist-x1, pid:2985]

Audio
 ├─ Devices:
 │      49. alsa_card.pci-0000_00_1f.3          [alsa]
 │
 ├─ Sinks:
 │      50. alsa_output.pci-0000_00_1f.3.analog-stereo [vol: 0.40]
 │  *   64. alsa_output.usb-Lenovo_Dock_USB_Audio-00.analog-stereo [vol: 0.40]
 │
 ├─ Sources:
 │  *   51. alsa_input.pci-0000_00_1f.3.analog-stereo [vol: 1.00]
 │      65. alsa_input.usb-Lenovo_Dock_USB_Audio-00.mono-fallback [vol: 1.00]
 │
 ├─ Filters:
 │
 └─ Streams:

Video
 ├─ Devices:
 │      35. v4l2_device.pci-0000_00_14.0-usb-0_8_1.0 [v4l2]
 │
 ├─ Sinks:
 │      99. not.an.audio.sink
 │
 ├─ Sources:
 │  *   56. v4l2_input.pci-0000_00_14.0-usb-0_8_1.0
 │
 ├─ Filters:
 │
 └─ Streams:

Settings
 └─ Default Configured Devices:
         0. Audio/Sink    alsa_output.pci-0000_00_1f.3.analog-stereo
";

    /// The same, as plain `wpctl status` describes it.
    const DESCRIBED: &str = "\
Audio
 ├─ Sinks:
 │      50. Built-in Audio Analog Stereo        [vol: 0.40]
 │  *   64. ThinkPad Thunderbolt 3 Dock USB Audio Analog Stereo [vol: 0.40]
 │
 ├─ Sources:
 │  *   51. Built-in Audio Analog Stereo        [vol: 1.00]
 │      65. ThinkPad Thunderbolt 3 Dock USB Audio Mono [vol: 1.00]
 │
Video
";

    #[test]
    fn only_the_audio_sections_devices_are_read() {
        assert_eq!(
            nodes(NAMED, "Sinks"),
            [
                Node {
                    id: 50,
                    current: false,
                    name: "alsa_output.pci-0000_00_1f.3.analog-stereo".into()
                },
                Node {
                    id: 64,
                    current: true,
                    name: "alsa_output.usb-Lenovo_Dock_USB_Audio-00.analog-stereo".into()
                },
            ]
        );
        assert_eq!(nodes(NAMED, "Sources").len(), 2);
        assert!(nodes("", "Sinks").is_empty());
    }

    #[test]
    fn a_device_is_offered_by_its_description_and_kept_by_its_name() {
        let choices = devices(NAMED, DESCRIBED, PLAYBACK);
        assert_eq!(choices[0].value, AUTO);
        assert_eq!(
            choices[2].value,
            "alsa_output.usb-Lenovo_Dock_USB_Audio-00.analog-stereo"
        );
        assert_eq!(
            choices[2].label,
            "ThinkPad Thunderbolt 3 Dock USB Audio Analog Stereo"
        );
        assert_eq!(devices(NAMED, DESCRIBED, RECORDING).len(), 3);
        assert_eq!(devices("", "", PLAYBACK).len(), 1);
    }

    #[test]
    fn the_configured_device_is_read_by_its_class() {
        assert_eq!(
            configured(NAMED, "Audio/Sink").as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(configured(NAMED, "Audio/Source"), None);
    }

    #[test]
    fn volumes_are_percentages() {
        assert_eq!(percent("Volume: 0.40\n"), Some(40));
        assert_eq!(percent("Volume: 0.35 [MUTED]\n"), Some(35));
        assert_eq!(percent("Volume: 1.50\n"), Some(100));
        assert_eq!(percent("Could not connect"), None);
    }

    #[test]
    fn automatic_clears_the_choice_and_a_volume_is_set_on_the_default() {
        static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let d = std::env::temp_dir().join(format!("alpymist-sound-{}", std::process::id()));
        let env = Env::test(&d, false, &RAN);
        let settings = Settings::new();
        settings.reset(&env, OUTPUT, false).unwrap();
        settings.set(&env, "sound.input", AUTO, false).unwrap();
        settings.set(&env, "sound.volume", "55", false).unwrap();
        assert_eq!(
            RAN.lock().unwrap().as_slice(),
            [
                "wpctl clear-default 0",
                "wpctl clear-default 1",
                "wpctl set-volume @DEFAULT_AUDIO_SINK@ 55%",
            ]
        );
        assert!(settings.set(&env, "sound.volume", "120", false).is_err());
        assert_eq!(settings.get(&env, OUTPUT), Ok(Value::Text(AUTO.into())));
    }
}

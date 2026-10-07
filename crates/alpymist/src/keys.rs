//! `alpymist key`: what a keyboard's control keys do. Volume, the microphone,
//! the screen's and the keyboard's light, whatever is playing, and the
//! radios.
//!
//! The packaged `hyprland-keys.conf` binds each key to one of these, on every
//! machine alike: a key a keyboard does not have is a bind never pressed.
//! Each says what it did in a notification that replaces the one before it,
//! since most of these have nothing else on screen to show them.
//!
//! Nothing here is root's. Sound is the session's own; a backlight is written
//! by the `video` group, which the packaged udev rule gives it to; Wi-Fi is
//! iwd's to switch, for the `netdev` group, and Bluetooth is bluetoothd's.
//! That is why a radio is switched off by its daemon and not by rfkill,
//! which belongs to root.

use clap::ValueEnum;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a control key asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Key {
    /// Louder, and no longer muted.
    VolumeUp,
    /// Quieter.
    VolumeDown,
    /// Mute the sound, or let it be heard again.
    Mute,
    /// Mute the microphone, or let it hear again.
    MicMute,
    /// A brighter screen.
    BrightnessUp,
    /// A dimmer screen, never a dark one.
    BrightnessDown,
    /// A brighter keyboard.
    KeyboardLightUp,
    /// A dimmer keyboard, down to dark.
    KeyboardLightDown,
    /// The keyboard's light on or off.
    KeyboardLight,
    /// Play, or pause, whatever is playing.
    PlayPause,
    /// The next track.
    Next,
    /// The previous track.
    Previous,
    /// Stop what is playing.
    Stop,
    /// Wi-Fi on or off.
    Wifi,
    /// Bluetooth on or off.
    Bluetooth,
    /// Every radio off, and back to how they were.
    Airplane,
}

/// The speakers, to `wpctl`.
const SINK: &str = "@DEFAULT_AUDIO_SINK@";
/// The microphone, to `wpctl`.
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

/// `alpymist key KEY`.
pub fn run(key: Key) -> Result<(), Box<dyn std::error::Error>> {
    let said = press(key).inspect_err(|e| {
        // A key has no terminal to complain in.
        notify(&Said::plain("That key did nothing", e));
    })?;
    if let Some(said) = said {
        println!("{}", said.line());
        notify(&said);
    }
    Ok(())
}

/// Do what `key` asks, and return what to say about it.
fn press(key: Key) -> Result<Option<Said>, String> {
    let sys = Path::new("/sys/class");
    match key {
        Key::VolumeUp => {
            // A muted speaker made louder is one somebody wants to hear.
            wpctl(&["set-mute", SINK, "0"])?;
            wpctl(&["set-volume", "--limit", "1.0", SINK, "5%+"])?;
            volume("Volume", SINK).map(Some)
        }
        Key::VolumeDown => {
            wpctl(&["set-volume", SINK, "5%-"])?;
            volume("Volume", SINK).map(Some)
        }
        Key::Mute => {
            wpctl(&["set-mute", SINK, "toggle"])?;
            volume("Volume", SINK).map(Some)
        }
        Key::MicMute => {
            wpctl(&["set-mute", SOURCE, "toggle"])?;
            volume("Microphone", SOURCE).map(Some)
        }
        Key::BrightnessUp | Key::BrightnessDown => {
            let light = screen(sys).ok_or("This machine has no screen brightness to change")?;
            let to = stepped(light.now, light.max, key == Key::BrightnessUp, 1);
            light.set(to)?;
            Ok(Some(Said::level("Brightness", percent(to, light.max))))
        }
        Key::KeyboardLightUp | Key::KeyboardLightDown | Key::KeyboardLight => {
            let light = keyboard(sys).ok_or("This keyboard has no light")?;
            let to = match key {
                Key::KeyboardLight if light.now > 0 => 0,
                Key::KeyboardLight => light.max,
                _ => stepped(light.now, light.max, key == Key::KeyboardLightUp, 0),
            };
            light.set(to)?;
            Ok(Some(Said::level("Keyboard light", percent(to, light.max))))
        }
        Key::PlayPause | Key::Next | Key::Previous | Key::Stop => {
            let verb = match key {
                Key::PlayPause => "play-pause",
                Key::Next => "next",
                Key::Previous => "previous",
                _ => "stop",
            };
            // Nothing playing is nothing to say: the key is pressed by
            // accident as often as not.
            let _ = quiet("playerctl", &[verb]);
            Ok(None)
        }
        Key::Wifi => {
            let on = wifi().ok_or("This machine has no Wi-Fi to switch")?;
            set_wifi(!on)?;
            Ok(Some(radio_said("Wi-Fi", !on)))
        }
        Key::Bluetooth => {
            let on = bluetooth().ok_or(
                "Bluetooth is not running: it is turned on in Settings \u{203a} Bluetooth",
            )?;
            set_bluetooth(!on)?;
            Ok(Some(radio_said("Bluetooth", !on)))
        }
        Key::Airplane => airplane().map(Some),
    }
}

/// What a notification says: a title, a line under it, and how full its bar
/// is, for what has a level.
#[derive(Debug, PartialEq, Eq)]
struct Said {
    title: String,
    body: String,
    level: Option<u32>,
}

impl Said {
    fn plain(title: &str, body: &str) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            level: None,
        }
    }

    fn level(title: &str, percent: u32) -> Self {
        Self {
            title: format!("{title} {percent}%"),
            body: String::new(),
            level: Some(percent),
        }
    }

    /// The same on one line, for a terminal.
    fn line(&self) -> String {
        if self.body.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, self.body)
        }
    }
}

fn radio_said(name: &str, on: bool) -> Said {
    Said::plain(name, if on { "On" } else { "Off" })
}

/// A string in `GVariant`'s text format, which `gdbus call` reads its
/// arguments in.
fn gvariant_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The arguments to `gdbus` that put `said` up, in place of whatever a key
/// said last, for a second and a half.
fn notification(said: &Said) -> Vec<String> {
    let mut args: Vec<String> = [
        "call",
        "--session",
        "--timeout",
        "2",
        "--dest",
        "org.freedesktop.Notifications",
        "--object-path",
        "/org/freedesktop/Notifications",
        "--method",
        "org.freedesktop.Notifications.Notify",
        // Whatever follows is the method's arguments, not options.
        "--",
        "'Alpymist'",
        "0",
        "''",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(gvariant_string(&said.title));
    args.push(gvariant_string(&said.body));
    args.push("[]".into());
    // One notification for all the keys: a held key moves a bar and does not
    // fill the screen with a column of them. `value` is the bar.
    let mut hints = "{'x-canonical-private-synchronous': <'alpymist-key'>".to_owned();
    if let Some(level) = said.level {
        let _ = write!(hints, ", 'value': <int32 {level}>");
    }
    hints.push('}');
    args.push(hints);
    args.push("1500".into());
    args
}

/// Put `said` up. A session with nothing showing notifications still has
/// its keys.
fn notify(said: &Said) {
    let _ = Command::new("gdbus")
        .args(notification(said))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Put a title and a line under it up, for a key of another command's.
pub fn say(title: &str, body: &str) {
    notify(&Said::plain(title, body));
}

/// Run `program`, and return what it printed.
fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{program} {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `program` for what it does.
fn quiet(program: &str, args: &[&str]) -> Result<(), String> {
    output(program, args).map(drop)
}

fn wpctl(args: &[&str]) -> Result<(), String> {
    quiet("wpctl", args).map_err(|_| "Sound is not running".to_owned())
}

/// What `wpctl get-volume` said: `Volume: 0.40`, or `Volume: 0.40 [MUTED]`,
/// as a percentage and whether it is muted.
fn parse_volume(said: &str) -> Option<(u32, bool)> {
    let mut words = said.split_whitespace();
    let volume: f64 = words.nth(1)?.parse().ok()?;
    let muted = words.next() == Some("[MUTED]");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (volume.clamp(0.0, 10.0) * 100.0).round() as u32;
    Some((percent, muted))
}

/// How loud `node` is now, as a notification.
fn volume(name: &str, node: &str) -> Result<Said, String> {
    let said = output("wpctl", &["get-volume", node]).map_err(|_| "Sound is not running")?;
    let (percent, muted) = parse_volume(&said).ok_or("Sound is not running")?;
    Ok(if muted {
        Said {
            title: format!("{name} muted"),
            body: String::new(),
            level: Some(0),
        }
    } else {
        Said::level(name, percent.min(100))
    })
}

/// A light the kernel has a number for: a screen's backlight, or a
/// keyboard's.
#[derive(Debug, PartialEq, Eq)]
struct Light {
    /// Its directory under `/sys/class`.
    dir: PathBuf,
    now: u32,
    max: u32,
}

impl Light {
    /// The light in `dir`, if it has a brightness and a most.
    fn read(dir: PathBuf) -> Option<Self> {
        let number = |name: &str| -> Option<u32> {
            std::fs::read_to_string(dir.join(name))
                .ok()?
                .trim()
                .parse()
                .ok()
        };
        let (now, max) = (number("brightness")?, number("max_brightness")?);
        (max > 0).then_some(Self { dir, now, max })
    }

    fn set(&self, to: u32) -> Result<(), String> {
        std::fs::write(self.dir.join("brightness"), to.to_string()).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                "This account may not change that light. It is the video group's, from \
                 the first restart after Alpymist was upgraded"
                    .to_owned()
            } else {
                format!("{}: {e}", self.dir.display())
            }
        })
    }
}

/// The lights in `class`, in name order, that `keep` lets through.
fn lights(class: &Path, keep: impl Fn(&str) -> bool) -> Vec<Light> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(class)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(&keep))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter().filter_map(Light::read).collect()
}

/// The screen's backlight. Where a machine has more than one, the kernel
/// says of each how it reaches the panel, and the firmware's own way is
/// taken before a platform driver's, and that before a graphics card's
/// register: the order the kernel's documentation gives.
fn screen(sys: &Path) -> Option<Light> {
    let rank = |light: &Light| {
        let kind = std::fs::read_to_string(light.dir.join("type")).unwrap_or_default();
        match kind.trim() {
            "firmware" => 0,
            "platform" => 1,
            _ => 2,
        }
    };
    lights(&sys.join("backlight"), |_| true)
        .into_iter()
        .min_by_key(rank)
}

/// The keyboard's backlight: `tpacpi::kbd_backlight` on a `ThinkPad`,
/// `smc::kbd_backlight` on a Mac, `dell::kbd_backlight`, and so on.
fn keyboard(sys: &Path) -> Option<Light> {
    lights(&sys.join("leds"), |name| name.contains("kbd_backlight"))
        .into_iter()
        .next()
}

/// One step from `now` towards `max` or towards `floor` percent of it. A
/// twentieth of the range at a time, and a hundredth in the lowest tenth,
/// where a twentieth is the difference between dim and dark. A light with
/// few levels, as a keyboard's three, moves one level.
fn stepped(now: u32, max: u32, up: bool, floor_percent: u32) -> u32 {
    let fine = now < max / 10 || (!up && now == max / 10);
    let step = if fine { max / 100 } else { max / 20 }.max(1);
    // A screen never goes to nothing: on many panels that is off, with no
    // way to see what key brings it back.
    let floor = (u64::from(max) * u64::from(floor_percent)).div_ceil(100);
    let floor = u32::try_from(floor).unwrap_or(max);
    if up {
        now.saturating_add(step).min(max)
    } else {
        now.saturating_sub(step).max(floor)
    }
}

fn percent(now: u32, max: u32) -> u32 {
    let percent = (u64::from(now) * 100 + u64::from(max) / 2) / u64::from(max.max(1));
    u32::try_from(percent).unwrap_or(100)
}

/// Whether Wi-Fi is on, from what `alpymist-wifi status` said; `None` where
/// there is no adapter, or no iwd.
fn parse_wifi(status: &str) -> Option<bool> {
    match status.lines().next().unwrap_or_default() {
        "Wi-Fi is off" => Some(false),
        line if line.starts_with("Wi-Fi: ") => None,
        _ => Some(true),
    }
}

fn wifi() -> Option<bool> {
    parse_wifi(&output("alpymist-wifi", &["status"]).ok()?)
}

fn set_wifi(on: bool) -> Result<(), String> {
    quiet("alpymist-wifi", &[if on { "on" } else { "off" }])
        .map_err(|_| "Wi-Fi could not be switched".to_owned())
}

/// Whether Bluetooth is on, from what `bluetoothctl show` said; `None`
/// where there is no adapter.
fn parse_bluetooth(show: &str) -> Option<bool> {
    show.lines()
        .find_map(|line| line.trim().strip_prefix("Powered: "))
        .map(|state| state == "yes")
}

/// `None` where bluetoothd is not running, which is how it ships: asking
/// `bluetoothctl` then would wait for it.
fn bluetooth() -> Option<bool> {
    quiet("rc-service", &["--quiet", "bluetooth", "status"]).ok()?;
    parse_bluetooth(&output("bluetoothctl", &["show"]).ok()?)
}

fn set_bluetooth(on: bool) -> Result<(), String> {
    quiet("bluetoothctl", &["power", if on { "on" } else { "off" }])
        .map_err(|_| "Bluetooth could not be switched".to_owned())
}

/// Which radios to switch, and which way, when the airplane key is pressed
/// with Wi-Fi and Bluetooth as given (`None` for one the machine does not
/// have, or does not run) and `were` the ones the last press switched off.
/// Any radio on is switched off; with none on, those the last press switched
/// off come back, or every radio where nothing was remembered.
fn airplane_plan(
    wifi: Option<bool>,
    bluetooth: Option<bool>,
    were: Option<(bool, bool)>,
) -> (bool, Option<bool>, Option<bool>) {
    let off = wifi == Some(true) || bluetooth == Some(true);
    if off {
        return (true, wifi.map(|_| false), bluetooth.map(|_| false));
    }
    let (wifi_was, bluetooth_was) = were.unwrap_or((true, true));
    (
        false,
        wifi.filter(|_| wifi_was).map(|_| true),
        bluetooth.filter(|_| bluetooth_was).map(|_| true),
    )
}

/// Where the radios the airplane key switched off are remembered, for as
/// long as the machine is up.
fn airplane_file() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    Some(PathBuf::from(dir).join("alpymist-airplane"))
}

fn airplane() -> Result<Said, String> {
    let (wifi, bluetooth) = (wifi(), bluetooth());
    if wifi.is_none() && bluetooth.is_none() {
        return Err("This machine has no radio to switch".into());
    }
    let file = airplane_file();
    let were = file
        .as_ref()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|text| (text.contains("wifi"), text.contains("bluetooth")));
    let (off, to_wifi, to_bluetooth) = airplane_plan(wifi, bluetooth, were);
    if let Some(file) = &file {
        if off {
            let mut on = Vec::new();
            if wifi == Some(true) {
                on.push("wifi");
            }
            if bluetooth == Some(true) {
                on.push("bluetooth");
            }
            let _ = std::fs::write(file, on.join("\n"));
        } else {
            let _ = std::fs::remove_file(file);
        }
    }
    if let Some(on) = to_wifi {
        set_wifi(on)?;
    }
    if let Some(on) = to_bluetooth {
        set_bluetooth(on)?;
    }
    Ok(Said::plain(
        "Airplane mode",
        if off {
            "On: the radios are off"
        } else {
            "Off: the radios are back"
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `/sys/class` holding the lights given: class, name, type,
    /// brightness, most.
    fn sys(name: &str, lights: &[(&str, &str, &str, u32, u32)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("alpymist-keys-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (class, light, kind, now, max) in lights {
            let dir = root.join(class).join(light);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
            std::fs::write(dir.join("brightness"), format!("{now}\n")).unwrap();
            std::fs::write(dir.join("max_brightness"), format!("{max}\n")).unwrap();
        }
        root
    }

    /// The packaged binds and this program are two halves of one thing: a
    /// key bound to a name this does not know is a key that does nothing,
    /// and says so only when pressed.
    #[test]
    fn every_key_the_package_binds_is_one_this_knows_and_the_other_way_round() {
        let conf = include_str!("../../../desktop/hypr/hyprland-keys.conf");
        let bound: Vec<&str> = conf
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once("exec, alpymist key "))
            .map(|(_, key)| key.trim())
            .collect();
        for key in &bound {
            assert!(
                Key::from_str(key, false).is_ok(),
                "hyprland-keys.conf binds `alpymist key {key}`, which is no key"
            );
        }
        for key in Key::value_variants() {
            let name = key.to_possible_value().unwrap();
            assert!(
                bound.contains(&name.get_name()),
                "nothing in hyprland-keys.conf is bound to {}",
                name.get_name()
            );
        }
        // On the lock screen too, and never a plain bind for those.
        for line in conf.lines().filter(|l| l.contains("alpymist key ")) {
            if !line.starts_with('#') {
                assert!(
                    line.starts_with("bindl") || line.starts_with("bindel"),
                    "{line}"
                );
            }
        }
        let security = include_str!("../../../desktop/hypr/hyprland-security.conf");
        assert!(
            security.contains("source = /usr/share/alpymist/hyprland-keys.conf"),
            "hyprland-security.conf does not source hyprland-keys.conf"
        );
    }

    /// The account is in `video` and not in `input`: the rule gives the
    /// lights to the first, and only the keyboard's among the LEDs.
    #[test]
    fn the_udev_rule_gives_the_lights_to_video_and_no_other_led() {
        let rules = include_str!("../../../desktop/udev/91-alpymist-backlight.rules");
        let rules: Vec<&str> = rules.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(rules.len(), 2);
        for rule in rules {
            assert!(rule.contains("chgrp video"), "{rule}");
            assert!(!rule.contains("input"), "{rule}");
            if rule.contains("SUBSYSTEM==\"leds\"") {
                assert!(rule.contains("KERNEL==\"*kbd_backlight*\""), "{rule}");
            }
        }
    }

    #[test]
    fn a_volume_is_read_as_wpctl_says_it() {
        assert_eq!(parse_volume("Volume: 0.40\n"), Some((40, false)));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some((100, true)));
        assert_eq!(parse_volume("Volume: 0.05"), Some((5, false)));
        assert_eq!(parse_volume(""), None);
        assert_eq!(parse_volume("Volume: loud"), None);
    }

    #[test]
    fn the_screen_never_steps_down_to_dark() {
        // The X1's panel.
        let mut now = 1060;
        let mut presses = 0;
        while stepped(now, 1060, false, 1) < now {
            now = stepped(now, 1060, false, 1);
            presses += 1;
        }
        assert_eq!(now, 11, "one percent of 1060, rounded up");
        assert!(presses < 40, "{presses} presses from full to dim");
        // And a panel with fifteen levels, as an old Mac's.
        assert_eq!(stepped(1, 15, false, 1), 1);
        assert_eq!(stepped(2, 15, false, 1), 1);
        assert_eq!(stepped(14, 15, true, 1), 15);
        assert_eq!(stepped(15, 15, true, 1), 15);
    }

    #[test]
    fn steps_are_fine_where_the_light_is_low() {
        assert_eq!(stepped(500, 1000, true, 1), 550);
        assert_eq!(stepped(500, 1000, false, 1), 450);
        assert_eq!(stepped(100, 1000, false, 1), 90, "into the lowest tenth");
        assert_eq!(stepped(90, 1000, true, 1), 100);
        assert_eq!(stepped(100, 1000, true, 1), 150);
        assert_eq!(stepped(50, 1000, false, 1), 40);
    }

    #[test]
    fn a_keyboard_with_three_levels_moves_one_and_goes_dark() {
        assert_eq!(stepped(0, 2, true, 0), 1);
        assert_eq!(stepped(1, 2, true, 0), 2);
        assert_eq!(stepped(2, 2, true, 0), 2);
        assert_eq!(stepped(1, 2, false, 0), 0);
        assert_eq!(stepped(0, 2, false, 0), 0);
        // A Mac's has 255.
        assert_eq!(stepped(255, 255, false, 0), 243);
        assert_eq!(stepped(2, 255, false, 0), 0);
    }

    #[test]
    fn a_percentage_is_rounded() {
        assert_eq!(percent(341, 1060), 32);
        assert_eq!(percent(1, 2), 50);
        assert_eq!(percent(1060, 1060), 100);
        assert_eq!(percent(0, 0), 0);
    }

    #[test]
    fn the_firmwares_backlight_is_taken_before_the_graphics_cards() {
        let root = sys(
            "screen",
            &[
                ("backlight", "nv_backlight", "raw", 50, 100),
                ("backlight", "apple_backlight", "platform", 7, 15),
                ("backlight", "broken", "firmware", 1, 0),
            ],
        );
        let light = screen(&root).unwrap();
        assert_eq!(light.dir, root.join("backlight/apple_backlight"));
        assert_eq!((light.now, light.max), (7, 15));
        light.set(9).unwrap();
        assert_eq!(screen(&root).unwrap().now, 9);
        assert_eq!(screen(&root.join("nowhere")), None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_keyboards_light_is_found_among_the_other_leds() {
        let root = sys(
            "keyboard",
            &[
                ("leds", "input0::capslock", "", 0, 1),
                ("leds", "smc::kbd_backlight", "", 0, 255),
                ("leds", "tpacpi::thinklight", "", 0, 1),
            ],
        );
        assert_eq!(
            keyboard(&root).unwrap().dir,
            root.join("leds/smc::kbd_backlight")
        );
        std::fs::remove_dir_all(&root).ok();
        let none = sys("no-keyboard", &[("leds", "input0::capslock", "", 0, 1)]);
        assert_eq!(keyboard(&none), None);
        std::fs::remove_dir_all(&none).ok();
    }

    #[test]
    fn the_radios_say_whether_they_are_on() {
        assert_eq!(parse_wifi("Wi-Fi is off\n"), Some(false));
        assert_eq!(parse_wifi("Wi-Fi: no adapter\n"), None);
        assert_eq!(parse_wifi("Wi-Fi: iwd is not running\n"), None);
        assert_eq!(parse_wifi("Home\n192.168.1.4\n"), Some(true));
        assert_eq!(parse_wifi("Joining Home…\n"), Some(true));
        assert_eq!(
            parse_bluetooth("Controller AA (public)\n\tPowered: yes\n"),
            Some(true)
        );
        assert_eq!(parse_bluetooth("\tPowered: no\n"), Some(false));
        assert_eq!(parse_bluetooth("No default controller available\n"), None);
    }

    #[test]
    fn airplane_mode_puts_back_only_what_it_switched_off() {
        // Wi-Fi on, Bluetooth off: both asked to be off, and only Wi-Fi
        // comes back.
        assert_eq!(
            airplane_plan(Some(true), Some(false), None),
            (true, Some(false), Some(false))
        );
        assert_eq!(
            airplane_plan(Some(false), Some(false), Some((true, false))),
            (false, Some(true), None)
        );
        // Nothing remembered, as after a restart: everything comes on.
        assert_eq!(
            airplane_plan(Some(false), Some(false), None),
            (false, Some(true), Some(true))
        );
        // A machine with no Bluetooth running is never asked about it.
        assert_eq!(
            airplane_plan(Some(true), None, None),
            (true, Some(false), None)
        );
        assert_eq!(
            airplane_plan(Some(false), None, None),
            (false, Some(true), None)
        );
    }

    #[test]
    fn one_notification_is_replaced_and_carries_the_level() {
        let args = notification(&Said::level("Volume", 40));
        assert!(args.contains(&"'Volume 40%'".to_owned()), "{args:?}");
        assert!(
            args.contains(
                &"{'x-canonical-private-synchronous': <'alpymist-key'>, 'value': <int32 40>}"
                    .to_owned()
            ),
            "{args:?}"
        );
        assert_eq!(args.last().map(String::as_str), Some("1500"));
        let args = notification(&Said::plain("Wi-Fi", "it's off"));
        assert!(args.contains(&"'it\\'s off'".to_owned()), "{args:?}");
        assert!(
            args.contains(&"{'x-canonical-private-synchronous': <'alpymist-key'>}".to_owned()),
            "{args:?}"
        );
    }
}

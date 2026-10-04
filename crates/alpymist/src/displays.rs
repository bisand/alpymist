//! `alpymist displays …`: the screens, and the layout remembered for the set
//! of them connected now. The work is in `alpymist-displays`.

use alpymist_displays::layout::{self, Layout, Layouts};
use alpymist_displays::screen::{self, Monitor};
use alpymist_displays::{hypr, watch};
use clap::Subcommand;

/// What to do with the screens.
#[derive(Subcommand)]
pub enum Action {
    /// List the screens connected, as they are and as their layout has them.
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Change one screen, now and in the layout for these screens.
    Set {
        /// The screen: its connector (`DP-3`), or part of its name
        /// (`S24E650`), as `list` shows them.
        screen: String,
        /// `1920x1080@60`, `1920x1080`, or `preferred`.
        #[arg(long)]
        mode: Option<String>,
        /// Its top left corner, in logical pixels: `1920,0`.
        #[arg(long, allow_hyphen_values = true)]
        position: Option<String>,
        /// How much larger everything is drawn: `1`, `1.25`, `2`.
        #[arg(long)]
        scale: Option<f64>,
        /// Turned clockwise: 0, 90, 180 or 270.
        #[arg(long)]
        rotate: Option<u16>,
        /// Mirrored left to right, after turning.
        #[arg(long)]
        flip: Option<bool>,
        /// Show things on it, or not.
        #[arg(long)]
        on: Option<bool>,
        /// Show the same as another screen, named as for SCREEN, or `none`
        /// for a picture of its own: a projector showing the laptop's.
        #[arg(long)]
        mirror: Option<String>,
    },
    /// Remember the screens as they are now as the layout for this set.
    Save,
    /// Put the layout for the screens connected now in place.
    Apply,
    /// Forget the layout for the screens connected now: they are extended
    /// again, and that remembered.
    Forget,
    /// Go to workspace N, 1 to 9, of the screen with the focus (Super+N):
    /// each screen has its own, unless Settings › Displays says otherwise.
    Workspace {
        /// 1 to 9.
        n: u32,
    },
    /// Take the focused window to workspace N of its screen (Super+Shift+N).
    Move {
        /// 1 to 9.
        n: u32,
    },
    /// Show every workspace of the screen with the focus, to pick one from
    /// (Super+Tab); again, and the overview goes.
    Overview,
    /// Follow screens as they come and go. `alpymist watchdog` does this,
    /// among its watches.
    #[command(hide = true)]
    Watch,
}

/// Do it.
///
/// # Errors
/// Hyprland could not be asked or told, or the layouts not kept.
#[allow(clippy::too_many_lines)] // one arm a subcommand
pub fn run(action: &Action) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        Action::Watch => Ok(watch::run()?),
        Action::Workspace { n } => Ok(alpymist_displays::switch(*n, false)?),
        Action::Move { n } => Ok(alpymist_displays::switch(*n, true)?),
        Action::Overview => Ok(alpymist_displays::overview()?),
        Action::Apply => {
            let plan = alpymist_displays::apply()?;
            if plan.new {
                println!("These screens were new together: extended, and remembered.");
            }
            Ok(())
        }
        Action::List { json } => list(*json),
        Action::Save => {
            let monitors = hypr::monitors()?;
            let mut current = Layout::current(&monitors);
            // A panel off for a closed lid is still on in the layout.
            let lid = alpymist_displays::lid_closed(std::path::Path::new(alpymist_displays::LID));
            for (m, o) in monitors.iter().zip(current.outputs.iter_mut()) {
                if lid && m.internal() {
                    o.enabled = true;
                }
            }
            layout::keep(current)?;
            println!("Remembered the layout for these screens.");
            Ok(())
        }
        Action::Forget => {
            let monitors = hypr::monitors()?;
            let path = layout::path();
            let mut all = Layouts::load(&path);
            if all.forget(&layout::key(screen::names(&monitors))) {
                all.save(&path)?;
            }
            alpymist_displays::apply()?;
            println!("Forgot the layout for these screens; they are extended again.");
            Ok(())
        }
        Action::Set {
            screen,
            mode,
            position,
            scale,
            rotate,
            flip,
            on,
            mirror,
        } => {
            let monitors = hypr::monitors()?;
            let names = screen::names(&monitors);
            let index = find(&monitors, &names, screen)?;
            let key = layout::key(names.iter().cloned());
            let mut current = Layouts::load(&layout::path())
                .find(&key)
                .cloned()
                .unwrap_or_else(|| Layout::extended(&monitors));
            let source = match mirror.as_deref() {
                None => None,
                Some("none") => Some(None),
                Some(other) => {
                    let j = find(&monitors, &names, other)?;
                    if j == index {
                        return Err("a screen cannot show the same as itself".into());
                    }
                    Some(Some(names[j].clone()))
                }
            };
            let output = current
                .output_mut(&names[index])
                .ok_or("that screen is not in the layout")?;
            if let Some(source) = source {
                output.mirror = source;
            }
            if let Some(mode) = mode {
                output.mode = mode_rule(mode, &monitors[index])?;
            }
            if let Some(position) = position {
                output.position = parse_position(position)?;
            }
            if let Some(scale) = scale {
                if !(0.25..=4.0).contains(scale) {
                    return Err("a scale is between 0.25 and 4".into());
                }
                output.scale = *scale;
            }
            if rotate.is_some() || flip.is_some() {
                let turns = match rotate {
                    Some(r) => turns(*r)?,
                    None => output.transform % 4,
                };
                let flipped = flip.unwrap_or(output.transform >= 4);
                output.transform = turns + if flipped { 4 } else { 0 };
            }
            if let Some(on) = on {
                output.enabled = *on;
            }
            if current.outputs.iter().all(|o| !o.enabled) {
                return Err("that would leave no screen on".into());
            }
            layout::keep(current)?;
            alpymist_displays::apply()?;
            Ok(())
        }
    }
}

/// Which of `monitors` `wanted` means: its connector, its whole name, or a
/// part of its name no other screen's has.
fn find(monitors: &[Monitor], names: &[String], wanted: &str) -> Result<usize, String> {
    if let Some(i) = monitors.iter().position(|m| m.name == wanted) {
        return Ok(i);
    }
    if let Some(i) = names.iter().position(|n| n == wanted) {
        return Ok(i);
    }
    let lower = wanted.to_lowercase();
    let matching: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_lowercase().contains(&lower))
        .map(|(i, _)| i)
        .collect();
    match matching.as_slice() {
        [i] => Ok(*i),
        [] => Err(format!(
            "no screen called {wanted}: `alpymist displays list` names them"
        )),
        _ => Err(format!(
            "{wanted} could be more than one screen: use its connector"
        )),
    }
}

/// A mode as asked for, as a rule writes it: one the screen offers, with
/// the refresh rate it offers when none was given.
fn mode_rule(wanted: &str, monitor: &Monitor) -> Result<String, String> {
    if wanted == "preferred" {
        return Ok(wanted.into());
    }
    let (size, rate) = match wanted.split_once('@') {
        Some((size, rate)) => (size, Some(rate.trim_end_matches("Hz"))),
        None => (wanted, None),
    };
    // How far each mode of that size is from the rate asked for: the nearest
    // within half a hertz, so that 59.94 is not taken for the 60 beside it,
    // and the first the screen lists when no rate was given.
    let want = rate.map(str::parse::<f64>);
    let offered = monitor
        .available_modes
        .iter()
        .filter_map(|offer| {
            let offer = offer.trim_end_matches("Hz");
            let (offered_size, offered_rate) = offer.split_once('@')?;
            if offered_size != size {
                return None;
            }
            let off = match &want {
                None => 0.0,
                Some(Ok(want)) => (want - offered_rate.parse::<f64>().ok()?).abs(),
                Some(Err(_)) => return None,
            };
            (off < 0.5).then_some((off, offer))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, offer)| offer.to_owned());
    offered.ok_or_else(|| {
        format!(
            "{} does not offer {wanted}; it offers {}",
            monitor.name,
            monitor.available_modes.join(", ")
        )
    })
}

/// What `--mode` can be given for the screen `wanted` names, for Tab to
/// offer: `preferred`, then each mode it offers, written as [`mode_rule`]
/// takes them. Only `preferred` when `wanted` names no one screen.
pub fn modes(monitors: &[Monitor], wanted: Option<&str>) -> Vec<String> {
    let mut found = vec!["preferred".to_owned()];
    let names = screen::names(monitors);
    if let Some(i) = wanted.and_then(|w| find(monitors, &names, w).ok()) {
        for offer in &monitors[i].available_modes {
            let offer = offer.trim_end_matches("Hz").to_owned();
            if !found.contains(&offer) {
                found.push(offer);
            }
        }
    }
    found
}

fn parse_position(text: &str) -> Result<[i32; 2], String> {
    let (x, y) = text
        .split_once([',', 'x'])
        .ok_or("a position is X,Y: `1920,0`")?;
    let n = |s: &str| {
        s.trim()
            .parse::<i32>()
            .map_err(|_| format!("{s} is not a number of pixels"))
    };
    Ok([n(x)?, n(y)?])
}

fn turns(degrees: u16) -> Result<u8, String> {
    match degrees {
        0 => Ok(0),
        90 => Ok(1),
        180 => Ok(2),
        270 => Ok(3),
        _ => Err("a screen turns by 0, 90, 180 or 270 degrees".into()),
    }
}

fn list(json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let monitors = hypr::monitors()?;
    let names = screen::names(&monitors);
    let key = layout::key(names.iter().cloned());
    let remembered = Layouts::load(&layout::path()).find(&key).cloned();
    if json {
        let screens: Vec<serde_json::Value> = monitors
            .iter()
            .zip(&names)
            .map(|(m, name)| {
                serde_json::json!({
                    "name": name,
                    "connector": m.name,
                    "internal": m.internal(),
                    "enabled": !m.disabled,
                    "mode": m.mode(),
                    "position": [m.x, m.y],
                    "scale": m.scale,
                    "transform": m.transform,
                    "modes": m.available_modes,
                })
            })
            .collect();
        let out = serde_json::json!({
            "screens": screens,
            "remembered": remembered.is_some(),
            "layout": remembered,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    for (m, name) in monitors.iter().zip(&names) {
        let state = if m.disabled {
            "off".to_owned()
        } else {
            let turned = match m.transform % 4 {
                1 => ", turned 90°",
                2 => ", turned 180°",
                3 => ", turned 270°",
                _ => "",
            };
            format!("{} at {},{}, scale {}{turned}", m.mode(), m.x, m.y, m.scale)
        };
        println!("{:<9} {name}\n          {state}", m.name);
    }
    println!(
        "{}",
        if remembered.is_some() {
            "This set of screens has a remembered layout."
        } else {
            "This set of screens has no remembered layout yet."
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{find, mode_rule, modes, parse_position, turns};
    use alpymist_displays::screen::Monitor;

    fn m(name: &str) -> Monitor {
        Monitor {
            name: name.into(),
            available_modes: vec!["1920x1080@60.00Hz".into(), "1920x1080@50.00Hz".into()],
            ..Monitor::default()
        }
    }

    #[test]
    fn a_screen_is_found_by_connector_or_by_part_of_its_name() {
        let monitors = [m("eDP-1"), m("DP-3"), m("DP-5")];
        let names = [
            "Panel".to_owned(),
            "Samsung S24E650 A".to_owned(),
            "Samsung S24C750 B".to_owned(),
        ];
        assert_eq!(find(&monitors, &names, "DP-5"), Ok(2));
        assert_eq!(find(&monitors, &names, "s24e650"), Ok(1));
        assert!(find(&monitors, &names, "Samsung").is_err(), "two of them");
        assert!(find(&monitors, &names, "LG").is_err());
    }

    #[test]
    fn tab_offers_a_screens_modes_as_they_are_taken() {
        let mut tv = m("HDMI-A-1");
        tv.available_modes = vec![
            "1920x1080@60.00Hz".into(),
            "1920x1080@59.94Hz".into(),
            "1920x1080@60.00Hz".into(),
            "1280x720@50.00Hz".into(),
        ];
        let monitors = [m("eDP-1"), tv];
        let offered = modes(&monitors, Some("HDMI-A-1"));
        assert_eq!(
            offered,
            [
                "preferred",
                "1920x1080@60.00",
                "1920x1080@59.94",
                "1280x720@50.00"
            ]
        );
        // Each means itself, the two rates a hair apart among them.
        for mode in &offered {
            assert_eq!(&mode_rule(mode, &monitors[1]).unwrap(), mode);
        }
        // No screen named yet, or none by that name: what any screen takes.
        assert_eq!(modes(&monitors, None), ["preferred"]);
        assert_eq!(modes(&monitors, Some("LG")), ["preferred"]);
    }

    #[test]
    fn modes_are_ones_the_screen_offers() {
        let s = m("DP-3");
        assert_eq!(mode_rule("1920x1080", &s).unwrap(), "1920x1080@60.00");
        assert_eq!(mode_rule("1920x1080@50", &s).unwrap(), "1920x1080@50.00");
        assert_eq!(mode_rule("preferred", &s).unwrap(), "preferred");
        assert!(mode_rule("3840x2160", &s).is_err());
        assert_eq!(parse_position("-1920,0").unwrap(), [-1920, 0]);
        assert_eq!(parse_position("1920x1080").unwrap(), [1920, 1080]);
        assert!(parse_position("left").is_err());
        assert_eq!(turns(270).unwrap(), 3);
        assert!(turns(45).is_err());
    }
}

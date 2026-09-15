//! Wi-Fi as the bar shows it, and as the command line prints it.
//!
//! Waybar runs `alpymist-wifi --waybar` as a custom module and reads a line of
//! JSON each time something changes: an icon, a tooltip, and a class the
//! stylesheet can colour. With no adapter the text is empty, and Waybar hides
//! the module, so a desktop without Wi-Fi shows no Wi-Fi icon.

use crate::model::{Radio, State, Station, band};
use std::fmt::Write as _;

/// Signal strength icons, 0 to 4 bars, from Nerd Font's Material Design set.
pub const BARS: [&str; 5] = [
    "\u{f092f}",
    "\u{f091f}",
    "\u{f0922}",
    "\u{f0925}",
    "\u{f0928}",
];
/// Wi-Fi off.
pub const OFF: &str = "\u{f092e}";
/// On, and joined to nothing.
pub const DISCONNECTED: &str = "\u{f092f}";
/// Joining.
pub const CONNECTING: &str = "\u{f0929}";

/// The icon for the bar.
#[must_use]
pub fn icon(state: &State) -> &'static str {
    match (state.radio, state.station) {
        (Radio::NoDaemon | Radio::NoAdapter, _) => "",
        (Radio::Off, _) => OFF,
        (Radio::On, Station::Connected | Station::Roaming) => {
            BARS[usize::from(state.link_bars().unwrap_or(0).min(4))]
        }
        (Radio::On, Station::Connecting) => CONNECTING,
        (Radio::On, _) => DISCONNECTED,
    }
}

/// A class for the bar's stylesheet.
#[must_use]
pub fn class(state: &State) -> &'static str {
    match (state.radio, state.station) {
        (Radio::NoDaemon, _) => "unavailable",
        (Radio::NoAdapter, _) => "no-adapter",
        (Radio::Off, _) => "off",
        (Radio::On, Station::Connected | Station::Roaming) => "connected",
        (Radio::On, Station::Connecting) => "connecting",
        (Radio::On, _) => "disconnected",
    }
}

/// A few lines saying where Wi-Fi is: the tooltip, and `alpymist-wifi status`.
#[must_use]
pub fn summary(state: &State) -> String {
    match (state.radio, state.station) {
        (Radio::NoDaemon, _) => "Wi-Fi: iwd is not running".into(),
        (Radio::NoAdapter, _) => "Wi-Fi: no adapter".into(),
        (Radio::Off, _) => "Wi-Fi is off".into(),
        (Radio::On, Station::Connecting) => format!(
            "Joining {}…",
            state.current.as_deref().unwrap_or("a network")
        ),
        (Radio::On, Station::Connected | Station::Roaming) => {
            let Some(link) = &state.link else {
                return state.current.clone().unwrap_or_default();
            };
            let mut out = link.name.clone();
            if let Some(ip) = &link.ipv4 {
                let _ = write!(out, "\n{ip}");
            }
            let mut details = Vec::new();
            if let Some(dbm) = link.rssi_dbm {
                details.push(format!("{dbm} dBm"));
            }
            if let Some(f) = link.frequency_mhz {
                details.push(band(f).to_owned());
            }
            if let Some(rate) = link.rx_mbit {
                details.push(format!("{rate} Mbit/s"));
            }
            if !details.is_empty() {
                let _ = write!(out, "\n{}", details.join("  ·  "));
            }
            out
        }
        (Radio::On, _) => "Wi-Fi is on, not joined to a network".into(),
    }
}

/// The line Waybar reads.
#[must_use]
pub fn waybar(state: &State) -> String {
    serde_json::json!({
        "text": icon(state),
        "tooltip": summary(state),
        "class": class(state),
        "alt": class(state),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{BARS, OFF, class, icon, summary, waybar};
    use crate::model::{Radio, State, Station, sample};

    #[test]
    fn a_joined_network_shows_its_bars_and_address() {
        let state = sample();
        assert_eq!(icon(&state), BARS[4]);
        assert_eq!(class(&state), "connected");
        let text = summary(&state);
        assert!(text.starts_with("Fjellheim\n192.168.1.42\n"), "{text}");
        assert!(text.contains("5 GHz"));
    }

    #[test]
    fn no_adapter_hides_the_module() {
        let state = State {
            radio: Radio::NoAdapter,
            ..State::default()
        };
        let line: serde_json::Value = serde_json::from_str(&waybar(&state)).unwrap();
        assert_eq!(line["text"], "");
    }

    #[test]
    fn off_is_shown_as_off_whatever_the_station_last_said() {
        let state = State {
            radio: Radio::Off,
            station: Station::Connected,
            ..State::default()
        };
        assert_eq!(icon(&state), OFF);
        assert_eq!(summary(&state), "Wi-Fi is off");
    }

    #[test]
    fn the_line_is_one_line_of_json() {
        let line = waybar(&sample());
        assert!(!line.contains('\n'));
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(parsed["tooltip"].as_str().unwrap().contains('\n'));
    }
}

//! What there is to know about Wi-Fi, as plain data.
//!
//! The backend fills a [`State`] from iwd; the popup, the bar and the command
//! line only ever read one. Nothing here talks to anything, so every way of
//! describing a network — its bars, its band, the words in a tooltip — is
//! tested without a radio.

/// Whether there is a radio, and whether it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Radio {
    /// iwd is not running, or not reachable from here.
    #[default]
    NoDaemon,
    /// iwd is running and sees no wireless adapter.
    NoAdapter,
    /// The adapter is there and switched off.
    Off,
    /// The adapter is there and on.
    On,
}

/// What the station is doing, in iwd's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Station {
    /// Not joined to anything.
    #[default]
    Disconnected,
    /// Joining, which includes waiting for an address.
    Connecting,
    /// Joined, with an address.
    Connected,
    /// Leaving.
    Disconnecting,
    /// Moving to another access point of the same network.
    Roaming,
}

impl Station {
    /// iwd's `State` property.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "connecting" | "connecting (auto)" => Self::Connecting,
            "connected" => Self::Connected,
            "disconnecting" => Self::Disconnecting,
            "roaming" | "ft-roaming" | "fw-roaming" => Self::Roaming,
            _ => Self::Disconnected,
        }
    }
}

/// How a network is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// No passphrase.
    Open,
    /// A passphrase: WPA, WPA2 or WPA3 Personal.
    Psk,
    /// A username and password or a certificate: WPA Enterprise, eduroam.
    Enterprise,
    /// WEP, which iwd lists and will not join.
    Wep,
}

impl Security {
    /// iwd's `Type` property.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "open" => Self::Open,
            "8021x" => Self::Enterprise,
            "wep" => Self::Wep,
            _ => Self::Psk,
        }
    }

    /// Whether joining it asks for anything.
    #[must_use]
    pub fn secured(self) -> bool {
        self != Self::Open
    }
}

/// A network in range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// iwd's object for it, which joining is asked of.
    pub path: String,
    /// Its name.
    pub name: String,
    /// How it is secured.
    pub security: Security,
    /// Its strongest access point, in dBm.
    pub signal_dbm: i16,
    /// iwd has joined it before and has what it needs to join it again.
    pub known: bool,
    /// iwd's object for the saved profile, which forgetting is asked of.
    pub known_path: Option<String>,
    /// It is the network joined now.
    pub connected: bool,
}

impl Network {
    /// Signal as 0 to 4 bars.
    #[must_use]
    pub fn bars(&self) -> u8 {
        bars(self.signal_dbm)
    }
}

/// The joined network, in more detail than a row in a list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Link {
    /// Its name.
    pub name: String,
    /// The address this machine has on it.
    pub ipv4: Option<String>,
    /// The access point's frequency, in MHz.
    pub frequency_mhz: Option<u32>,
    /// Its channel.
    pub channel: Option<u16>,
    /// The security actually negotiated, such as `WPA2-Personal`.
    pub security: Option<String>,
    /// Signal, in dBm.
    pub rssi_dbm: Option<i16>,
    /// Receive rate, in Mbit/s.
    pub rx_mbit: Option<u32>,
    /// Transmit rate, in Mbit/s.
    pub tx_mbit: Option<u32>,
    /// The 802.11 generation in use, such as `802.11ac`.
    pub mode: Option<String>,
}

/// Everything known about Wi-Fi at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    /// Whether there is a radio to use.
    pub radio: Radio,
    /// The wireless interface, such as `wlan0`.
    pub device: Option<String>,
    /// What the station is doing.
    pub station: Station,
    /// A scan is running.
    pub scanning: bool,
    /// The network joined or being joined, by name.
    pub current: Option<String>,
    /// Details of the joined network, once joined.
    pub link: Option<Link>,
    /// Networks in range, in iwd's order of preference.
    pub networks: Vec<Network>,
}

impl State {
    /// The network called `name`, if it is in range.
    #[must_use]
    pub fn network(&self, name: &str) -> Option<&Network> {
        self.networks.iter().find(|n| n.name == name)
    }

    /// The network at `path`, if it is in range.
    #[must_use]
    pub fn network_at(&self, path: &str) -> Option<&Network> {
        self.networks.iter().find(|n| n.path == path)
    }

    /// Networks to offer, in iwd's order of preference: everything in range
    /// except the one joined or being joined, which is shown on its own.
    pub fn others(&self) -> impl Iterator<Item = &Network> {
        let current = match self.station {
            Station::Disconnected => None,
            _ => self.current.as_deref(),
        };
        self.networks
            .iter()
            .filter(move |n| !n.connected && Some(n.name.as_str()) != current)
    }

    /// Signal as 0 to 4 bars for the joined network, if one is joined.
    #[must_use]
    pub fn link_bars(&self) -> Option<u8> {
        if self.station != Station::Connected {
            return None;
        }
        let dbm = self.link.as_ref().and_then(|l| l.rssi_dbm).or_else(|| {
            self.current
                .as_deref()
                .and_then(|c| self.network(c))
                .map(|n| n.signal_dbm)
        })?;
        Some(bars(dbm))
    }
}

/// dBm as 0 to 4 bars, on iwctl's thresholds so the two never disagree.
#[must_use]
pub fn bars(dbm: i16) -> u8 {
    match dbm {
        d if d >= -60 => 4,
        d if d >= -67 => 3,
        d if d >= -75 => 2,
        d if d >= -85 => 1,
        _ => 0,
    }
}

/// The band a frequency is in, as people say it.
#[must_use]
pub fn band(frequency_mhz: u32) -> &'static str {
    match frequency_mhz {
        0..3000 => "2.4 GHz",
        3000..5925 => "5 GHz",
        _ => "6 GHz",
    }
}

/// Why a passphrase cannot be used, if it cannot.
///
/// WPA passphrases are 8 to 63 printable ASCII characters. Anything else is
/// refused by the access point after a wait, which is a worse way to find out.
///
/// # Errors
/// What is wrong with it, for the person typing.
pub fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    let len = passphrase.chars().count();
    if !(8..=63).contains(&len) {
        return Err("A Wi-Fi passphrase is 8 to 63 characters.".into());
    }
    if !passphrase
        .chars()
        .all(|c| c.is_ascii() && !c.is_ascii_control())
    {
        return Err("A Wi-Fi passphrase uses only plain ASCII characters.".into());
    }
    Ok(())
}

/// The file iwd keeps a network's profile in, under `/var/lib/iwd`.
///
/// iwd's rule: a name of letters, digits, spaces, `-` and `_` is used as it
/// is; anything else is written as `=` and the name's bytes in hex, so no name
/// can escape the directory or collide with another.
#[must_use]
pub fn profile_name(name: &str, security: Security) -> String {
    use std::fmt::Write as _;
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    let stem = if plain {
        name.to_string()
    } else {
        let hex = name.bytes().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        });
        format!("={hex}")
    };
    let extension = match security {
        Security::Open => "open",
        Security::Psk => "psk",
        Security::Enterprise => "8021x",
        Security::Wep => "wep",
    };
    format!("{stem}.{extension}")
}

/// Stand-in state, for snapshots and tests.
#[must_use]
pub fn sample() -> State {
    use std::fmt::Write as _;
    let hex = |name: &str| {
        name.bytes().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    };
    let network = |name: &str, security, signal_dbm, known| Network {
        path: format!("/net/connman/iwd/0/3/{}_psk", hex(name)),
        name: name.into(),
        security,
        signal_dbm,
        known,
        known_path: known.then(|| format!("/net/connman/iwd/{}_psk", hex(name))),
        connected: false,
    };
    let mut home = network("Fjellheim", Security::Psk, -54, true);
    home.connected = true;
    State {
        radio: Radio::On,
        device: Some("wlan0".into()),
        station: Station::Connected,
        scanning: false,
        current: Some("Fjellheim".into()),
        link: Some(Link {
            name: "Fjellheim".into(),
            ipv4: Some("192.168.1.42".into()),
            frequency_mhz: Some(5240),
            channel: Some(48),
            security: Some("WPA2-Personal".into()),
            rssi_dbm: Some(-54),
            rx_mbit: Some(200),
            tx_mbit: Some(173),
            mode: Some("802.11ac".into()),
        }),
        networks: vec![
            home,
            network("Kaffebar Gjest", Security::Open, -63, false),
            network("Naboen sitt nett", Security::Psk, -71, false),
            network("eduroam", Security::Enterprise, -78, false),
            network("DIRECT-printer", Security::Psk, -88, false),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{Security, Station, band, bars, profile_name, sample, validate_passphrase};

    #[test]
    fn bars_follow_iwctls_thresholds() {
        assert_eq!(bars(-40), 4);
        assert_eq!(bars(-60), 4);
        assert_eq!(bars(-61), 3);
        assert_eq!(bars(-75), 2);
        assert_eq!(bars(-85), 1);
        assert_eq!(bars(-90), 0);
    }

    #[test]
    fn bands_are_named_by_frequency() {
        assert_eq!(band(2412), "2.4 GHz");
        assert_eq!(band(5240), "5 GHz");
        assert_eq!(band(5955), "6 GHz");
    }

    #[test]
    fn iwd_words_are_read() {
        assert_eq!(Station::parse("connected"), Station::Connected);
        assert_eq!(Station::parse("connecting (auto)"), Station::Connecting);
        assert_eq!(Station::parse("something new"), Station::Disconnected);
        assert_eq!(Security::parse("8021x"), Security::Enterprise);
        assert!(!Security::parse("open").secured());
    }

    #[test]
    fn the_joined_network_is_not_offered_again() {
        let state = sample();
        assert!(state.others().all(|n| n.name != "Fjellheim"));
        assert_eq!(state.others().count(), state.networks.len() - 1);
        assert_eq!(state.link_bars(), Some(4));
    }

    #[test]
    fn profiles_are_named_the_way_iwd_names_them() {
        assert_eq!(profile_name("Fjellheim", Security::Psk), "Fjellheim.psk");
        assert_eq!(
            profile_name("Kaffebar Gjest", Security::Open),
            "Kaffebar Gjest.open"
        );
        // The bytes as iwd's object path for this network spells them.
        assert_eq!(
            profile_name("Øvre Bogenvei 71", Security::Psk),
            "=c39876726520426f67656e766569203731.psk"
        );
        assert_eq!(profile_name("../etc", Security::Psk), "=2e2e2f657463.psk");
    }

    #[test]
    fn passphrases_are_checked_before_anything_is_tried() {
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase("correct horse").is_ok());
        assert!(validate_passphrase(&"x".repeat(64)).is_err());
        assert!(validate_passphrase("blåbærsyltetøy").is_err());
    }
}

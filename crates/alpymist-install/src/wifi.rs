//! Joining a Wi-Fi network from the installer, through iwd.
//!
//! A laptop with no Ethernet port installs offline unless it can join Wi-Fi,
//! and offline means no firmware beyond what the image carries and no updates.
//! So the Network screen lists what iwd can see and joins the chosen network
//! on the live system, before anything is written: a wrong passphrase is found
//! out here, where it can be retyped, and the network is known to work before
//! the installed system is told to use it.
//!
//! iwd is driven by `alpymist-wifi`, the desktop's Wi-Fi manager, over D-Bus:
//! the installer and the popup under the bar join networks the same way, and
//! iwd hands back its answers as data rather than as tables to parse. The
//! passphrase goes to iwd through an agent, and iwd writes the network's
//! profile itself once it has joined — the file the plan then copies onto the
//! installed system.
//!
//! What stays here is the screen's view of it: networks as the screen lists
//! them, and a worker thread, so scanning and joining never stall a frame.

use alpymist_wifi::iwd::Iwd;
use alpymist_wifi::model::{Security, State};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

pub use alpymist_wifi::model::validate_passphrase;

/// Where iwd keeps the networks it knows, on the live and installed systems.
pub const IWD_STATE: &str = "/var/lib/iwd";

/// A network iwd can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// Its name.
    pub ssid: String,
    /// Whether it needs a passphrase.
    pub secured: bool,
    /// Signal, from 0 to 4 bars.
    pub bars: u8,
}

/// Where joining a network has got to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    /// There is no Wi-Fi adapter, or no iwd to drive it.
    #[default]
    Unavailable,
    /// Looking for networks.
    Scanning,
    /// Networks have been listed; nothing joined yet.
    Ready,
    /// Joining this network.
    Connecting(String),
    /// Joined this network and have an address.
    Connected(String),
    /// Could not join this network, and why.
    Failed(String, String),
}

/// Everything the Network screen knows about Wi-Fi.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Wifi {
    /// The wireless interface, when there is one.
    pub adapter: Option<String>,
    /// What iwd can see, in iwd's order of preference.
    pub networks: Vec<Network>,
    /// Where joining has got to.
    pub status: Status,
    /// The passphrase typed for the chosen network.
    pub passphrase: String,
}

impl std::fmt::Debug for Wifi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wifi")
            .field("adapter", &self.adapter)
            .field("networks", &self.networks)
            .field("status", &self.status)
            .field(
                "passphrase",
                &if self.passphrase.is_empty() {
                    ""
                } else {
                    "<set>"
                },
            )
            .finish()
    }
}

impl Wifi {
    /// The network called `ssid`, if iwd can see it.
    #[must_use]
    pub fn network(&self, ssid: &str) -> Option<&Network> {
        self.networks.iter().find(|n| n.ssid == ssid)
    }

    /// Whether `ssid` is joined.
    #[must_use]
    pub fn is_connected_to(&self, ssid: &str) -> bool {
        matches!(&self.status, Status::Connected(s) if s == ssid)
    }
}

/// Stand-in networks, for previews, snapshots and tests.
#[must_use]
pub fn sample() -> Wifi {
    let network = |ssid: &str, secured, bars| Network {
        ssid: ssid.into(),
        secured,
        bars,
    };
    Wifi {
        adapter: Some("wlan0".into()),
        networks: vec![
            network("Fjellheim", true, 4),
            network("Kaffebar Gjest", false, 3),
            network("Naboen sitt nett", true, 2),
            network("DIRECT-printer", true, 1),
        ],
        status: Status::Ready,
        passphrase: String::new(),
    }
}

/// The file iwd keeps a network's settings in, under [`IWD_STATE`].
#[must_use]
pub fn profile_name(ssid: &str, secured: bool) -> String {
    let security = if secured {
        Security::Psk
    } else {
        Security::Open
    };
    alpymist_wifi::model::profile_name(ssid, security)
}

/// The networks the screen offers, from what iwd sees.
///
/// Networks needing a login rather than a passphrase — 802.1X, and WEP — are
/// left out: this screen cannot join them.
#[must_use]
pub fn networks(state: &State) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for n in &state.networks {
        let secured = match n.security {
            Security::Psk => true,
            Security::Open => false,
            Security::Enterprise | Security::Wep => continue,
        };
        if n.name.is_empty() || networks.iter().any(|seen| seen.ssid == n.name) {
            continue;
        }
        networks.push(Network {
            ssid: n.name.clone(),
            secured,
            bars: n.bars(),
        });
    }
    networks
}

/// This machine's first wireless interface, if it has one.
#[must_use]
pub fn adapter() -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/net")
        .ok()?
        .flatten()
        .filter(|entry| entry.path().join("wireless").is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names.into_iter().next()
}

/// This machine's first wired network port, if it has one.
///
/// A physical interface — one with a device behind it — that is not wireless.
/// That leaves out `lo`, bridges, tunnels and whatever a container runtime made.
#[must_use]
pub fn wired_interface() -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/net")
        .ok()?
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.join("device").exists() && !path.join("wireless").is_dir()
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names.into_iter().next()
}

/// What the screen asks the worker to do.
#[derive(Clone, PartialEq, Eq)]
pub enum Request {
    /// Look for networks.
    Scan,
    /// Join a network, with its passphrase if it is secured.
    Connect {
        /// The network.
        ssid: String,
        /// The passphrase, for a secured network.
        passphrase: Option<String>,
    },
}

/// What the worker reports back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A scan finished with these networks.
    Networks(Vec<Network>),
    /// Joined, with an address.
    Connected(String),
    /// Could not join, and why.
    Failed(String, String),
}

/// The screen's end of a running worker.
pub struct Link {
    /// Requests to the worker.
    pub commands: Sender<Request>,
    /// Results from it.
    pub events: Receiver<Event>,
}

/// How long a scan may take before its results are taken as they are.
const SCAN_TIMEOUT: Duration = Duration::from_secs(12);

/// How long to wait for an address after joining.
const ADDRESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Start a worker driving iwd.
///
/// Scanning and joining take seconds, and the installer keeps drawing while
/// they happen; so they run on a thread, and the screen collects results.
/// `adapter` is only for the log: iwd knows its own devices.
#[must_use]
pub fn spawn(adapter: String) -> Link {
    let (command_tx, command_rx) = channel::<Request>();
    let (event_tx, event_rx) = channel();
    std::thread::spawn(move || {
        let iwd = match Iwd::with_agent() {
            Ok(iwd) => Some(iwd),
            Err(e) => {
                eprintln!("wifi: {e}");
                None
            }
        };
        for command in command_rx {
            let event = match (&iwd, command) {
                (Some(iwd), Request::Scan) => Event::Networks(scan(iwd, &adapter)),
                (Some(iwd), Request::Connect { ssid, passphrase }) => {
                    connect(iwd, &adapter, ssid, passphrase)
                }
                (None, Request::Scan) => Event::Networks(Vec::new()),
                (None, Request::Connect { ssid, .. }) => {
                    Event::Failed(ssid, "iwd cannot be reached".into())
                }
            };
            if event_tx.send(event).is_err() {
                break;
            }
        }
    });
    Link {
        commands: command_tx,
        events: event_rx,
    }
}

fn scan(iwd: &Iwd, adapter: &str) -> Vec<Network> {
    let networks = networks(&iwd.scan_and_wait(SCAN_TIMEOUT));
    eprintln!("wifi: {} networks on {adapter}", networks.len());
    networks
}

fn connect(iwd: &Iwd, adapter: &str, ssid: String, passphrase: Option<String>) -> Event {
    let failed = |ssid: String, why: &str| Event::Failed(ssid, why.to_string());
    let mut state = iwd.state();
    if state.network(&ssid).is_none() {
        state = iwd.scan_and_wait(SCAN_TIMEOUT);
    }
    let Some(network) = state.network(&ssid).cloned() else {
        return failed(ssid, "the network is out of range");
    };

    // A profile iwd kept from an earlier try holds the passphrase typed then,
    // and iwd would use it rather than ask for the one typed now.
    if passphrase.is_some()
        && let Some(known) = &network.known_path
    {
        let _ = iwd.forget(known);
    }

    eprintln!("wifi: joining {ssid:?} on {adapter}");
    if let Err(e) = iwd.join(&network.path, passphrase) {
        return failed(ssid, &e.reason());
    }
    match iwd.wait_for_address(ADDRESS_TIMEOUT) {
        Some(address) => {
            eprintln!("wifi: joined {ssid:?} as {address}");
            Event::Connected(ssid)
        }
        None => failed(ssid, "no address after 30 seconds"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Network, networks, profile_name, validate_passphrase};
    use alpymist_wifi::model::{Security, sample};

    #[test]
    fn networks_are_listed_as_the_screen_needs_them() {
        let mut state = sample();
        // A second access point for a name iwd lists once is still one row.
        let mut twin = state.networks[1].clone();
        twin.path.push_str("_twin");
        state.networks.push(twin);
        let listed = networks(&state);
        assert_eq!(
            listed.first(),
            Some(&Network {
                ssid: "Fjellheim".into(),
                secured: true,
                bars: 4,
            })
        );
        assert!(
            listed.iter().all(|n| n.ssid != "eduroam"),
            "802.1X is left out: this screen cannot log in to it"
        );
        assert_eq!(
            listed.iter().filter(|n| n.ssid == "Kaffebar Gjest").count(),
            1
        );
        assert!(
            !listed
                .iter()
                .find(|n| n.ssid == "Kaffebar Gjest")
                .unwrap()
                .secured
        );
    }

    #[test]
    fn wep_is_left_out_too() {
        let mut state = sample();
        state.networks[2].security = Security::Wep;
        assert!(
            networks(&state)
                .iter()
                .all(|n| n.ssid != "Naboen sitt nett")
        );
    }

    /// iwd's own rule; a different name is a file iwd never reads.
    #[test]
    fn profiles_are_named_the_way_iwd_names_them() {
        assert_eq!(profile_name("Alpymist Test", true), "Alpymist Test.psk");
        assert_eq!(profile_name("kaffe_bar-2", false), "kaffe_bar-2.open");
        assert_eq!(profile_name("Blåbær", true), "=426cc3a562c3a672.psk");
        assert_eq!(
            profile_name("../../etc/passwd", true),
            "=2e2e2f2e2e2f6574632f706173737764.psk",
            "a name can never leave the directory"
        );
    }

    #[test]
    fn passphrases_are_checked_before_anything_is_tried() {
        assert!(validate_passphrase("correct horse battery").is_ok());
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase("line\nbreak!!").is_err());
    }
}

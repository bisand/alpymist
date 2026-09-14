//! Joining a Wi-Fi network from the installer, through iwd.
//!
//! A laptop with no Ethernet port installs offline unless it can join Wi-Fi,
//! and offline means no firmware beyond what the image carries and no updates.
//! So the Network screen lists what iwd can see and joins the chosen network
//! on the live system, before anything is written: a wrong passphrase is found
//! out here, where it can be retyped, and the network is known to work before
//! the installed system is told to use it.
//!
//! iwd has no machine-readable command line, only `iwctl`'s tables, so the
//! parsing lives here, pure and tested against captured output. The joining is
//! done by writing the network's profile — the same file the installed system
//! gets — and letting iwd act on it, which proved more dependable on a real
//! iwd than asking `iwctl` to connect: a connect issued a moment after the file
//! appears can run before iwd has read it.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

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
    /// What iwd can see, strongest first.
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

/// The file iwd keeps a network's settings in.
///
/// iwd's rule: a name of letters, digits, spaces, `-` and `_` is used as it
/// is; anything else is written as `=` and the name's bytes in hex, so no name
/// can escape the directory or collide with another.
#[must_use]
pub fn profile_name(ssid: &str, secured: bool) -> String {
    let plain = !ssid.is_empty()
        && ssid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    let stem = if plain {
        ssid.to_string()
    } else {
        let hex = ssid.bytes().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        });
        format!("={hex}")
    };
    format!("{stem}.{}", if secured { "psk" } else { "open" })
}

/// The contents of a network's profile.
#[must_use]
pub fn profile(passphrase: Option<&str>) -> String {
    passphrase.map_or_else(String::new, |p| format!("[Security]\nPassphrase={p}\n"))
}

/// The networks in `iwctl station <adapter> get-networks` output.
///
/// Columns are name, security and signal. The name may contain spaces, so a
/// row is read from the right. Signal is four stars with the missing bars drawn
/// dim, so bars are counted before the colour is stripped. Networks needing a
/// login rather than a passphrase — 802.1X, and WEP — are left out: this screen
/// cannot join them.
#[must_use]
pub fn parse_networks(output: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for raw in output.lines() {
        let bars = bright_stars(raw);
        let line = strip_ansi(raw);
        let line = line.trim().trim_start_matches('>').trim();
        let (rest, signal) = last_word(line);
        let (name, security) = last_word(rest);
        if signal.is_empty() || !signal.chars().all(|c| c == '*') {
            continue;
        }
        let secured = match security {
            "psk" => true,
            "open" => false,
            _ => continue,
        };
        let ssid = name.trim().to_string();
        if ssid.is_empty() || networks.iter().any(|n| n.ssid == ssid) {
            continue;
        }
        networks.push(Network {
            ssid,
            secured,
            bars: bars.min(4),
        });
    }
    // iwctl lists strongest first already; sorting again keeps that true if it
    // ever stops, and is stable, so equal signals keep iwctl's order.
    networks.sort_by_key(|n| std::cmp::Reverse(n.bars));
    networks
}

/// From `iwctl station <adapter> show`: whether it is connected, and its
/// IPv4 address once it has one.
#[must_use]
pub fn parse_station(output: &str) -> (bool, Option<String>) {
    let mut connected = false;
    let mut address = None;
    for line in output.lines().map(strip_ansi) {
        let line = line.trim();
        if let Some(state) = line.strip_prefix("State") {
            connected = state.trim() == "connected";
        } else if let Some(ip) = line.strip_prefix("IPv4 address") {
            let ip = ip.trim();
            if !ip.is_empty() {
                address = Some(ip.to_string());
            }
        }
    }
    (connected, address)
}

/// `text` split before its last word, with the whitespace between dropped.
fn last_word(text: &str) -> (&str, &str) {
    let text = text.trim_end();
    match text.rfind(char::is_whitespace) {
        Some(at) => (text[..at].trim_end(), text[at..].trim_start()),
        None => ("", text),
    }
}

/// `text` without terminal colour sequences.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI: ESC [ parameters, ending in a letter.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Stars drawn in the normal colour: the bars a signal has. iwctl draws the
/// missing ones dim, with `90` (bright black) in their colour sequence.
fn bright_stars(text: &str) -> u8 {
    let mut count = 0u8;
    let mut dim = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            let mut sequence = String::new();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
                sequence.push(c);
            }
            dim = sequence
                .trim_start_matches('[')
                .split(';')
                .any(|part| part == "90");
        } else if c == '*' && !dim {
            count = count.saturating_add(1);
        }
    }
    count
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

/// How long to wait for an address after joining.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Start a worker driving iwd on `adapter`.
///
/// Scanning and joining take seconds, and the installer keeps drawing while
/// they happen; so they run on a thread, and the screen collects results.
#[must_use]
pub fn spawn(adapter: String) -> Link {
    let (command_tx, command_rx) = channel::<Request>();
    let (event_tx, event_rx) = channel();
    std::thread::spawn(move || {
        for command in command_rx {
            let event = match command {
                Request::Scan => Event::Networks(scan(&adapter)),
                Request::Connect { ssid, passphrase } => {
                    connect(&adapter, &ssid, passphrase.as_deref())
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

fn iwctl(args: &[&str]) -> Option<(bool, String)> {
    let output = std::process::Command::new("iwctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some((output.status.success(), text))
}

fn scan(adapter: &str) -> Vec<Network> {
    let _ = iwctl(&["station", adapter, "scan"]);
    // A scan takes a few seconds and iwctl does not wait for it.
    let mut networks = Vec::new();
    for _ in 0..4 {
        std::thread::sleep(Duration::from_secs(2));
        if let Some((_, text)) = iwctl(&["station", adapter, "get-networks"]) {
            networks = parse_networks(&text);
            if !networks.is_empty() {
                break;
            }
        }
    }
    eprintln!("wifi: {} networks on {adapter}", networks.len());
    networks
}

fn connect(adapter: &str, ssid: &str, passphrase: Option<&str>) -> Event {
    let failed = |why: &str| Event::Failed(ssid.to_string(), why.to_string());
    // Whatever iwd remembers about this network from a failed try is dropped,
    // or it keeps the old passphrase and never reads the new file.
    let _ = iwctl(&["known-networks", ssid, "forget"]);

    let path = Path::new(IWD_STATE).join(profile_name(ssid, passphrase.is_some()));
    if let Err(e) = write_private(&path, &profile(passphrase)) {
        return failed(&format!("could not save the network: {e}"));
    }
    eprintln!("wifi: joining {ssid:?} on {adapter}");

    // iwd needs a moment to notice the file; after that a connect either
    // starts joining or fails outright on a wrong passphrase. Other errors —
    // it was already joining on its own — are not failures.
    std::thread::sleep(Duration::from_secs(1));
    if let Some((false, text)) = iwctl(&["--dont-ask", "station", adapter, "connect", ssid])
        && strip_ansi(&text).contains("Operation failed")
    {
        let _ = iwctl(&["known-networks", ssid, "forget"]);
        return failed(if passphrase.is_some() {
            "check the passphrase"
        } else {
            "the network refused the connection"
        });
    }

    let until = Instant::now() + CONNECT_TIMEOUT;
    while Instant::now() < until {
        if let Some((_, text)) = iwctl(&["station", adapter, "show"])
            && let (true, Some(address)) = parse_station(&text)
        {
            eprintln!("wifi: joined {ssid:?} as {address}");
            return Event::Connected(ssid.to_string());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = iwctl(&["known-networks", ssid, "forget"]);
    failed("no address after 30 seconds")
}

/// Write `contents` to `path`, readable by root alone from the moment it
/// exists: it holds a passphrase.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        Network, parse_networks, parse_station, profile, profile_name, validate_passphrase,
    };

    /// Captured from iwctl 3.12 on Alpine, joined to "Alpymist Test".
    const NETWORKS: &str = "                               Available networks\u{1b}[1;90m                              \u{1b}[0m
\u{1b}[90m--------------------------------------------------------------------------------
\u{1b}[0m\u{1b}[1;90m      Network name                      Security            Signal
\u{1b}[0m\u{1b}[90m--------------------------------------------------------------------------------
\u{1b}[0m  \u{1b}[1;90m> \u{1b}[0m  Alpymist Test                     psk                 ****
      Kaffebar                          open                **\u{1b}[1;90m**\u{1b}[0m
      Eduroam                           8021x               ***\u{1b}[1;90m*\u{1b}[0m
      Naboen sitt nett                  psk                 *\u{1b}[1;90m***\u{1b}[0m

";

    const STATION: &str = "                                 Station: wlan1\u{1b}[1;90m                                \u{1b}[0m
\u{1b}[90m--------------------------------------------------------------------------------
\u{1b}[0m\u{1b}[1;90m  Settable  Property              Value
\u{1b}[0m\u{1b}[90m--------------------------------------------------------------------------------
\u{1b}[0m            Scanning              no
            State                 connected
            Connected network     Alpymist Test
            IPv4 address          10.42.0.57
";

    #[test]
    fn networks_are_read_from_iwctl_with_their_signal() {
        assert_eq!(
            parse_networks(NETWORKS),
            [
                Network {
                    ssid: "Alpymist Test".into(),
                    secured: true,
                    bars: 4
                },
                Network {
                    ssid: "Kaffebar".into(),
                    secured: false,
                    bars: 2
                },
                Network {
                    ssid: "Naboen sitt nett".into(),
                    secured: true,
                    bars: 1
                },
            ],
            "802.1X is left out: this screen cannot log in to it"
        );
    }

    #[test]
    fn headings_and_empty_output_are_not_networks() {
        assert!(parse_networks("").is_empty());
        assert!(parse_networks("No devices in Station mode available.\n").is_empty());
    }

    #[test]
    fn a_station_is_joined_only_once_it_has_an_address() {
        assert_eq!(
            parse_station(STATION),
            (true, Some("10.42.0.57".to_string()))
        );
        let joining = STATION
            .replace(
                "State                 connected",
                "State                 connecting",
            )
            .replace("10.42.0.57", "");
        assert_eq!(parse_station(&joining), (false, None));
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
    fn a_profile_holds_the_passphrase_and_an_open_one_nothing() {
        assert_eq!(
            profile(Some("correct horse")),
            "[Security]\nPassphrase=correct horse\n"
        );
        assert_eq!(profile(None), "");
    }

    #[test]
    fn passphrases_are_checked_before_anything_is_tried() {
        assert!(validate_passphrase("correct horse battery").is_ok());
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase(&"x".repeat(64)).is_err());
        assert!(validate_passphrase("blåbærsyltetøy").is_err());
        assert!(
            validate_passphrase("line\nbreak!!").is_err(),
            "a newline would end the profile's line early"
        );
    }
}

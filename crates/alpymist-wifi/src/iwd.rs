//! iwd, over the system bus.
//!
//! iwd publishes everything as objects: an adapter per radio, a device per
//! interface with a station on it, a network per name in range and a known
//! network per saved profile. One `GetManagedObjects` call returns all of it,
//! so a [`State`] is always read whole, and a change of any kind — iwd
//! signals every property it changes — is answered by reading it again. That
//! is a few kilobytes over a local socket; keeping a mirror of iwd's object
//! tree in step with its signals would be far more code for nothing visible.
//!
//! Who may do this is iwd's D-Bus policy, not ours: members of `wheel` and
//! `netdev` may send it anything, which is what the desktop user is.
//!
//! Joining a network iwd has no profile for makes iwd ask an agent for the
//! passphrase. [`Iwd::with_agent`] registers one that answers with the
//! passphrase handed to [`Iwd::join`], so the passphrase goes to iwd and to
//! nowhere else — no file written, no root needed.

use crate::model::{Link, Network, Radio, Security, State, Station};
use alpymist_dbus::{Connection, Error, Failure, Proxy, Rule, Value};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const SERVICE: &str = "net.connman.iwd";
const ADAPTER: &str = "net.connman.iwd.Adapter";
const DEVICE: &str = "net.connman.iwd.Device";
const STATION: &str = "net.connman.iwd.Station";
const DIAGNOSTIC: &str = "net.connman.iwd.StationDiagnostic";
const NETWORK: &str = "net.connman.iwd.Network";
const KNOWN: &str = "net.connman.iwd.KnownNetwork";
const AGENT_PATH: &str = "/org/alpymist/wifi/agent";
const AGENT: &str = "net.connman.iwd.Agent";

/// An interface's properties, as the bus has them: `a{sv}`.
type Properties = Value;
/// Every object, by its path, and its interfaces by their names.
type Objects = HashMap<String, HashMap<String, Properties>>;

/// A connection to iwd.
pub struct Iwd {
    conn: Connection,
    passphrase: Arc<Mutex<Option<String>>>,
}

/// The objects a command needs, found in one read.
#[derive(Debug, Default)]
struct Paths {
    adapter: Option<(String, bool)>,
    device: Option<(String, String, bool)>,
    station: Option<String>,
}

impl Iwd {
    /// Connect to the system bus.
    ///
    /// # Errors
    /// No system bus. iwd not running is not an error here: it shows as
    /// [`Radio::NoDaemon`], and iwd may yet start.
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::system().map_err(|e| format!("the system bus: {e}"))?;
        Ok(Self {
            conn,
            passphrase: Arc::new(Mutex::new(None)),
        })
    }

    /// Connect, and register an agent so networks without a profile can be
    /// joined.
    ///
    /// # Errors
    /// No system bus. An agent iwd would not take — iwd not running yet — is
    /// not an error: joining a saved network needs none.
    pub fn with_agent() -> Result<Self, String> {
        let iwd = Self::connect()?;
        let passphrase = Arc::clone(&iwd.passphrase);
        iwd.conn
            .serve(AGENT_PATH, AGENT, move |call| {
                agent(call.member.as_deref(), &passphrase)
            })
            .map_err(|e| format!("agent: {e}"))?;
        iwd.register_agent();
        Ok(iwd)
    }

    /// Ask iwd to use our agent. Harmless to repeat: iwd refuses a second
    /// registration, and after iwd restarts the first is gone.
    fn register_agent(&self) {
        if let Ok(manager) = self.proxy("/net/connman/iwd", "net.connman.iwd.AgentManager") {
            let _ = manager.call("RegisterAgent", vec![Value::path(AGENT_PATH)]);
        }
    }

    // In a `Result` for what it is chained with: the call that follows it is
    // what can fail.
    #[allow(clippy::unnecessary_wraps)]
    fn proxy(&self, path: &str, interface: &str) -> Result<Proxy, Error> {
        Ok(self.conn.proxy(SERVICE, path, interface))
    }

    fn objects(&self) -> Option<Objects> {
        let managed = self
            .conn
            .proxy(SERVICE, "/", "org.freedesktop.DBus.ObjectManager")
            .ask("GetManagedObjects", Vec::new())
            .ok()?;
        Some(
            managed
                .entries()
                .filter_map(|(path, interfaces)| {
                    let interfaces = interfaces
                        .entries()
                        .filter_map(|(name, props)| {
                            Some((name.as_str()?.to_owned(), props.clone()))
                        })
                        .collect();
                    Some((path.as_path()?.to_owned(), interfaces))
                })
                .collect(),
        )
    }

    /// Everything iwd knows now.
    #[must_use]
    pub fn state(&self) -> State {
        let Some(objects) = self.objects() else {
            return State::default();
        };
        let paths = paths(&objects);
        let mut state = State {
            radio: radio(&paths),
            device: paths.device.as_ref().map(|(_, name, _)| name.clone()),
            ..State::default()
        };
        let Some(station_path) = paths.station.as_deref() else {
            return state;
        };
        let station = objects
            .iter()
            .find(|(p, _)| p.as_str() == station_path)
            .and_then(|(_, i)| i.get(STATION));
        if let Some(props) = station {
            state.station = string(props, "State").map_or(Station::Disconnected, Station::parse);
            state.scanning = boolean(props, "Scanning").unwrap_or(false);
        }

        // iwd ranks by signal and band together; out of range is left out.
        let ordered = self
            .proxy(station_path, STATION)
            .and_then(|p| p.ask("GetOrderedNetworks", Vec::new()))
            .unwrap_or(Value::Array("(on)".into(), Vec::new()));
        for entry in ordered.items() {
            // Each is a network's path and its signal, in hundredths of a dBm.
            let [path, signal] = entry.items() else {
                continue;
            };
            let (Some(path), Some(signal)) = (
                path.as_path(),
                signal.as_i64().and_then(|n| i16::try_from(n).ok()),
            ) else {
                continue;
            };
            let Some(props) = objects.get(path).and_then(|i| i.get(NETWORK)) else {
                continue;
            };
            let known_path = object_path(props, "KnownNetwork");
            let network = Network {
                path: path.to_owned(),
                name: string(props, "Name").unwrap_or_default().to_owned(),
                security: Security::parse(string(props, "Type").unwrap_or("psk")),
                signal_dbm: signal / 100,
                known: known_path.is_some(),
                known_path,
                connected: boolean(props, "Connected").unwrap_or(false),
            };
            if network.connected {
                state.current = Some(network.name.clone());
            }
            state.networks.push(network);
        }

        // Joining shows as the connected network before it is joined; the
        // station's own property is the one to trust for which it is.
        if let Some(props) = station
            && let Some(joined) = object_path(props, "ConnectedNetwork")
            && let Some(name) = objects
                .iter()
                .find(|(p, _)| p.as_str() == joined)
                .and_then(|(_, i)| i.get(NETWORK))
                .and_then(|n| string(n, "Name"))
        {
            state.current = Some(name.to_owned());
        }

        if state.station == Station::Connected
            && let Some(name) = state.current.clone()
        {
            let mut link = self.diagnostics(station_path);
            link.name = name;
            link.ipv4 = state.device.as_deref().and_then(crate::address::ipv4);
            if link.rssi_dbm.is_none() {
                link.rssi_dbm = state.network(&link.name).map(|n| n.signal_dbm);
            }
            state.link = Some(link);
        }
        state
    }

    fn diagnostics(&self, station: &str) -> Link {
        let props: Properties = self
            .proxy(station, DIAGNOSTIC)
            .and_then(|p| p.ask("GetDiagnostics", Vec::new()))
            .unwrap_or(Value::Array("{sv}".into(), Vec::new()));
        // Bitrates are in units of 100 kbit/s.
        let mbit = |key| u32_of(&props, key).map(|r| r / 10);
        Link {
            frequency_mhz: u32_of(&props, "Frequency"),
            channel: number(&props, "Channel"),
            security: string(&props, "Security").map(str::to_owned),
            rssi_dbm: number(&props, "RSSI"),
            rx_mbit: mbit("RxBitrate"),
            tx_mbit: mbit("TxBitrate"),
            mode: string(&props, "RxMode").map(str::to_owned),
            ..Link::default()
        }
    }

    fn paths(&self) -> Result<Paths, String> {
        self.objects()
            .map(|o| paths(&o))
            .ok_or_else(|| "iwd is not running".to_owned())
    }

    /// Look for networks. Returns once the scan has started; the results
    /// arrive as a change.
    ///
    /// # Errors
    /// No station, or iwd refused. A scan already running is not an error.
    pub fn scan(&self) -> Result<(), String> {
        let station = self.paths()?.station.ok_or("Wi-Fi is off")?;
        match self
            .proxy(&station, STATION)
            .and_then(|p| p.call("Scan", Vec::new()))
        {
            Ok(_) => Ok(()),
            Err(e) if error_name(&e) == Some("net.connman.iwd.Busy") => Ok(()),
            Err(e) => Err(describe(&e)),
        }
    }

    /// Scan, and wait for it to finish or for `timeout`, whichever is first;
    /// then read the state. A radio that is off is switched on first, since
    /// whoever asks for a scan wants networks.
    #[must_use]
    pub fn scan_and_wait(&self, timeout: Duration) -> State {
        if self.state().radio == Radio::Off {
            let _ = self.set_powered(true);
            // A device appears a moment after its adapter is powered.
            std::thread::sleep(Duration::from_millis(500));
        }
        let changes = self.changes().ok();
        if self.scan().is_err() {
            return self.state();
        }
        let deadline = Instant::now() + timeout;
        // The Scanning property turns true a moment after Scan returns.
        std::thread::sleep(Duration::from_millis(300));
        let mut state = self.state();
        while state.scanning && Instant::now() < deadline {
            match &changes {
                Some(c) => {
                    let _ = c.recv_timeout(Duration::from_secs(1));
                }
                None => std::thread::sleep(Duration::from_secs(1)),
            }
            state = self.state();
        }
        state
    }

    /// Wait until the station is joined and has an IPv4 address, or
    /// `timeout` passes. Returns the address.
    #[must_use]
    pub fn wait_for_address(&self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let state = self.state();
            if state.station == Station::Connected
                && let Some(ip) = state.link.and_then(|l| l.ipv4)
            {
                return Some(ip);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Join the network at `path`, with `passphrase` for iwd's agent if it
    /// asks for one. Blocks until iwd has joined or given up, which can take
    /// several seconds.
    ///
    /// # Errors
    /// Why it could not be joined.
    pub fn join(&self, path: &str, passphrase: Option<String>) -> Result<(), JoinError> {
        // Registering again covers an iwd restarted since the agent was.
        self.register_agent();
        let with_passphrase = passphrase.is_some();
        *self
            .passphrase
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = passphrase;
        let result = self
            .proxy(path, NETWORK)
            .and_then(|p| p.call("Connect", Vec::new()));
        *self
            .passphrase
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        let Err(e) = result else {
            return Ok(());
        };
        Err(match error_name(&e) {
            Some("net.connman.iwd.AlreadyConnected") => return Ok(()),
            Some("net.connman.iwd.Failed") if with_passphrase => JoinError::WrongPassphrase,
            Some("net.connman.iwd.Failed") => JoinError::Refused,
            Some("net.connman.iwd.Aborted") if !with_passphrase => JoinError::NeedsPassphrase,
            Some("net.connman.iwd.Aborted") => JoinError::Cancelled,
            Some("net.connman.iwd.NotSupported" | "net.connman.iwd.NotConfigured") => {
                JoinError::NeedsSetup
            }
            Some("net.connman.iwd.Timeout") => JoinError::NoAnswer,
            _ => JoinError::Other(describe(&e)),
        })
    }

    /// Leave the joined network. iwd will not rejoin it by itself until asked.
    ///
    /// # Errors
    /// No station, or iwd refused.
    pub fn disconnect(&self) -> Result<(), String> {
        let station = self.paths()?.station.ok_or("Wi-Fi is off")?;
        self.proxy(&station, STATION)
            .and_then(|p| p.call("Disconnect", Vec::new()))
            .map(|_| ())
            .map_err(|e| describe(&e))
    }

    /// Forget the saved profile at `known_path`, leaving the network if it is
    /// joined.
    ///
    /// # Errors
    /// iwd refused.
    pub fn forget(&self, known_path: &str) -> Result<(), String> {
        self.proxy(known_path, KNOWN)
            .and_then(|p| p.call("Forget", Vec::new()))
            .map(|_| ())
            .map_err(|e| describe(&e))
    }

    /// Switch the radio on or off.
    ///
    /// Off is the device, as `iwctl device wlan0 set-property Powered off`
    /// does; iwd remembers nothing it would lose. On is the adapter first when
    /// that is off too, since a device only exists on a powered adapter.
    ///
    /// # Errors
    /// No adapter, or the radio is blocked by a hardware switch.
    pub fn set_powered(&self, on: bool) -> Result<(), String> {
        let paths = self.paths()?;
        let (adapter, adapter_on) = paths.adapter.ok_or("There is no Wi-Fi adapter.")?;
        let set = |path: &str, interface: &str| {
            self.proxy(path, interface)
                .map_err(|e| describe(&e))?
                .set("Powered", on.into())
                .map_err(|e| describe(&e))
        };
        if on && !adapter_on {
            set(&adapter, ADAPTER)?;
            return Ok(());
        }
        match paths.device {
            Some((device, _, powered)) if powered != on => set(&device, DEVICE),
            None if !on => set(&adapter, ADAPTER),
            _ => Ok(()),
        }
    }

    /// A receiver that gets `()` whenever iwd signals anything, or its name
    /// changes hands on the bus — iwd starting or stopping.
    ///
    /// # Errors
    /// The bus would not take the match rule.
    pub fn changes(&self) -> Result<Receiver<()>, String> {
        let from_iwd = Rule::from(SERVICE);
        let owner = Rule::from("org.freedesktop.DBus")
            .member("NameOwnerChanged")
            .arg0(SERVICE);
        let (tx, rx) = channel();
        for rule in [from_iwd, owner] {
            let messages = self.conn.signals(rule).map_err(|e| e.to_string())?;
            let tx = tx.clone();
            std::thread::spawn(move || {
                for _ in messages {
                    if tx.send(()).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(rx)
    }
}

/// Why a network could not be joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// iwd was given a passphrase and the network refused it.
    WrongPassphrase,
    /// The network refused, with no passphrase involved.
    Refused,
    /// The network wants a passphrase and none was given.
    NeedsPassphrase,
    /// Joining was called off before it finished.
    Cancelled,
    /// Enterprise and the like: a profile has to be written by hand.
    NeedsSetup,
    /// The access point stopped answering.
    NoAnswer,
    /// Anything else, in iwd's or the bus's words.
    Other(String),
}

impl JoinError {
    /// A sentence on its own, as the popup shows it.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::WrongPassphrase => "Could not join — check the passphrase.".into(),
            Self::Refused => "The network would not let us join.".into(),
            Self::NeedsPassphrase => "This network needs a passphrase.".into(),
            Self::Cancelled => "Joining was cancelled.".into(),
            Self::NeedsSetup => "This kind of network needs setting up by hand.".into(),
            Self::NoAnswer => "The network did not answer.".into(),
            Self::Other(why) => why.clone(),
        }
    }

    /// The reason alone, to follow "Could not join <network>: ", as the
    /// installer says it.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::WrongPassphrase => "check the passphrase".into(),
            Self::Refused => "the network refused the connection".into(),
            Self::NeedsPassphrase => "it needs a passphrase".into(),
            Self::Cancelled => "joining was cancelled".into(),
            Self::NeedsSetup => "it needs setting up by hand".into(),
            Self::NoAnswer => "the network did not answer".into(),
            Self::Other(why) => why.clone(),
        }
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.sentence())
    }
}

/// The agent iwd asks for secrets: what it answers each of iwd's questions
/// with. The passphrase is the one handed to [`Iwd::join`] and nothing else
/// is known here, so everything else iwd may ask for is declined.
fn agent(member: Option<&str>, passphrase: &Mutex<Option<String>>) -> Result<Vec<Value>, Failure> {
    let declined = |why: &str| Failure::new("net.connman.iwd.Agent.Error.Canceled", why);
    match member {
        // Told, not asked: the agent is let go, or a request called off.
        Some("Release" | "Cancel") => Ok(Vec::new()),
        Some("RequestPassphrase") => passphrase
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .map(|passphrase| vec![passphrase.into()])
            .ok_or_else(|| declined("no passphrase given")),
        // A private key's passphrase, a user name and password: enterprise
        // networks, which are set up by hand.
        _ => Err(declined("not supported")),
    }
}

fn paths(objects: &Objects) -> Paths {
    // A machine with two radios is rare enough that the first will do; sorted
    // so it is the same one every time.
    let mut sorted: Vec<_> = objects.iter().collect();
    sorted.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    let mut paths = Paths::default();
    for (path, interfaces) in sorted {
        if let Some(props) = interfaces.get(ADAPTER)
            && paths.adapter.is_none()
        {
            let powered = boolean(props, "Powered").unwrap_or(false);
            paths.adapter = Some((path.clone(), powered));
        }
        if let Some(props) = interfaces.get(DEVICE)
            && paths.device.is_none()
        {
            let name = string(props, "Name").unwrap_or_default().to_owned();
            let powered = boolean(props, "Powered").unwrap_or(false);
            paths.device = Some((path.clone(), name, powered));
            if interfaces.contains_key(STATION) {
                paths.station = Some(path.clone());
            }
        }
    }
    paths
}

fn radio(paths: &Paths) -> Radio {
    match (&paths.adapter, &paths.device) {
        (None, _) => Radio::NoAdapter,
        (Some((_, false)), _) | (Some(_), None) | (_, Some((_, _, false))) => Radio::Off,
        _ => Radio::On,
    }
}

fn string<'a>(props: &'a Properties, key: &str) -> Option<&'a str> {
    props.get(key).and_then(Value::as_str)
}

fn boolean(props: &Properties, key: &str) -> Option<bool> {
    props.get(key).and_then(Value::as_bool)
}

fn u32_of(props: &Properties, key: &str) -> Option<u32> {
    props.get(key).and_then(Value::as_u32)
}

/// A number of whatever width iwd sends it in, where it fits in a `T`.
fn number<T: TryFrom<i64>>(props: &Properties, key: &str) -> Option<T> {
    props
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| T::try_from(n).ok())
}

fn object_path(props: &Properties, key: &str) -> Option<String> {
    props.get(key).and_then(Value::as_path).map(str::to_owned)
}

fn error_name(error: &Error) -> Option<&str> {
    error.name()
}

/// An error in words, for when there are no better ones.
fn describe(error: &Error) -> String {
    match error {
        Error::Method {
            message: Some(message),
            ..
        } if !message.is_empty() => format!("iwd: {message}"),
        Error::Method { name, .. } => {
            let short = name.rsplit('.').next().unwrap_or(name);
            if short == "ServiceUnknown" {
                "iwd is not running".to_owned()
            } else {
                format!("iwd: {short}")
            }
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Objects, Value, agent, describe, object_path, paths, string};
    use alpymist_dbus::Error;
    use std::sync::Mutex;

    #[test]
    fn the_agent_answers_with_the_passphrase_it_was_given_and_nothing_else() {
        let given = Mutex::new(Some("correct horse".to_owned()));
        assert_eq!(
            agent(Some("RequestPassphrase"), &given),
            Ok(vec!["correct horse".into()])
        );
        assert_eq!(agent(Some("Release"), &given), Ok(Vec::new()));
        assert_eq!(agent(Some("Cancel"), &given), Ok(Vec::new()));
        // An enterprise network's questions, and anything not known of.
        for member in [
            Some("RequestPrivateKeyPassphrase"),
            Some("RequestUserNameAndPassword"),
            Some("RequestUserPassword"),
            Some("Introspect"),
            None,
        ] {
            let declined = agent(member, &given).unwrap_err();
            assert_eq!(declined.name, "net.connman.iwd.Agent.Error.Canceled");
        }
        // And with none given, iwd is told so and does not get an empty one.
        let none = Mutex::new(None);
        assert!(agent(Some("RequestPassphrase"), &none).is_err());
    }

    #[test]
    fn what_iwd_publishes_is_read_by_name_and_kind() {
        let station = Value::named([("State", "connected".into())]);
        let device = Value::named([("Name", "wlan0".into()), ("Powered", true.into())]);
        let network = Value::named([
            ("Name", "Fjellheim".into()),
            ("KnownNetwork", Value::path("/net/connman/iwd/known")),
            // A string where a path is meant is not taken for one.
            ("Device", "/net/connman/iwd/0/3".into()),
        ]);
        assert_eq!(string(&network, "Name"), Some("Fjellheim"));
        assert_eq!(
            object_path(&network, "KnownNetwork").as_deref(),
            Some("/net/connman/iwd/known")
        );
        assert_eq!(object_path(&network, "Device"), None);
        assert_eq!(string(&network, "Missing"), None);

        let mut objects = Objects::new();
        objects.insert(
            "/net/connman/iwd/0".into(),
            [(
                super::ADAPTER.to_owned(),
                Value::named([("Powered", true.into())]),
            )]
            .into(),
        );
        objects.insert(
            "/net/connman/iwd/0/3".into(),
            [
                (super::DEVICE.to_owned(), device),
                (super::STATION.to_owned(), station),
            ]
            .into(),
        );
        let found = paths(&objects);
        assert_eq!(found.adapter, Some(("/net/connman/iwd/0".into(), true)));
        assert_eq!(
            found.device,
            Some(("/net/connman/iwd/0/3".into(), "wlan0".into(), true))
        );
        assert_eq!(found.station.as_deref(), Some("/net/connman/iwd/0/3"));
    }

    #[test]
    fn an_error_is_said_in_iwds_words_or_the_buss() {
        let method = |name: &str, message: Option<&str>| Error::Method {
            name: name.to_owned(),
            message: message.map(str::to_owned),
        };
        assert_eq!(
            describe(&method("net.connman.iwd.Failed", Some("Operation failed"))),
            "iwd: Operation failed"
        );
        assert_eq!(describe(&method("net.connman.iwd.Busy", None)), "iwd: Busy");
        assert_eq!(
            describe(&method(
                "org.freedesktop.DBus.Error.ServiceUnknown",
                Some("")
            )),
            "iwd is not running"
        );
        assert_eq!(describe(&Error::Closed), "the connection to the bus closed");
    }
}

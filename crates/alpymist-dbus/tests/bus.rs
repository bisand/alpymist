//! The client against a real bus: a `dbus-daemon` of the tests' own, which
//! checks every message it is handed and hangs up on one that is not D-Bus.
//!
//! Where there is no `dbus-daemon` to run, these say so and pass: the wire
//! format's own tests need nothing installed, and CI has the daemon.

use alpymist_dbus::{Connection, Error, Failure, Rule, Value};
use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

/// A bus of our own, gone when this is dropped.
struct Bus {
    daemon: Child,
    socket: String,
}

impl Bus {
    fn start(name: &str) -> Option<Self> {
        let socket = std::env::temp_dir()
            .join(format!("alpymist-dbus-{}-{name}", std::process::id()))
            .to_str()?
            .to_owned();
        let _ = std::fs::remove_file(&socket);
        let daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .arg(format!("--address=unix:path={socket}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut daemon) = daemon else {
            eprintln!("no dbus-daemon to run: {name} skipped");
            return None;
        };
        // It prints its address once it listens.
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        assert!(address.contains(&socket), "{address}");
        Some(Self { daemon, socket })
    }

    fn connect(&self) -> Connection {
        Connection::open(&self.socket).unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const PATH: &str = "/org/alpymist/Test";
const INTERFACE: &str = "org.alpymist.Test";

#[test]
fn the_bus_names_us_and_answers() {
    let Some(bus) = Bus::start("names") else {
        return;
    };
    let ours = bus.connect();
    assert!(ours.name().starts_with(":1."), "{}", ours.name());
    let other = bus.connect();
    assert_ne!(ours.name(), other.name());

    let names = ours
        .call(BUS, BUS_PATH, BUS, "ListNames", Vec::new())
        .unwrap();
    let names: Vec<&str> = names[0].items().iter().filter_map(Value::as_str).collect();
    assert!(names.contains(&BUS) && names.contains(&ours.name()) && names.contains(&other.name()));

    // An error is its name and what was said with it.
    let error = ours
        .call(BUS, BUS_PATH, BUS, "NoSuchMethod", Vec::new())
        .unwrap_err();
    assert_eq!(
        error.name(),
        Some("org.freedesktop.DBus.Error.UnknownMethod")
    );
    let error = ours
        .call("org.alpymist.Nobody", "/", "a.b", "C", Vec::new())
        .unwrap_err();
    assert_eq!(
        error.name(),
        Some("org.freedesktop.DBus.Error.ServiceUnknown")
    );
    assert!(error.to_string().contains("org.alpymist.Nobody"), "{error}");

    // What is not a path never reaches the bus, which would hang up on it.
    let error = ours.call(BUS, "not a path", BUS, "ListNames", Vec::new());
    assert!(matches!(error, Err(Error::Protocol(_))), "{error:?}");
    assert!(
        ours.call(BUS, BUS_PATH, BUS, "ListNames", Vec::new())
            .is_ok()
    );
}

#[test]
fn every_kind_of_value_passes_through_the_bus_and_back() {
    let Some(bus) = Bus::start("values") else {
        return;
    };
    let server = bus.connect();
    server
        .serve(PATH, INTERFACE, |call| Ok(call.body.clone()))
        .unwrap();
    let client = bus.connect();
    let sent = vec![
        Value::Array(
            "{oa{sa{sv}}}".into(),
            vec![Value::Entry(
                Box::new(Value::path("/net/connman/iwd/0/4")),
                Box::new(Value::Array(
                    "{sa{sv}}".into(),
                    vec![Value::Entry(
                        Box::new("net.connman.iwd.Network".into()),
                        Box::new(Value::named([
                            ("Name", "Fjellheim".into()),
                            ("Connected", true.into()),
                            ("RSSI", Value::I16(-6100)),
                            ("Known", Value::path("/net/connman/iwd/known")),
                            ("Modes", Value::strings(["station", "ap"])),
                        ])),
                    )],
                )),
            )],
        ),
        Value::U8(255),
        Value::U16(7),
        Value::I32(-1),
        Value::I64(i64::MIN),
        Value::U64(u64::MAX),
        Value::F64(-0.25),
        Value::Signature("a{sv}".into()),
        Value::Struct(vec![
            Value::U8(1),
            Value::U64(2),
            "æøå".into(),
            Value::U8(3),
        ]),
        Value::Array("(os)".into(), Vec::new()),
        Value::Array(
            "ay".into(),
            vec![Value::Array("y".into(), vec![Value::U8(9)])],
        ),
        Value::variant(Value::variant(Value::U32(3))),
    ];
    let back = client
        .call(server.name(), PATH, INTERFACE, "Echo", sent.clone())
        .unwrap();
    assert_eq!(back, sent);
    assert_eq!(back[0].items().len(), 1);

    // Nothing served there, and a refusal from what is.
    let error = client.call(server.name(), "/elsewhere", INTERFACE, "Echo", Vec::new());
    assert_eq!(
        error.unwrap_err().name(),
        Some("org.freedesktop.DBus.Error.UnknownMethod")
    );
    server
        .serve("/refuses", INTERFACE, |_| {
            Err(Failure::new("org.alpymist.Test.Error.No", "not today"))
        })
        .unwrap();
    let error = client
        .call(server.name(), "/refuses", INTERFACE, "Anything", Vec::new())
        .unwrap_err();
    assert_eq!(
        error,
        Error::Method {
            name: "org.alpymist.Test.Error.No".into(),
            message: Some("not today".into()),
        }
    );
}

/// What iwd does: asked to connect, it asks the one who asked for a
/// passphrase, and answers only once it has it.
#[test]
fn a_call_is_answered_while_one_of_our_own_is_waiting() {
    let Some(bus) = Bus::start("agent") else {
        return;
    };
    let daemon = bus.connect();
    let asks = daemon.clone();
    daemon
        .serve(PATH, INTERFACE, move |call| {
            let caller = call.sender.clone().unwrap_or_default();
            let answer = asks
                .call(
                    &caller,
                    "/agent",
                    "org.alpymist.Agent",
                    "RequestPassphrase",
                    Vec::new(),
                )
                .map_err(|e| Failure::new("org.alpymist.Test.Error.Aborted", &e.to_string()))?;
            Ok(vec![
                format!("joined with {}", answer[0].as_str().unwrap_or("?")).into(),
            ])
        })
        .unwrap();

    let client = bus.connect();
    client
        .serve("/agent", "org.alpymist.Agent", |_| {
            Ok(vec!["hunter2".into()])
        })
        .unwrap();
    let joined = client
        .proxy(daemon.name(), PATH, INTERFACE)
        .ask("Connect", Vec::new())
        .unwrap();
    assert_eq!(joined.as_str(), Some("joined with hunter2"));

    // And with no agent to ask, the refusal comes all the way back.
    let bare = bus.connect();
    let error = bare
        .call(daemon.name(), PATH, INTERFACE, "Connect", Vec::new())
        .unwrap_err();
    assert_eq!(error.name(), Some("org.alpymist.Test.Error.Aborted"));
}

/// What polkit does: a prompt is up for as long as somebody takes, and is
/// called off by a second call while the first has not returned.
#[test]
fn a_method_that_waits_does_not_keep_another_from_being_called() {
    let Some(bus) = Bus::start("cancel") else {
        return;
    };
    let agent = bus.connect();
    let (call_off, called_off) = channel::<()>();
    let called_off = std::sync::Mutex::new(called_off);
    let call_off = std::sync::Mutex::new(call_off);
    agent
        .serve(PATH, INTERFACE, move |call| match call.member.as_deref() {
            Some("Begin") => {
                let waited = called_off
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(20));
                waited
                    .map(|()| vec!["called off".into()])
                    .map_err(|_| Failure::new("org.alpymist.Test.Error.Timeout", "nobody came"))
            }
            Some("Cancel") => {
                let _ = call_off.lock().unwrap().send(());
                Ok(Vec::new())
            }
            _ => Err(Failure::new("org.freedesktop.DBus.Error.UnknownMethod", "")),
        })
        .unwrap();

    let polkit = bus.connect();
    let begun = polkit.proxy(agent.name(), PATH, INTERFACE);
    let waiting = std::thread::spawn(move || begun.ask("Begin", Vec::new()));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!waiting.is_finished());
    polkit
        .call(agent.name(), PATH, INTERFACE, "Cancel", Vec::new())
        .unwrap();
    assert_eq!(
        waiting.join().unwrap().unwrap().as_str(),
        Some("called off")
    );
}

#[test]
fn signals_asked_for_are_heard_and_others_are_not() {
    let Some(bus) = Bus::start("signals") else {
        return;
    };
    let ours = bus.connect();
    let name = "org.alpymist.Test.Daemon";
    let owner = ours
        .signals(Rule::from(BUS).member("NameOwnerChanged").arg0(name))
        .unwrap();
    let other = ours
        .signals(
            Rule::from(BUS)
                .member("NameOwnerChanged")
                .arg0("org.alpymist.Other"),
        )
        .unwrap();

    // The daemon starts: it takes its name.
    let daemon = bus.connect();
    let taken = daemon
        .call(
            BUS,
            BUS_PATH,
            BUS,
            "RequestName",
            vec![name.into(), Value::U32(0)],
        )
        .unwrap();
    assert_eq!(taken[0].as_u32(), Some(1));
    let started = owner.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(started.member.as_deref(), Some("NameOwnerChanged"));
    let [changed, was, is] = started.body.as_slice() else {
        panic!("{:?}", started.body);
    };
    assert_eq!(
        (changed.as_str(), was.as_str(), is.as_str()),
        (Some(name), Some(""), Some(daemon.name()))
    );

    // And stops: its connection goes, and the name with it.
    let unique = daemon.name().to_owned();
    drop(daemon);
    let stopped = owner.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(stopped.body[1].as_str(), Some(unique.as_str()));
    assert_eq!(stopped.body[2].as_str(), Some(""));
    assert!(other.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn a_property_is_read_and_set_as_the_bus_has_them() {
    let Some(bus) = Bus::start("properties") else {
        return;
    };
    let device = bus.connect();
    let powered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let state = std::sync::Arc::clone(&powered);
    device
        .serve(PATH, "org.freedesktop.DBus.Properties", move |call| {
            let named = |i: usize| call.body.get(i).and_then(Value::as_str);
            let known = (named(0), named(1)) == (Some(INTERFACE), Some("Powered"));
            match (call.member.as_deref(), known) {
                (Some("Get"), true) => {
                    let now = state.load(std::sync::atomic::Ordering::SeqCst);
                    Ok(vec![Value::variant(now.into())])
                }
                (Some("Set"), true) => {
                    let to = call.body.get(2).and_then(Value::as_bool);
                    let to = to.ok_or_else(|| Failure::new("org.alpymist.Test.Error.Type", ""))?;
                    state.store(to, std::sync::atomic::Ordering::SeqCst);
                    Ok(Vec::new())
                }
                _ => Err(Failure::new(
                    "org.freedesktop.DBus.Error.UnknownProperty",
                    "no such property",
                )),
            }
        })
        .unwrap();

    let client = bus.connect();
    let proxy = client.proxy(device.name(), PATH, INTERFACE);
    assert_eq!(proxy.get("Powered").unwrap(), Value::Bool(false));
    proxy.set("Powered", true.into()).unwrap();
    assert!(powered.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(proxy.get("Powered").unwrap().as_bool(), Some(true));
    let error = proxy.get("Nothing").unwrap_err();
    assert_eq!(
        error.name(),
        Some("org.freedesktop.DBus.Error.UnknownProperty")
    );
    assert_eq!(proxy.path(), PATH);
}

#[test]
fn whoever_waits_on_a_bus_that_goes_is_told() {
    let Some(mut bus) = Bus::start("gone") else {
        return;
    };
    let slow = bus.connect();
    slow.serve(PATH, INTERFACE, |_| {
        std::thread::sleep(Duration::from_secs(30));
        Ok(Vec::new())
    })
    .unwrap();
    let client = bus.connect();
    let name = slow.name().to_owned();
    let waiting =
        std::thread::spawn(move || client.call(&name, PATH, INTERFACE, "Slow", Vec::new()));
    std::thread::sleep(Duration::from_millis(200));
    bus.daemon.kill().unwrap();
    assert_eq!(waiting.join().unwrap(), Err(Error::Closed));

    // And where there is no bus at all, that is said and nothing hangs.
    let error = Connection::open("/nonexistent/alpymist/bus").err().unwrap();
    assert!(matches!(error, Error::Io(_)), "{error:?}");
}

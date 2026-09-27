//! `alpymist-thunderbolt` — asking before a Thunderbolt or USB4 device is
//! let in.
//!
//! ```text
//! alpymist-thunderbolt            watch, for the session: started with the desktop
//! alpymist-thunderbolt ask UUID   ask about one device now
//! alpymist-thunderbolt list       the devices plugged in and allowed always
//! alpymist-thunderbolt forget UUID
//!                                 stop allowing a device always
//! ```
//!
//! The watch asks about a device nobody has allowed, only while the session
//! is unlocked, and lets in again, without asking, one allowed always. What
//! needs root is `alpymist-thunderbolt-helper`, through pkexec.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod dialog;

use alpymist_thunderbolt::policy::{self, Decision, Session};
use alpymist_thunderbolt::session::{self, Changes};
use alpymist_thunderbolt::store::Store;
use alpymist_thunderbolt::sysfs::{Bus, valid_uuid};
use alpymist_thunderbolt::{AUTH, HELPER};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "\
usage: alpymist-thunderbolt [watch | ask UUID | list | forget UUID]

With no command, watches for Thunderbolt and USB4 devices for this session:
asks about one nobody has allowed, and lets in again one allowed always.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["watch"] => watch(),
        ["ask", uuid] if valid_uuid(uuid) => ask(uuid),
        ["list"] => {
            list();
            ExitCode::SUCCESS
        }
        ["forget", uuid] if valid_uuid(uuid) => helper(&["forget", uuid]),
        ["-V" | "--version"] => {
            println!("alpymist-thunderbolt {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn list() {
    let root = Path::new("/");
    let bus = Bus::read(root);
    let store = Store::new(root);
    for d in &bus.domains {
        let level = d
            .level
            .map_or_else(|| "unknown".into(), |l| format!("{l:?}").to_lowercase());
        let dma = if d.dma_protected { "on" } else { "off" };
        println!(
            "domain{}: security {level}, IOMMU DMA protection {dma}",
            d.index
        );
    }
    for d in &bus.devices {
        let state = if d.authorized { "in" } else { "waiting" };
        let always = if store.get(&d.uuid).is_some() {
            ", allowed always"
        } else {
            ""
        };
        println!("  {}  {}  {} ({state}{always})", d.name, d.uuid, d.title());
    }
    let present: HashSet<&str> = bus.devices.iter().map(|d| d.uuid.as_str()).collect();
    for a in store.allowed() {
        if !present.contains(a.uuid.as_str()) {
            println!(
                "  -    {}  {} {} (allowed always, not plugged in)",
                a.uuid, a.vendor, a.model
            );
        }
    }
}

/// Run the helper through `alpymist-auth`, which asks for a password if
/// polkit wants one.
fn helper(args: &[&str]) -> ExitCode {
    match Command::new(AUTH)
        .args(["run", "--", HELPER])
        .args(args)
        .status()
    {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("alpymist-thunderbolt: {AUTH}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Watch for as long as the session lasts.
fn watch() -> ExitCode {
    let Some(uid) = session::own_uid() else {
        eprintln!("alpymist-thunderbolt: cannot tell who this is");
        return ExitCode::FAILURE;
    };
    // One watch per session: a second, from a reload, leaves.
    let Ok(_instance) = single_instance() else {
        return ExitCode::SUCCESS;
    };
    let root = Path::new("/");
    let changes = Changes::new();
    let store = Store::new(root);
    // Asked about, or tried, since it was plugged in: not again until it is
    // unplugged and plugged back in.
    let mut settled: HashSet<String> = HashSet::new();
    let mut asking: Option<(String, Child)> = None;
    loop {
        if let Some((_, child)) = &mut asking
            && !matches!(child.try_wait(), Ok(None))
        {
            asking = None;
        }
        let bus = Bus::read(root);
        settled.retain(|u| bus.find(u).is_some_and(|d| !d.authorized));
        let state = session::state(root, uid);
        let mut waiting = false;
        for device in &bus.devices {
            if settled.contains(&device.uuid) {
                continue;
            }
            let allowed = store.get(&device.uuid);
            // The session cannot read keys; the helper uses one if it has it.
            // What matters here is only whether to wait for an unlock.
            let decision = policy::decide(
                device,
                bus.domain_of(device),
                allowed.as_ref(),
                false,
                state,
            );
            match decision {
                Decision::Reconnect(_) => {
                    settled.insert(device.uuid.clone());
                    reconnect(&device.uuid);
                }
                Decision::Ask if asking.is_none() => {
                    settled.insert(device.uuid.clone());
                    match std::env::current_exe()
                        .and_then(|me| Command::new(me).args(["ask", &device.uuid]).spawn())
                    {
                        Ok(child) => asking = Some((device.uuid.clone(), child)),
                        Err(e) => eprintln!("alpymist-thunderbolt: cannot ask: {e}"),
                    }
                }
                Decision::Ask => waiting = true,
                Decision::Nothing => {
                    waiting |= !device.authorized
                        && state == Session::Locked
                        && bus
                            .domain_of(device)
                            .and_then(|d| d.level)
                            .is_some_and(alpymist_thunderbolt::sysfs::Level::asks);
                }
            }
        }
        // Waiting on an unlock or on the dialog: look again soon. Otherwise
        // only a device coming or going can change anything.
        let timeout = if waiting || asking.is_some() {
            Duration::from_secs(1)
        } else {
            Duration::from_mins(1)
        };
        changes.wait(timeout);
    }
}

fn reconnect(uuid: &str) {
    match Command::new("pkexec")
        .args([HELPER, "reconnect", uuid])
        .output()
    {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!(
            "alpymist-thunderbolt: reconnecting {uuid}: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => eprintln!("alpymist-thunderbolt: pkexec: {e}"),
    }
}

/// Hold this session's socket, or learn that another watch holds it.
fn single_instance() -> Result<Option<std::os::unix::net::UnixListener>, ()> {
    use std::os::unix::net::{UnixListener, UnixStream};
    let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return Ok(None);
    };
    let path = Path::new(&dir).join("alpymist-thunderbolt.sock");
    if UnixStream::connect(&path).is_ok() {
        return Err(());
    }
    std::fs::remove_file(&path).ok();
    Ok(UnixListener::bind(&path).ok())
}

/// Ask about one device, and let it in if the answer is yes.
fn ask(uuid: &str) -> ExitCode {
    let bus = Bus::read(Path::new("/"));
    let Some(device) = bus.find(uuid) else {
        return ExitCode::FAILURE;
    };
    if device.authorized {
        return ExitCode::SUCCESS;
    }
    match dialog::ask(device, bus.domain_of(device)) {
        Ok(Some(keep)) => {
            let keep = match keep {
                alpymist_thunderbolt::system::Keep::Once => "once",
                alpymist_thunderbolt::system::Keep::Always => "always",
            };
            helper(&["allow", uuid, keep])
        }
        Ok(None) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("alpymist-thunderbolt: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod dialog {
    use alpymist_thunderbolt::sysfs::{Device, Domain};
    use alpymist_thunderbolt::system::Keep;

    pub fn ask(_: &Device, _: Option<&Domain>) -> Result<Option<Keep>, String> {
        Err("the question needs Wayland".into())
    }
}

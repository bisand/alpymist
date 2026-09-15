//! `alpymist-wifi` — Wi-Fi from the bar, or the command line.
//!
//! ```text
//! alpymist-wifi              open the popup (run again to close it)
//! alpymist-wifi status       where Wi-Fi is, in a few lines
//! alpymist-wifi on|off|toggle
//! alpymist-wifi scan         look for networks, and list them
//! alpymist-wifi --waybar     a line of JSON for Waybar at every change
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod widget;

use alpymist_wifi::bar;
use alpymist_wifi::iwd::Iwd;
use alpymist_wifi::model::{Radio, State};
use std::os::unix::net::UnixListener;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
usage: alpymist-wifi [status | on | off | toggle | scan | --waybar]

With no command, opens the Wi-Fi popup under the bar; run it again to close it.
  status     say where Wi-Fi is
  on, off    switch the radio on or off; toggle switches it over
  scan       look for networks and list them
  --waybar   print a line of JSON for a Waybar custom module at every change";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("alpymist-wifi {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("status") => with_iwd(|iwd| {
            println!("{}", bar::summary(&iwd.state()));
            Ok(())
        }),
        Some("on") => with_iwd(|iwd| iwd.set_powered(true)),
        Some("off") => with_iwd(|iwd| iwd.set_powered(false)),
        Some("toggle") => with_iwd(|iwd| iwd.set_powered(iwd.state().radio != Radio::On)),
        Some("scan") => with_iwd(scan),
        Some("--waybar") => with_iwd(waybar),
        Some(other) => {
            eprintln!("alpymist-wifi: unknown command {other}\n\n{USAGE}");
            ExitCode::from(2)
        }
        None => popup(),
    }
}

fn with_iwd(f: impl FnOnce(&Iwd) -> Result<(), String>) -> ExitCode {
    match Iwd::connect().and_then(|iwd| f(&iwd)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-wifi: {e}");
            ExitCode::FAILURE
        }
    }
}

fn scan(iwd: &Iwd) -> Result<(), String> {
    if iwd.state().radio == Radio::NoDaemon {
        return Err("iwd is not running".into());
    }
    print_networks(&iwd.scan_and_wait(Duration::from_secs(15)));
    Ok(())
}

fn print_networks(state: &State) {
    for n in &state.networks {
        let mut notes = Vec::new();
        if n.connected {
            notes.push("joined");
        } else if n.known {
            notes.push("saved");
        }
        if n.security.secured() {
            notes.push("secured");
        }
        println!(
            "{:<32} {:>4} dBm  {}",
            n.name,
            n.signal_dbm,
            notes.join(", ")
        );
    }
}

/// Print the bar's line now and at every change.
///
/// Signal strength changes without iwd saying so, and iwd may not be running
/// yet, so the state is also read every few seconds; a line is only printed
/// when it differs from the last.
fn waybar(iwd: &Iwd) -> Result<(), String> {
    let changes = iwd.changes()?;
    alpymist_widget::waybar::follow(
        || bar::waybar(&iwd.state()),
        || {
            if changes.recv_timeout(Duration::from_secs(10)).is_ok() {
                // Changes come in bursts; one read after the burst will do.
                std::thread::sleep(Duration::from_millis(150));
                while changes.try_recv().is_ok() {}
            }
        },
    );
    Ok(())
}

/// The popup's name: its socket, and its layer surface's namespace.
const NAME: &str = "alpymist-wifi";

fn popup() -> ExitCode {
    match alpymist_widget::instance::toggle(NAME, open) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-wifi: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn open(listener: Option<UnixListener>) -> Result<bool, String> {
    use alpymist_wifi::popup::Command;
    use std::sync::Arc;
    use widget::Event;

    let appearance = alpymist_widget::appearance();
    let fonts = alpymist_wifi::view::Fonts::load(&appearance);
    for problem in &fonts.problems {
        eprintln!("alpymist-wifi: font {problem}");
    }

    let iwd = Arc::new(Iwd::with_agent()?);
    let (events, channel) = alpymist_widget::host::events::<Event>();

    // The watcher: a fresh state at every change iwd signals, and every few
    // seconds regardless.
    {
        let iwd = Arc::clone(&iwd);
        let events = events.clone();
        std::thread::spawn(move || {
            let changes = iwd.changes().ok();
            loop {
                if events.send(Event::State(iwd.state())).is_err() {
                    return;
                }
                let signalled = changes
                    .as_ref()
                    .is_some_and(|c| c.recv_timeout(Duration::from_secs(4)).is_ok());
                if signalled {
                    std::thread::sleep(Duration::from_millis(80));
                    while changes.as_ref().is_some_and(|c| c.try_recv().is_ok()) {}
                } else if changes.is_none() {
                    std::thread::sleep(Duration::from_secs(4));
                }
            }
        });
    }

    // The worker: one command at a time, a fresh state after each.
    let (commands, queue) = std::sync::mpsc::channel::<Command>();
    std::thread::spawn(move || {
        // The list is only as fresh as iwd's last scan; start another, and
        // say nothing if the radio is off.
        let _ = iwd.scan();
        for command in queue {
            let result = match &command {
                Command::Scan => iwd.scan(),
                Command::Join {
                    path, passphrase, ..
                } => iwd.join(path, passphrase.clone()).map_err(|e| e.sentence()),
                Command::Disconnect => iwd.disconnect(),
                Command::Forget { known_path, .. } => iwd.forget(known_path),
                Command::SetPowered(on) => iwd.set_powered(*on),
            };
            if events.send(Event::Done(command, result)).is_err()
                || events.send(Event::State(iwd.state())).is_err()
            {
                return;
            }
        }
    });

    alpymist_widget::host::run(
        widget::Wifi::new(appearance, fonts, commands),
        &alpymist_widget::host::Options::new(NAME),
        channel,
        listener,
    )
}

#[cfg(not(target_os = "linux"))]
fn open(_: Option<UnixListener>) -> Result<bool, String> {
    Err(
        "the popup draws on a Wayland layer surface, which needs Linux.\n\
         To see it here: cargo run -p alpymist-wifi --example snapshot -- out/"
            .into(),
    )
}

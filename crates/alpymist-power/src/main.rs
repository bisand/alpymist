//! `alpymist-power` — the battery from the bar, or the command line.
//!
//! ```text
//! alpymist-power               open the popup (run again to close it)
//! alpymist-power status        where power is, in a few lines
//! alpymist-power profile [MODE]  say the power mode, or switch it
//! alpymist-power lid           the lid was closed: do what is set
//! alpymist-power button        the power button was pressed: do what is set
//! alpymist-power suspend|hibernate
//! alpymist-power --waybar      a line of JSON for Waybar at every change
//! ```

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod widget;

use alpymist_power::actions;
use alpymist_power::bar;
use alpymist_power::battery::Power;
use alpymist_power::config::{self, Action, Config};
use alpymist_power::profile::{Knobs, Profile};
use alpymist_power::watch::Changes;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
usage: alpymist-power [status | profile [MODE] | lid | button | suspend | hibernate | --waybar]

With no command, opens the power popup under the bar; run it again to close it.
  status         say where power is
  profile        say the power mode and the modes offered
  profile MODE   switch to power-saver, balanced or performance
  lid            the lid was closed: do what the popup set for it
  button         the power button was pressed: do what the popup set for it
  suspend        lock the screen and suspend
  hibernate      lock the screen and hibernate
  --waybar       print a line of JSON for a Waybar custom module at every change";

/// The popup's name: its socket, and its layer surface's namespace.
const NAME: &str = "alpymist-power";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["-h" | "--help"] => {
            println!("{USAGE}");
            Ok(())
        }
        ["-V" | "--version"] => {
            println!("alpymist-power {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["status"] => {
            println!("{}", bar::summary(&Power::now(), Knobs::now().current()));
            Ok(())
        }
        ["profile"] => {
            print_profiles(&Knobs::now());
            Ok(())
        }
        ["profile", mode] => match Profile::parse(mode) {
            Some(p) => actions::helper(&["profile", p.id()]),
            None => Err(format!("no such power mode: {mode}")),
        },
        // Hyprland's switch binding fires on close; `lid close` reads well
        // in a configuration too.
        ["lid"] | ["lid", "close"] => lid(),
        ["lid", "open"] => Ok(()),
        ["button"] => {
            let config = Config::load();
            actions::run(config.actions.power_button, &config)
        }
        ["suspend"] => actions::run(Action::Suspend, &Config::load()),
        ["hibernate"] => actions::run(Action::Hibernate, &Config::load()),
        ["--waybar"] => {
            waybar();
            Ok(())
        }
        [] => alpymist_widget::instance::toggle(NAME, open),
        _ => {
            eprintln!(
                "alpymist-power: unknown command {}\n\n{USAGE}",
                args.join(" ")
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("alpymist-power: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_profiles(knobs: &Knobs) {
    let current = knobs.current();
    let available = knobs.available();
    if available.is_empty() {
        println!("This machine offers no power modes.");
        return;
    }
    for p in available {
        let mark = if Some(p) == current { "*" } else { " " };
        println!("{mark} {}", p.id());
    }
}

fn lid() -> Result<(), String> {
    let config = Config::load();
    let action = actions::for_lid(
        &config,
        &Power::now(),
        actions::docked(Path::new("/sys/class/drm")),
    );
    let state = match action {
        Action::Suspend => Some("mem"),
        Action::Hibernate => Some("disk"),
        _ => None,
    };
    // A lid shut on a machine that cannot sleep that way is still locked.
    if let Some(state) = state
        && let Err(why) = alpymist_power::system::can_sleep(Path::new("/sys"), state)
    {
        eprintln!("alpymist-power: {why}; locking instead");
        return actions::run(Action::Lock, &config);
    }
    actions::run(action, &config)
}

/// Print the bar's line now and at every change: a charger, a level the
/// kernel announces, the popup saving what the bar shows — and every half
/// minute regardless, for the time left and firmware that says nothing.
fn waybar() {
    let dir = config::path().and_then(|p| p.parent().map(Path::to_path_buf));
    let changes = Changes::new(dir.as_deref());
    alpymist_widget::waybar::follow(
        || {
            let config = Config::load();
            bar::waybar(&Power::now(), Knobs::now().current(), &config.bar)
        },
        || {
            changes.wait(Duration::from_secs(30));
        },
    );
}

#[cfg(target_os = "linux")]
fn open(listener: Option<UnixListener>) -> Result<bool, String> {
    use alpymist_power::popup::Command;
    use widget::Event;

    let appearance = alpymist_widget::appearance();
    let fonts = alpymist_power::view::Fonts::load(&appearance);
    for problem in &fonts.problems {
        eprintln!("alpymist-power: font {problem}");
    }
    let (events, channel) = alpymist_widget::host::events::<Event>();

    // The watcher: a fresh reading at every change the kernel announces, and
    // every few seconds regardless, for power draw and time left.
    {
        let events = events.clone();
        std::thread::spawn(move || {
            let changes = Changes::new(None);
            loop {
                if events.send(Event::Reading(alpymist_power::read())).is_err() {
                    return;
                }
                changes.wait(Duration::from_secs(3));
            }
        });
    }

    // The worker: one command at a time, a fresh reading after each.
    let (commands, queue) = std::sync::mpsc::channel::<Command>();
    std::thread::spawn(move || {
        for command in queue {
            let result = match &command {
                Command::SetProfile(p) => actions::helper(&["profile", p.id()]),
                Command::SetChargeLimit(n) => actions::helper(&["charge-limit", &n.to_string()]),
                Command::Save(config) => config.save(),
            };
            if events.send(Event::Done(command, result)).is_err()
                || events.send(Event::Reading(alpymist_power::read())).is_err()
            {
                return;
            }
        }
    });

    alpymist_widget::host::run(
        widget::PowerPopup::new(appearance, fonts, Config::load(), commands),
        &alpymist_widget::host::Options::new(NAME),
        channel,
        listener,
    )
}

#[cfg(not(target_os = "linux"))]
fn open(_: Option<UnixListener>) -> Result<bool, String> {
    Err(
        "the popup draws on a Wayland layer surface, which needs Linux.\n\
         To see it here: cargo run -p alpymist-power --example snapshot -- out/"
            .into(),
    )
}

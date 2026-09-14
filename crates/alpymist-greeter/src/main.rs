//! The Alpymist login screen.
//!
//! On a real machine greetd starts this on its VT, with the session to launch
//! after a successful login:
//!
//! ```sh
//! alpymist-greeter --cmd 'dbus-run-session start-hyprland'
//! ```
//!
//! The command is split on whitespace, not parsed as shell: greetd already runs
//! sessions through `sh` to read the profile, so there is nothing to quote.
//!
//! During development it opens a window, and any account logs in with the
//! password `alpymist`:
//!
//! ```sh
//! cargo run -p alpymist-greeter
//! ```

#![forbid(unsafe_code)]

use alpymist_greeter::app::Power;

/// Where the last person to log in is remembered, so they are offered first.
/// The package makes it greetd's.
#[cfg(feature = "drm")]
const LAST_USER: &str = "/var/cache/alpymist-greeter/last-user";

/// The session command from `--cmd`, split into arguments.
fn session_command() -> Result<Vec<String>, String> {
    let mut args = std::env::args().skip(1);
    let mut cmd = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cmd" => cmd = args.next(),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let cmd: Vec<String> = cmd
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if cmd.is_empty() {
        return Err("no session to start: pass --cmd '<command>'".into());
    }
    Ok(cmd)
}

#[cfg(feature = "drm")]
fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The program a power action runs. Through doas, whose rule the package
/// installs: greetd's user may run exactly these two and nothing else.
fn power_command(power: Power) -> [&'static str; 3] {
    match power {
        Power::Restart => ["doas", "-n", "/sbin/reboot"],
        Power::PowerOff => ["doas", "-n", "/sbin/poweroff"],
    }
}

#[cfg(all(feature = "winit", not(feature = "drm")))]
mod preview {
    use alpymist_greeter::app::{App, Authenticator, action_for};
    use alpymist_greeter::login::Outcome;
    use alpymist_greeter::users::User;
    use denise::{Color, DamageTracker, Frame, InputEvent, Rect};
    use denise_render::Canvas;
    use denise_winit::{DeniseApp, WindowConfig, run};
    use std::sync::Arc;
    use std::time::Duration;

    struct Host {
        app: App,
    }

    impl DeniseApp for Host {
        fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
            let mut changed = false;
            for event in events {
                if let Some(action) = action_for(event) {
                    changed |= self.app.act(action);
                }
            }
            changed |= self.app.tick();
            let (clock, date) = alpymist_greeter::clock::now();
            changed |= self.app.set_time(clock, date);
            if let Some(power) = self.app.take_power() {
                eprintln!("would run: {}", super::power_command(power).join(" "));
            }
            if self.app.started {
                eprintln!("logged in; greetd would start the session now");
                std::process::exit(0);
            }
            if changed || self.app.checking() {
                damage.add_full();
            }
        }

        fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
            let mut canvas = Canvas::new(frame);
            canvas.clear(Color::rgb(0, 0, 0));
            self.app.draw(&mut canvas);
        }
    }

    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        use denise::geom::Size;

        let mut users = alpymist_greeter::users::discover();
        if users.is_empty() {
            users = vec![
                User {
                    name: "andre".into(),
                    display: "André Biseth".into(),
                    uid: 1000,
                },
                User {
                    name: "guest".into(),
                    display: "guest".into(),
                    uid: 1001,
                },
            ];
        }
        let authenticate: Authenticator = Arc::new(|_: &str, password: &str| {
            // PAM's delay on a wrong password, so the Checking state is seen.
            std::thread::sleep(Duration::from_millis(800));
            if password == "alpymist" {
                (Outcome::Started, Vec::new())
            } else {
                (
                    Outcome::Rejected("That password is not right.".into()),
                    Vec::new(),
                )
            }
        });

        let size = Size::new(1280, 800);
        let mut app = App::new(users, authenticate, size.width, size.height);
        app.hostname = "alpymist".into();
        app.keyboard = "preview".into();
        eprintln!("{}", app.face.status.describe());
        eprintln!("session: {}", super::session_command()?.join(" "));
        run(
            WindowConfig {
                title: "Alpymist Login".into(),
                size,
                ..WindowConfig::default()
            },
            Host { app },
        )?;
        Ok(())
    }
}

/// On the machine: KMS for output, evdev for input, greetd for everything else.
#[cfg(feature = "drm")]
mod console {
    use alpymist_greeter::app::{App, Authenticator, Status, action_for};
    use alpymist_greeter::login::{self, Outcome, Stream};
    use denise::geom::Rect;
    use denise::{InputEvent, InputSource, Surface};
    use denise_drm::SurfaceConfig;
    use denise_evdev::{Console, InputBackend};
    use denise_render::Canvas;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = super::session_command()?;

        // Graphics mode stops fbcon drawing over us; muting the keyboard stops
        // the VT from also receiving the password as terminal input — which,
        // left unmuted, is exactly the garbage a TUI greeter shows.
        let mut console = Console::open_if_present();
        if let Some(console) = console.as_mut() {
            console.graphics_mode()?;
            console.mute_keyboard()?;
        }
        let result = run(cmd);
        // Always hand the VT back before exiting: greetd starts the session
        // on this same terminal, and a compositor inheriting a muted keyboard
        // in graphics mode is a desktop nobody can type into.
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        result
    }

    fn run(cmd: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [SIGTERM, SIGINT, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&stop))?;
        }

        let socket = std::env::var("GREETD_SOCK").ok();
        let authenticate: Authenticator = {
            let socket = socket.clone();
            Arc::new(move |user: &str, password: &str| {
                let Some(path) = socket.as_deref() else {
                    return (
                        Outcome::Failed("Not started by greetd, so nobody can log in.".into()),
                        Vec::new(),
                    );
                };
                let mut notices = Vec::new();
                let outcome = match UnixStream::connect(path) {
                    Ok(stream) => {
                        login::attempt(&mut Stream(stream), user, password, &cmd, &mut notices)
                    }
                    Err(e) => Outcome::Failed(format!("Could not reach greetd: {e}")),
                };
                (outcome, notices)
            })
        };

        // The boot splash lets go of the display as greetd starts; wait for it.
        let mut surface = alpymist_ui::display::open_patiently(
            SurfaceConfig::default(),
            alpymist_ui::display::PATIENCE,
        )?;
        let size = surface.size();
        eprintln!("display: {}x{} via DRM/KMS", size.width, size.height);

        let mut app = App::new(
            alpymist_greeter::users::discover(),
            authenticate,
            size.width,
            size.height,
        );
        app.hostname = super::hostname();
        if let Ok(last) = std::fs::read_to_string(super::LAST_USER) {
            app.select(last.trim());
        }
        if socket.is_none() {
            app.status = Status::Problem("Not started by greetd, so nobody can log in.".into());
        }
        eprintln!("{}", app.face.status.describe());

        let mut input: Option<InputBackend> = None;
        let mut next_look = Instant::now();
        let mut events: Vec<InputEvent> = Vec::new();
        let mut dirty = true;

        loop {
            // A keyboard that enumerates a moment after the greeter starts is
            // ordinary on this hardware; keep looking until there is one.
            if input.is_none() && Instant::now() >= next_look {
                match InputBackend::open_all(size) {
                    Ok(mut backend) => {
                        let (layout, source) = backend.set_layout_from_system();
                        eprintln!("keyboard: {} (from {source})", layout.name);
                        app.keyboard = layout.name.to_string();
                        input = Some(backend);
                        dirty = true;
                    }
                    Err(_) => next_look = Instant::now() + Duration::from_secs(1),
                }
            }

            events.clear();
            if let Some(backend) = input.as_mut() {
                backend.poll(&mut events);
            }
            for event in &events {
                if let Some(action) = action_for(event) {
                    dirty |= app.act(action);
                }
            }
            dirty |= app.tick();
            let (clock, date) = alpymist_greeter::clock::now();
            dirty |= app.set_time(clock, date);

            if let Some(power) = app.take_power() {
                let [program, args @ ..] = super::power_command(power);
                let ran = std::process::Command::new(program).args(args).status();
                if !ran.is_ok_and(|s| s.success()) {
                    app.status = Status::Problem("This screen is not allowed to do that.".into());
                    dirty = true;
                }
            }

            if dirty {
                {
                    let mut frame = surface.acquire()?;
                    let mut canvas = Canvas::new(&mut frame);
                    app.draw(&mut canvas);
                }
                surface.present(&[Rect::from_size(size)])?;
                dirty = false;
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }

            if app.started {
                if let Some(user) = app.user() {
                    let _ = std::fs::write(super::LAST_USER, format!("{}\n", user.name));
                }
                eprintln!("session accepted; handing the display to it");
                return Ok(());
            }
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
        }
    }
}

fn main() {
    #[cfg(feature = "drm")]
    let result = console::main();
    #[cfg(all(feature = "winit", not(feature = "drm")))]
    let result = preview::main();
    #[cfg(not(any(feature = "winit", feature = "drm")))]
    let result: Result<(), Box<dyn std::error::Error>> =
        Err("built without a backend; enable --features winit or drm".into());

    if let Err(e) = result {
        eprintln!("alpymist-greeter: {e}");
        std::process::exit(1);
    }
}

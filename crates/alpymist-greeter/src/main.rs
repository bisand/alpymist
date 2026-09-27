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
///
/// Asked plainly, they only signal init, which stops every service and then
/// the machine. Forced, they sync the disks and stop it straight away: for when
/// init never acts on the signal, which is what happens while a boot is stuck
/// on a service that never finishes starting (busybox init reads signals only
/// between inittab's `wait` actions, and Alpine's boot is one).
fn power_command(power: Power, force: bool) -> Vec<&'static str> {
    let mut command = match power {
        Power::Restart => vec!["doas", "-n", "/sbin/reboot"],
        Power::PowerOff => vec!["doas", "-n", "/sbin/poweroff"],
    };
    if force {
        command.push("-f");
    }
    command
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
                eprintln!(
                    "would run: {}",
                    super::power_command(power, false).join(" ")
                );
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
        // A development machine will not have the installed picture:
        // `ALPYMIST_PICTURE=brand/wallpapers/milky-way.jpg` shows one.
        let picture = std::env::var_os("ALPYMIST_PICTURE")
            .unwrap_or_else(|| alpymist_greeter::app::PICTURE.into());
        app.load_picture(picture.as_ref());
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
    use alpymist_greeter::app::{App, Authenticator, Power, Status, action_for};
    use alpymist_greeter::login::{self, Outcome, Stream};
    use alpymist_greeter::vt;
    use denise::{InputEvent, InputSource};
    use denise_drm::SurfaceConfig;
    use denise_evdev::{Console, InputBackend};
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// How often to ask the kernel which console is showing. A switch back is
    /// noticed within this, and the file is one short read.
    const VT_CHECK: Duration = Duration::from_millis(250);

    /// How long a restart or power off may take to begin before it is forced.
    const FORCE_POWER_AFTER: Duration = Duration::from_secs(15);

    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = super::session_command()?;

        // Graphics mode stops fbcon drawing over us; muting the keyboard stops
        // the VT from also receiving the password as terminal input — which,
        // left unmuted, is exactly the garbage a TUI greeter shows.
        let mut console = Console::open_if_present();
        take_console(&mut console)?;
        let result = run(cmd, &mut console);
        // Always hand the VT back before exiting: greetd starts the session
        // on this same terminal, and a compositor inheriting a muted keyboard
        // in graphics mode is a desktop nobody can type into.
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        result
    }

    /// Run a restart or power off, saying whether doas let it.
    fn run_power(power: Power, force: bool) -> bool {
        let command = super::power_command(power, force);
        let (program, args) = command.split_first().expect("a command has a program");
        std::process::Command::new(program)
            .args(args)
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Graphics mode and a muted keyboard, on whatever console there is.
    fn take_console(console: &mut Option<Console>) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(console) = console.as_mut() {
            console.graphics_mode()?;
            console.mute_keyboard()?;
        }
        Ok(())
    }

    /// Show console `to`, having handed this one back first: the kernel ignores
    /// a switch away from a console in graphics mode, and its keyboard has to
    /// work over there. `chvt` waits for the switch to happen; if it has not in
    /// a couple of seconds, it never will, and it is killed rather than waited
    /// on for ever.
    fn switch_to(to: u32, console: &mut Option<Console>) -> bool {
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        let Ok(mut chvt) = std::process::Command::new("chvt")
            .arg(to.to_string())
            .spawn()
        else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match chvt.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => {
                    let _ = chvt.kill();
                    let _ = chvt.wait();
                    return false;
                }
            }
        }
    }

    /// Each login attempt, as a conversation with greetd on `socket` that
    /// starts `cmd` if the password is right.
    fn through_greetd(socket: Option<String>, cmd: Vec<String>) -> Authenticator {
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
    }

    /// The display, once the boot splash has let go of it as greetd starts.
    ///
    /// A Screen rather than the surface itself: it draws into ordinary memory
    /// and copies the result over, which is about four times faster than
    /// rasterising into the scanout mapping. See its documentation.
    fn open_screen() -> Result<alpymist_ui::display::Screen, denise_drm::DrmError> {
        alpymist_ui::display::Screen::open_patiently(
            SurfaceConfig::default(),
            alpymist_ui::display::PATIENCE,
        )
    }

    /// Every keyboard and pointer, read with the system's layout, if there are
    /// any yet.
    fn open_keyboard(size: denise::Size, app: &mut App) -> Option<InputBackend> {
        let mut backend = InputBackend::open_all(size).ok()?;
        let (layout, source) = backend.set_layout_from_system();
        eprintln!("keyboard: {} (from {source})", layout.name);
        app.keyboard = layout.name.to_string();
        Some(backend)
    }

    /// The login screen itself, before anything is drawn.
    fn new_app(socket: Option<&str>, authenticate: Authenticator, size: denise::Size) -> App {
        let mut app = App::new(
            alpymist_greeter::users::discover(),
            authenticate,
            size.width,
            size.height,
        );
        app.hostname = super::hostname();
        app.load_picture(std::path::Path::new(alpymist_greeter::app::PICTURE));
        if let Ok(last) = std::fs::read_to_string(super::LAST_USER) {
            app.select(last.trim());
        }
        if socket.is_none() {
            app.status = Status::Problem("Not started by greetd, so nobody can log in.".into());
        }
        eprintln!("{}", app.face.status.describe());
        app
    }

    /// Whether another console is showing. See the vt module.
    struct VtWatch {
        /// This screen's own console; `None` for a greeter with none.
        own: Option<u32>,
        next: Instant,
    }

    impl VtWatch {
        fn new() -> Self {
            let own = std::fs::read_to_string("/proc/self/stat")
                .ok()
                .and_then(|stat| vt::own_vt(&stat));
            Self {
                own,
                next: Instant::now(),
            }
        }

        /// Asked at most every [`VT_CHECK`]: `None` in between, and always
        /// when this screen has no console of its own.
        fn away(&mut self) -> Option<bool> {
            let own = self.own?;
            if Instant::now() < self.next {
                return None;
            }
            self.next = Instant::now() + VT_CHECK;
            let active = std::fs::read_to_string(vt::ACTIVE)
                .ok()
                .and_then(|a| vt::active_vt(&a));
            Some(active.is_some_and(|vt| vt != own))
        }

        /// Whether Ctrl+Alt+F`to` should switch: to a console that is not this one.
        fn is_elsewhere(&self, to: u32) -> bool {
            self.own.is_some_and(|own| own != to)
        }
    }

    /// A restart or power off that has been asked of init, and when.
    #[derive(Default)]
    struct PowerRequest(Option<(Power, Instant)>);

    impl PowerRequest {
        /// Ask init, and say on screen that it has been asked.
        fn ask(&mut self, power: Power, app: &mut App) {
            if run_power(power, false) {
                let doing = match power {
                    Power::Restart => "Restarting…",
                    Power::PowerOff => "Powering off…",
                };
                app.status = Status::Notice(doing.into());
                self.0 = Some((power, Instant::now()));
            } else {
                app.status = Status::Problem("This screen is not allowed to do that.".into());
            }
        }

        /// A shutdown in progress stops greetd, and this screen with it, well
        /// within [`FORCE_POWER_AFTER`]. Still here means init never took the
        /// request, so force it. Returns whether the screen has something new.
        fn force_if_ignored(&mut self, app: &mut App) -> bool {
            let Some((power, _)) = self.0.filter(|(_, at)| at.elapsed() >= FORCE_POWER_AFTER)
            else {
                return false;
            };
            eprintln!("{power:?} was not carried out; forcing it");
            self.0 = None;
            if run_power(power, true) {
                return false;
            }
            app.status = Status::Problem("This screen is not allowed to do that.".into());
            true
        }
    }

    fn run(
        cmd: Vec<String>,
        console: &mut Option<Console>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [SIGTERM, SIGINT, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&stop))?;
        }
        let socket = std::env::var("GREETD_SOCK").ok();
        let authenticate = through_greetd(socket.clone(), cmd);

        let first = open_screen()?;
        let size = first.size();
        let mut screen = Some(first);
        eprintln!("display: {}x{} via DRM/KMS", size.width, size.height);
        let mut app = new_app(socket.as_deref(), authenticate, size);

        let mut input: Option<InputBackend> = None;
        let mut next_look = Instant::now();
        let mut events: Vec<InputEvent> = Vec::new();
        let mut dirty = true;
        let mut watch = VtWatch::new();
        let mut asked = PowerRequest::default();

        loop {
            // Another console is showing: let it have the display, and stop
            // reading the keyboard, so what is typed there reaches only it.
            match watch.away() {
                Some(true) if screen.is_some() => {
                    eprintln!("another console is showing; letting go of the display");
                    screen = None;
                    input = None;
                }
                Some(false) if screen.is_none() => {
                    eprintln!("back on this console");
                    take_console(console)?;
                    screen = Some(open_screen()?);
                    next_look = Instant::now();
                    dirty = true;
                }
                _ => {}
            }
            if screen.is_none() {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                std::thread::sleep(VT_CHECK);
                continue;
            }

            // A keyboard that enumerates a moment after the greeter starts is
            // ordinary on this hardware; keep looking until there is one.
            if input.is_none() && Instant::now() >= next_look {
                input = open_keyboard(size, &mut app);
                dirty |= input.is_some();
                next_look = Instant::now() + Duration::from_secs(1);
            }

            events.clear();
            if let Some(backend) = input.as_mut() {
                backend.poll(&mut events);
            }
            let mut switch = None;
            for event in &events {
                if let Some(to) = vt::console_for(event) {
                    switch = Some(to).filter(|&to| watch.is_elsewhere(to));
                } else if let Some(action) = action_for(event) {
                    dirty |= app.act(action);
                }
            }
            if let Some(to) = switch {
                // Let go of the display and the keyboard before the console,
                // so the text console can draw and nothing typed there
                // reaches this screen. Coming back takes all three again.
                screen = None;
                input = None;
                if !switch_to(to, console) {
                    take_console(console)?;
                    screen = Some(open_screen()?);
                    app.status = Status::Problem(format!("Could not switch to console {to}."));
                    dirty = true;
                }
                watch.next = Instant::now();
                continue;
            }
            dirty |= app.tick();
            let (clock, date) = alpymist_greeter::clock::now();
            dirty |= app.set_time(clock, date);

            if let Some(power) = app.take_power() {
                asked.ask(power, &mut app);
                dirty = true;
            }
            dirty |= asked.force_if_ignored(&mut app);

            if let (true, Some(screen)) = (dirty, screen.as_mut()) {
                // The region `draw` reports is for a compositor that can be
                // told to upload less than a screen. There is none here: this
                // is the scanout mapping, and `present_with` copies the whole
                // frame into it in one pass either way.
                screen.present_with(|canvas| {
                    app.draw(canvas);
                })?;
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

#[cfg(test)]
mod tests {
    use super::power_command;
    use alpymist_greeter::app::Power;

    #[test]
    fn a_power_action_asks_init_first_and_forces_only_when_told() {
        assert_eq!(
            power_command(Power::PowerOff, false),
            ["doas", "-n", "/sbin/poweroff"]
        );
        assert_eq!(
            power_command(Power::Restart, true),
            ["doas", "-n", "/sbin/reboot", "-f"]
        );
    }
}

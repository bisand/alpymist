//! The Alpymist installer.
//!
//! On a real machine this draws straight to DRM/KMS with no compositor. During
//! development it opens a window, so the same code can be walked through in
//! seconds rather than through a three-minute image rebuild:
//!
//! ```sh
//! cargo run -p alpymist-install
//! ```
//!
//! Arrows move, Space chooses, Enter continues, Esc goes back, F10 quits.
//! Set `ALPYMIST_FONT` to a Fira Mono TTF to preview with the real typeface.
//!
//! On a real machine this installs. Set `ALPYMIST_DRY_RUN=1` to walk the whole
//! wizard and have the install step report what it *would* run without
//! touching a disk — which is how it is exercised in a VM and in tests.

#![forbid(unsafe_code)]

#[cfg(all(feature = "winit", not(feature = "drm")))]
mod run {
    use alpymist_core::Tier;
    use alpymist_install::answers::Answers;
    use alpymist_install::app::{App, action_for};
    use denise::{Color, DamageTracker, Frame, InputEvent, Rect};
    use denise_render::Canvas;
    use denise_winit::{DeniseApp, WindowConfig, run};

    /// Wraps the installer so the backend can drive it.
    struct Host {
        app: App,
    }

    impl DeniseApp for Host {
        fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
            let mut changed = false;
            for event in events {
                // Recomputed per event: moving onto a field with an arrow key
                // changes how the next keystroke should be read.
                let editing_text = self.app.focused_field().is_some();
                if let Some(action) = action_for(event, editing_text) {
                    self.app.act(action);
                    changed = true;
                }
            }
            // Any action can change the whole panel — the cursor, the button
            // states and the advisory line all move together — so there is
            // nothing to gain from tracking finer damage here.
            // The install reports progress on its own thread, so the screen has
            // to keep repainting even when nobody has touched anything.
            self.app.tick();
            if changed || self.app.installing() {
                damage.add_full();
            }
        }

        fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
            let mut canvas = Canvas::new(frame);
            canvas.clear(Color::rgb(0, 0, 0));
            self.app.draw(&mut canvas);
        }
    }

    /// Open a window and run the installer in it.
    ///
    /// # Errors
    /// Returns whatever the backend could not do.
    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        use denise::geom::Size;

        // Probing needs Linux; on a development machine there is nothing to
        // probe, so the Desktop screen simply offers the choice outright.
        let detected_tier = probe_tier();
        // A development machine may have no /sys/block, or disks nobody wants
        // offered; the preview shows stand-ins rather than an empty screen.
        let mut disks = alpymist_install::disks::discover(&alpymist_install::safety::gather());
        if disks.is_empty() {
            disks = alpymist_install::disks::sample();
        }
        let answers = Answers {
            detected_tier,
            disks,
            // A window system has already applied the keyboard layout.
            typed_by_os: true,
            ..Answers::default()
        }
        .with_defaults();

        let size = Size::new(1280, 800);
        let app = App::with_mode(answers, size.width, size.height, crate::install_mode());
        eprintln!("{}", app.face.status.describe());
        match detected_tier {
            Some(tier) => eprintln!("hardware: reports {tier:?}"),
            None => eprintln!("hardware: not probed on this platform"),
        }

        let config = WindowConfig {
            title: "Alpymist Installer".into(),
            size,
            ..WindowConfig::default()
        };
        run(config, Host { app })?;
        Ok(())
    }

    /// Ask the hardware what it can drive, where that is possible.
    fn probe_tier() -> Option<Tier> {
        alpymist_hwprobe::probe()
            .ok()
            .map(|caps| alpymist_core::select_tier(&caps).tier)
    }
}

/// Running on the machine being installed: KMS for output, evdev for input.
///
/// No compositor, no window system and no GPU driver stack — Denise rasterises
/// on the CPU and page-flips dumb buffers straight to the scanout engine. That
/// is what lets this run before any desktop exists, and why it looks the same on
/// every hardware tier.
#[cfg(feature = "drm")]
mod drm_run {
    use alpymist_core::Tier;
    use alpymist_install::answers::{Answers, Firmware};
    use alpymist_install::app::{App, action_for};
    use alpymist_install::typing;
    use denise::geom::Rect;
    use denise::{InputEvent, InputSource, Surface};
    use denise_drm::{DrmSurface, SurfaceConfig};
    use denise_evdev::{Console, InputBackend};
    use denise_render::Canvas;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// Run the installer on the console.
    ///
    /// # Errors
    /// Returns whatever the display or input layer could not do.
    pub fn main() -> Result<(), Box<dyn std::error::Error>> {
        // Take the VT out of text mode first, so the boot messages stop being
        // drawn over and the kernel stops echoing keystrokes we are about to
        // read ourselves. Absent on a system with no VTs, which is fine.
        let mut console = Console::open_if_present();
        if let Some(console) = console.as_mut() {
            console.graphics_mode()?;
            console.mute_keyboard()?;
        }

        let result = run(&mut console);

        // Always hand the console back, including on the error path: leaving a
        // VT in graphics mode with a muted keyboard is a machine that looks
        // dead to whoever is sitting at it.
        if let Some(console) = console.as_mut() {
            let _ = console.restore();
        }
        if result? {
            restart()?;
        }
        Ok(())
    }

    /// Restart the machine, after the console has been handed back.
    ///
    /// Through `reboot` rather than the syscall, so OpenRC stops services and
    /// unmounts filesystems properly. A dry run never restarts: it is how the
    /// installer is tried on machines nobody wants rebooted.
    fn restart() -> Result<(), Box<dyn std::error::Error>> {
        if crate::dry_run() {
            eprintln!("dry run: would restart now; quitting instead");
            return Ok(());
        }
        eprintln!("restarting");
        let status = std::process::Command::new("reboot").status()?;
        if !status.success() {
            return Err(format!("reboot failed: {status}").into());
        }
        Ok(())
    }

    /// Returns whether the user asked to restart.
    fn run(console: &mut Option<Console>) -> Result<bool, Box<dyn std::error::Error>> {
        let _ = console;

        // Stopping the service sends SIGTERM, and a signal's default action
        // ends the process without running Drop — so the console would stay
        // in graphics mode with its keyboard muted, a machine that looks dead.
        // Catching the signals turns them into an ordinary loop exit, and the
        // console is handed back on the way out like any other.
        let stop = Arc::new(AtomicBool::new(false));
        for signal in [SIGTERM, SIGINT, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&stop))?;
        }
        let mut surface = DrmSurface::open(SurfaceConfig::default())?;
        let size = surface.size();

        eprintln!("display: {}x{} via DRM/KMS", size.width, size.height);

        // Input devices are not necessarily there when this starts. A USB
        // keyboard enumerating a second after boot is ordinary, and on the
        // hardware this targets it is close to expected. Quitting because
        // nothing is plugged in *yet* would strand the user at a blank screen
        // with no way to find out why, so the installer draws first and keeps
        // looking.
        let mut complained = false;
        let mut input = open_input(size, &mut complained);

        let detected_tier = probe_tier();
        let disks = alpymist_install::disks::discover(&alpymist_install::safety::gather());
        eprintln!(
            "disks: {}",
            if disks.is_empty() {
                "none offered".to_string()
            } else {
                disks
                    .iter()
                    .map(|d| d.label())
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        );
        // Whatever the live system is already set to is how this keyboard
        // types right now, which makes it a better default than US.
        let configured = std::fs::read_to_string("/etc/conf.d/loadkmap")
            .ok()
            .and_then(|text| typing::configured_keymap(&text));
        let answers = Answers {
            detected_tier,
            disks,
            firmware: Firmware::detect(),
            keyboard: configured.map(|(layout, _)| layout.to_string()),
            keyboard_variant: configured.map(|(_, variant)| variant.to_string()),
            ..Answers::default()
        }
        .with_defaults();
        eprintln!("firmware: {:?}", answers.firmware);
        let mut app = App::with_mode(answers, size.width, size.height, crate::install_mode());
        eprintln!("{}", app.face.status.describe());

        let mut events: Vec<InputEvent> = Vec::new();
        let mut next_retry = Instant::now() + RETRY_EVERY;
        // The layout keys are currently translated with, once input exists.
        let mut typing_as: Option<&'static str> = None;
        loop {
            events.clear();
            match input.as_mut() {
                Some(input) => input.poll(&mut events),
                None if Instant::now() >= next_retry => {
                    input = open_input(size, &mut complained);
                    typing_as = None;
                    next_retry = Instant::now() + RETRY_EVERY;
                }
                None => {}
            }
            for event in &events {
                let editing_text = app.focused_field().is_some();
                if let Some(action) = action_for(event, editing_text) {
                    app.act(action);
                }
            }
            // Type the way the chosen layout does, as soon as it is chosen, so
            // the passwords entered later are the ones that work after boot.
            let answers = &app.wizard.answers;
            let wanted = typing::effective_layout(
                answers.keyboard.as_deref(),
                answers.keyboard_variant.as_deref(),
            );
            if let Some(input) = input.as_mut()
                && typing_as != Some(wanted.name)
            {
                input.set_layout(wanted);
                eprintln!("typing as: {}", wanted.name);
                typing_as = Some(wanted.name);
            }
            app.tick();
            if app.quitting || stop.load(Ordering::Relaxed) {
                eprintln!("stopping; giving the console back");
                return Ok(app.restart_requested);
            }

            {
                let mut frame = surface.acquire()?;
                let mut canvas = Canvas::new(&mut frame);
                app.draw(&mut canvas);
            }
            surface.present(&[Rect::from_size(size)])?;
        }
    }

    /// How often to look again when no input device has appeared yet.
    const RETRY_EVERY: Duration = Duration::from_secs(1);

    /// Open whatever input devices exist, or report that none do yet.
    ///
    /// Returns `None` rather than failing: see the call site.
    ///
    /// `complained` makes the absence of input a *state* rather than an event.
    /// Reporting it on every retry writes a line a second for as long as the
    /// machine is on — which on an installer left at the welcome screen
    /// overnight is tens of thousands of identical lines, in a log whose only
    /// job is to be readable when something has gone wrong.
    fn open_input(size: denise::geom::Size, complained: &mut bool) -> Option<InputBackend> {
        match InputBackend::open_all(size) {
            Ok(mut input) => {
                let (layout, source) = input.set_layout_from_system();
                eprintln!("keyboard: {} (from {source:?})", layout.name);
                *complained = false;
                Some(input)
            }
            Err(why) => {
                if !*complained {
                    eprintln!("input: {why}; drawing anyway and looking again every second");
                    *complained = true;
                }
                None
            }
        }
    }

    /// Ask the hardware what desktop it can drive.
    fn probe_tier() -> Option<Tier> {
        alpymist_hwprobe::probe()
            .ok()
            .map(|caps| alpymist_core::select_tier(&caps).tier)
    }
}

#[cfg(all(feature = "winit", not(feature = "drm")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(outcome) = unattended_main() {
        return outcome;
    }
    run::main()
}

#[cfg(feature = "drm")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(outcome) = unattended_main() {
        return outcome;
    }
    drm_run::main()
}

/// Handle `--unattended` if it was asked for.
#[cfg(any(feature = "winit", feature = "drm"))]
fn unattended_main() -> Option<Result<(), Box<dyn std::error::Error>>> {
    let args: Vec<String> = std::env::args().collect();
    let parsed = unattended::parse(&args)?;
    Some(match parsed.and_then(|o| unattended::run(&o)) {
        Ok(()) => Ok(()),
        Err(why) => {
            eprintln!("{why}");
            Err(why.into())
        }
    })
}

/// A non-interactive install, for machines with no input and for testing.
///
/// The wizard cannot be walked without a keyboard, and a VM frequently has
/// none — which also makes this the only way the destructive path can be
/// exercised automatically. It takes the same plan and the same safety checks
/// as the wizard; only the answers arrive differently.
#[cfg(any(feature = "winit", feature = "drm"))]
mod unattended {
    use alpymist_core::Tier;
    use alpymist_install::answers::{Answers, DiskPlan, Firmware, Network};
    use alpymist_install::execute::{self, Mode, Progress};
    use alpymist_install::plan;

    /// Command-line answers.
    pub struct Options {
        /// Disk to install to.
        pub disk: String,
        /// Login name.
        pub user: String,
        /// System hostname.
        pub hostname: String,
        /// Keyboard layout.
        pub keyboard: String,
        /// IANA timezone.
        pub timezone: String,
        /// Whether to really do it.
        pub mode: Mode,
    }

    /// Parse the arguments, or explain what they should have been.
    ///
    /// Returns `None` when `--unattended` was not asked for.
    pub fn parse(args: &[String]) -> Option<Result<Options, String>> {
        if !args.iter().any(|a| a == "--unattended") {
            return None;
        }
        let value = |name: &str| -> Option<String> {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let Some(disk) = value("--disk") else {
            return Some(Err("--unattended needs --disk /dev/...".into()));
        };
        Some(Ok(Options {
            disk,
            user: value("--user").unwrap_or_else(|| "alpymist".into()),
            hostname: value("--hostname").unwrap_or_else(|| "alpymist".into()),
            keyboard: value("--keyboard").unwrap_or_else(|| "us".into()),
            timezone: value("--timezone").unwrap_or_else(|| "UTC".into()),
            // Unattended defaults to *dry run*, unlike the wizard: a
            // command line that erases a disk when you forget a word is a
            // trap, and this one is easy to run by accident over ssh.
            mode: if args.iter().any(|a| a == "--commit") {
                Mode::Commit
            } else {
                Mode::DryRun
            },
        }))
    }

    /// Build the answers this implies.
    fn answers(o: &Options) -> Answers {
        Answers {
            keyboard: Some(o.keyboard.clone()),
            timezone: Some(o.timezone.clone()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: o.disk.clone(),
                encrypt: false,
            }),
            disk_confirmed: true,
            username: o.user.clone(),
            full_name: o.user.clone(),
            hostname: o.hostname.clone(),
            firmware: Firmware::detect(),
            detected_tier: Some(Tier::Potato),
            ..Answers::default()
        }
    }

    /// Run it, printing what happens.
    ///
    /// # Errors
    /// Returns why it could not be planned or was refused.
    pub fn run(o: &Options) -> Result<(), String> {
        let answers = answers(o);
        let plan = plan::build(&answers).map_err(|e| e.message())?;

        println!("target: {}", plan.target);
        println!(
            "mode:   {}",
            if o.mode == Mode::Commit {
                "COMMIT — this will erase the disk"
            } else {
                "dry run"
            }
        );
        for (i, step) in plan.steps.iter().enumerate() {
            println!(
                "  {}. {}{}",
                i + 1,
                step.title,
                if step.destructive {
                    "  [destructive]"
                } else {
                    ""
                }
            );
        }
        println!("---");

        let mut failed = None;
        let ok = execute::run(&plan, o.mode, &mut |progress| match progress {
            Progress::Starting {
                index,
                total,
                title,
                command,
            } => {
                println!("[{}/{total}] {title}", index + 1);
                println!("        {command}");
            }
            Progress::Output(line) => println!("        {line}"),
            Progress::Finished { .. } => {}
            Progress::Refused(reasons) => {
                failed = Some(format!("refused: {}", reasons.join("; ")));
            }
            Progress::Done { ok } => println!("--- {}", if ok { "done" } else { "stopped" }),
        });

        match (ok, failed) {
            (true, _) => Ok(()),
            (false, Some(why)) => Err(why),
            (false, None) => Err("a step failed; see the output above".into()),
        }
    }
}

/// Whether this run will really write to a disk.
///
/// A real machine installs; `ALPYMIST_DRY_RUN` makes it only report. The
/// default here is the destructive one, because that is what an installer is
/// for — but every *library* default is the opposite, so a disk can only be
/// destroyed by running the installer, never by omitting an argument.
#[cfg(any(feature = "winit", feature = "drm"))]
/// Whether this run must not touch the machine: no disk writes, no restart.
fn dry_run() -> bool {
    std::env::var_os("ALPYMIST_DRY_RUN").is_some()
}

fn install_mode() -> alpymist_install::execute::Mode {
    use alpymist_install::execute::Mode;
    if dry_run() {
        eprintln!("mode: dry run — nothing will be written to any disk");
        Mode::DryRun
    } else {
        eprintln!("mode: install — the chosen disk will be erased once confirmed");
        Mode::Commit
    }
}

#[cfg(not(any(feature = "winit", feature = "drm")))]
fn main() {
    eprintln!("alpymist-install was built without a backend; enable --features winit or drm");
    std::process::exit(2);
}

//! Settings in a window: Wayland input into the view, and changes out to the
//! settings library, one at a time on a worker thread.

use alpymist_about::info::About;
use alpymist_menu::font::LazyFont;
use alpymist_settings::{Env, Error, Settings, Value};
use alpymist_settings_app::view::{Action, Effect, Fonts, View};
use alpymist_widget::Outcome;
use alpymist_widget::host::{self, Sender};
use alpymist_widget::instance;
use alpymist_widget::window::{self, App, Cursor, Key, Mods};
use denise::{ElementState, Frame, InputEvent, KeyCode, Modifiers, Point, PointerButton, Size};
use denise_text::GlyphSource;
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

/// The app id, and the socket's name.
const NAME: &str = "alpymist-settings";
/// Alpymist's password dialog, which runs pkexec with an agent of its own.
const AUTH: &str = "/usr/bin/alpymist-auth";
/// Where `alpymist` is installed, as polkit's policy names it.
const ALPYMIST: &str = "/usr/bin/alpymist";

/// What the worker sends back.
pub enum Event {
    /// A setting was changed, or not.
    Applied {
        id: &'static str,
        result: Result<Vec<String>, String>,
        now: Result<Value, String>,
    },
    /// The About details went to the clipboard, or not.
    Copied(Result<(), String>),
}

/// A change for the worker.
struct Job {
    id: &'static str,
    value: Value,
}

struct SettingsApp {
    view: Option<View>,
    pending: Option<String>,
    jobs: mpsc::Sender<Job>,
    sender: Sender<Event>,
    started: Instant,
    pointer: Point,
    scale: u32,
}

/// Open Settings at `at`, or hand `at` to the window already open.
pub fn run(at: Option<&str>) -> Result<(), String> {
    let socket = instance::socket_path(NAME);
    if let Some(path) = &socket
        && let Ok(mut stream) = UnixStream::connect(path)
    {
        let line = at.map_or_else(|| "raise".to_owned(), |id| format!("open {id}"));
        return stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| format!("the open Settings window did not answer: {e}"));
    }
    let listener = socket.as_ref().and_then(|path| {
        std::fs::remove_file(path).ok();
        UnixListener::bind(path).ok()
    });

    let (sender, events) = host::events();
    let (jobs, queue) = mpsc::channel::<Job>();
    let worker = sender.clone();
    std::thread::spawn(move || {
        for job in queue {
            let _ = worker.send(apply(&job));
        }
    });

    let app = SettingsApp {
        view: None,
        pending: at.map(str::to_owned),
        jobs,
        sender,
        started: Instant::now(),
        pointer: Point::new(0, 0),
        scale: 1,
    };
    let options = window::Options {
        app_id: NAME.into(),
        min_size: (680, 440),
        max_size: None,
    };
    let result = window::run(app, &options, events, listener);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result
}

/// Change a setting as this person, or through pkexec when it is the
/// system's, then tell the session and read back what it is now.
fn apply(job: &Job) -> Event {
    let env = Env::detect();
    let settings = Settings::new();
    let text = job.value.to_string();
    let result = match settings.set(&env, job.id, &text, false) {
        Ok(changed) => {
            let _ = settings.live(&env, job.id, &changed.value);
            Ok(changed.notes)
        }
        Err(Error::NeedsRoot(_)) => as_root(job.id, &text).map(|()| {
            let _ = settings.live(&env, job.id, &job.value);
            Vec::new()
        }),
        Err(e) => Err(e.to_string()),
    };
    Event::Applied {
        id: job.id,
        result,
        now: settings.get(&env, job.id).map_err(|e| e.to_string()),
    }
}

/// `alpymist set ID VALUE` as root, asking for the password in Alpymist's
/// dialog.
fn as_root(id: &str, value: &str) -> Result<(), String> {
    let mut command = if Path::new(AUTH).exists() {
        let mut c = Command::new(AUTH);
        c.args(["run", "--"]);
        c
    } else {
        Command::new("pkexec")
    };
    let output = command
        .args([ALPYMIST, "set", id, value, "--no-live"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not ask for a password: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(match output.status.code() {
        Some(126) => "Cancelled.".to_owned(),
        Some(127) => "Not allowed: this needs an administrator's password.".to_owned(),
        _ => said
            .lines()
            .last()
            .unwrap_or("could not change it")
            .trim_start_matches("alpymist: ")
            .to_owned(),
    })
}

/// Start a program and leave it running.
fn spawn(argv: &[&str]) {
    if let Some((program, args)) = argv.split_first() {
        let _ = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

fn read_values(settings: &Settings) -> BTreeMap<&'static str, Result<Value, String>> {
    let env = Env::detect();
    settings
        .all()
        .iter()
        .map(|s| (s.id, settings.get(&env, s.id).map_err(|e| e.to_string())))
        .collect()
}

fn fonts() -> Fonts {
    let theme = alpymist_theme::load().file;
    let load = |path: &str| -> Option<Box<dyn GlyphSource>> {
        let bytes = std::fs::read(path).ok()?;
        LazyFont::from_vec(path, bytes)
            .ok()
            .map(|f| Box::new(f) as Box<dyn GlyphSource>)
    };
    Fonts {
        text: load(&theme.font),
        strong: theme
            .font
            .contains("Regular")
            .then(|| theme.font.replace("Regular", "SemiBold"))
            .and_then(|p| load(&p)),
        icons: load(&theme.icon_font),
    }
}

/// A window key as the physical key Denise's widgets expect.
fn key_code(key: Key) -> Option<KeyCode> {
    Some(match key {
        Key::Up => KeyCode::ArrowUp,
        Key::Down => KeyCode::ArrowDown,
        Key::Left => KeyCode::ArrowLeft,
        Key::Right => KeyCode::ArrowRight,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::Tab | Key::BackTab => KeyCode::Tab,
        Key::Enter => KeyCode::Enter,
        Key::Space => KeyCode::Space,
        Key::Escape => KeyCode::Escape,
        Key::Backspace => KeyCode::Backspace,
        Key::Delete => KeyCode::Delete,
        Key::Chord(c) => match c {
            'a' => KeyCode::A,
            'c' => KeyCode::C,
            'f' => KeyCode::F,
            'q' => KeyCode::Q,
            'v' => KeyCode::V,
            'w' => KeyCode::W,
            'x' => KeyCode::X,
            'z' => KeyCode::Z,
            _ => return None,
        },
    })
}

impl SettingsApp {
    fn now(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn feed(&mut self, events: &[InputEvent]) -> Outcome {
        let now = self.now();
        let Some(view) = self.view.as_mut() else {
            return Outcome::Unchanged;
        };
        let effects = view.handle(events, now);
        let mut outcome = Outcome::redraw_if(view.needs_paint());
        for effect in effects {
            match effect {
                Effect::Close => outcome = Outcome::Close,
                Effect::Set { id, value } => {
                    let _ = self.jobs.send(Job { id, value });
                }
                Effect::Action(Action::OpenWifi) => spawn(&["alpymist-wifi"]),
                Effect::Action(Action::OpenPower) => spawn(&["alpymist-power"]),
                Effect::Action(Action::CheckUpdates) => spawn(&["alpymist-store", "updates"]),
                Effect::Action(Action::CopyAbout) => {
                    let sender = self.sender.clone();
                    std::thread::spawn(move || {
                        let _ =
                            sender.send(Event::Copied(copy(&About::read(Path::new("/")).text())));
                    });
                }
            }
        }
        outcome
    }
}

fn copy(text: &str) -> Result<(), String> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not copy: wl-copy: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Could not copy: {e}"))?;
    }
    child
        .wait()
        .map_err(|e| format!("Could not copy: {e}"))
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "Could not copy.".to_owned())
        })
}

impl App for SettingsApp {
    type Event = Event;

    fn title(&self) -> String {
        "Settings".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        (920, 660)
    }

    fn resize(&mut self, size: Size, scale: u32) {
        self.scale = scale;
        if let Some(view) = self.view.as_mut() {
            view.resize(size, scale);
        } else {
            let settings = Settings::new();
            let values = read_values(&settings);
            let theme = alpymist_theme::load().file.denise();
            let mut view = View::new(
                size,
                scale,
                theme,
                fonts(),
                settings,
                values,
                About::read(Path::new("/")),
            );
            if let Some(id) = self.pending.take()
                && !view.open(&id)
            {
                view.toast(&format!("No setting or area `{id}`."), true);
            }
            self.view = Some(view);
        }
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if let Some(view) = self.view.as_mut() {
            view.paint(frame);
        }
    }

    fn key(&mut self, key: Key, mods: Mods) -> Outcome {
        let Some(code) = key_code(key) else {
            return Outcome::Unchanged;
        };
        let modifiers = Modifiers::NONE
            .set(Modifiers::SHIFT, mods.shift || key == Key::BackTab)
            .set(Modifiers::CTRL, mods.ctrl)
            .set(Modifiers::ALT, mods.alt);
        let down = |state| InputEvent::Key {
            code,
            state,
            repeat: false,
            modifiers,
        };
        let mut events = vec![down(ElementState::Down), down(ElementState::Up)];
        // The window sends Space as a key only; a text field wants it typed.
        if key == Key::Space {
            events.insert(1, InputEvent::Text { ch: ' ' });
        }
        self.feed(&events)
    }

    fn text(&mut self, ch: char) -> Outcome {
        self.feed(&[InputEvent::Text { ch }])
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        match at {
            Some(position) => {
                self.pointer = position;
                self.feed(&[InputEvent::PointerMoved { position }])
            }
            None => self.feed(&[InputEvent::PointerLeft]),
        }
    }

    fn press(&mut self, at: Point) -> Outcome {
        self.pointer = at;
        self.feed(&[InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ElementState::Down,
            position: at,
            modifiers: Modifiers::NONE,
        }])
    }

    fn release(&mut self, at: Point) -> Outcome {
        self.feed(&[InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ElementState::Up,
            position: at,
            modifiers: Modifiers::NONE,
        }])
    }

    fn scroll(&mut self, rows: i32) -> Outcome {
        #[allow(clippy::cast_precision_loss)]
        let delta = (rows * 48 * i32::try_from(self.scale).unwrap_or(1)) as f32;
        let position = self.pointer;
        self.feed(&[InputEvent::PointerScroll {
            delta_x: 0.0,
            delta_y: delta,
            position,
        }])
    }

    fn row_height(&self) -> f64 {
        24.0
    }

    fn cursor(&self, _at: Point) -> Cursor {
        Cursor::Default
    }

    fn animating(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|v| v.next_wake_ms().is_some())
    }

    fn tick(&mut self) -> Outcome {
        self.feed(&[])
    }

    fn event(&mut self, event: Event) -> Outcome {
        let Some(view) = self.view.as_mut() else {
            return Outcome::Unchanged;
        };
        match event {
            Event::Applied { id, result, now } => {
                view.set_value(id, now);
                match result {
                    Ok(notes) => {
                        if let Some(note) = notes.first() {
                            view.toast(note, false);
                        }
                    }
                    Err(e) => view.toast(&e, true),
                }
            }
            Event::Copied(Ok(())) => view.toast("Copied the details to the clipboard.", false),
            Event::Copied(Err(e)) => view.toast(&e, true),
        }
        Outcome::Redraw
    }

    fn message(&mut self, line: &str) -> Outcome {
        if let (Some(view), Some(id)) = (self.view.as_mut(), line.strip_prefix("open ")) {
            if !view.open(id.trim()) {
                view.toast(&format!("No setting or area `{}`.", id.trim()), true);
            }
            return Outcome::Redraw;
        }
        Outcome::Unchanged
    }
}

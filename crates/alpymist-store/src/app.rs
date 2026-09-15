//! The store in a window: the host, the view, and a worker for each source.
//!
//! Each source gets a thread that reads its catalogue as soon as the window
//! is asked for — fetching it first when there is none yet — says what is
//! installed, and then carries out operations one at a time as they are
//! queued. The window is on screen before any of that is done, and fills in
//! as the answers arrive.

use alpymist_store::config::Config;
use alpymist_store::icons::Icons;
use alpymist_store::pictures::{self, Picture, Pictures};
use alpymist_store::source::{self, Op, Source};
use alpymist_store::store::{self, Command, Event, Key, SourceInfo, Store, Target};
use alpymist_store::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Sender};
use alpymist_widget::window::{self, App, Cursor, Mods};
use alpymist_widget::{Appearance, Outcome};
use denise::Frame;
use denise::geom::{Point, Size};
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;

/// The app id, and the socket's name.
const NAME: &str = "alpymist-store";

/// Open the store, or hand `message` to the one already open.
pub fn run(config: &Config, problems: Vec<String>, message: Option<String>) -> Result<(), String> {
    let socket = socket_path();
    if let Some(path) = &socket
        && let Ok(mut stream) = UnixStream::connect(path)
    {
        let line = message.unwrap_or_else(|| "raise".into());
        stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| format!("the open store did not answer: {e}"))?;
        return Ok(());
    }
    let listener = socket.as_ref().and_then(|path| {
        std::fs::remove_file(path).ok();
        UnixListener::bind(path).ok()
    });

    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    for p in &fonts.problems {
        eprintln!("{NAME}: font {p}");
    }

    let infos: Vec<SourceInfo> = config
        .sources
        .iter()
        .map(|s| {
            let flatpak = matches!(s.kind, alpymist_store::config::Kind::Flatpak(_));
            SourceInfo {
                id: s.id.clone(),
                label: s.label.clone(),
                icon: s.icon.clone(),
                colour: s.colour,
                launches: flatpak,
                apps: flatpak,
            }
        })
        .collect();
    let mut state = Store::new(infos);
    state.problems = problems;
    if config.sources.is_empty() {
        state.problems.push("store.toml lists no sources".into());
    }

    // polkit asks this process's own agent for the password when a package
    // is installed or removed; the agent shows alpymist-auth's dialog. Kept
    // for as long as the window is open.
    let agent = match alpymist_auth::agent::register(auth_program()) {
        Ok(registration) => Some(registration),
        Err(e) => {
            state
                .problems
                .push(format!("Alpine packages cannot be changed here: {e}"));
            None
        }
    };

    let (sender, events) = host::events();
    let workers: Vec<mpsc::Sender<Op>> = config
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            spawn_worker(
                u16::try_from(i).unwrap_or(u16::MAX),
                source::open(s),
                sender.clone(),
            )
        })
        .collect();
    let launchers: Vec<Arc<dyn Source>> = config
        .sources
        .iter()
        .map(|s| Arc::from(source::open(s)))
        .collect();

    let mut app = StoreApp {
        appearance,
        fonts,
        icons: Icons::default(),
        pictures: Pictures::default(),
        sender: sender.clone(),
        store: state,
        layout: None,
        size: Size::new(0, 0),
        scale: 1,
        workers,
        launchers,
        preferred: (config.store.width, config.store.height),
    };
    if let Some(line) = message {
        app.message(&line);
    }
    let options = window::Options {
        app_id: NAME.into(),
        min_size: (560, 400),
    };
    let result = window::run(app, &options, events, listener);
    drop(agent);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result
}

/// The password dialog: `alpymist-auth` where the package puts it, or where
/// `ALPYMIST_AUTH` says, for trying a build.
fn auth_program() -> PathBuf {
    std::env::var_os("ALPYMIST_AUTH")
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from("/usr/bin/alpymist-auth"), PathBuf::from)
}

fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    Some(PathBuf::from(dir).join(format!("{NAME}-{display}.sock")))
}

/// A source's thread: load, say what is installed, then work through
/// operations.
fn spawn_worker(source: u16, backend: Box<dyn Source>, events: Sender<Event>) -> mpsc::Sender<Op> {
    let (ops, queue) = mpsc::channel::<Op>();
    std::thread::spawn(move || {
        let send = |event: Event| events.send(event).is_ok();
        if let Err(e) = backend.prepare() {
            send(Event::Status {
                source,
                text: format!("could not add the remote: {e}"),
            });
        }
        if !backend.has_catalog() {
            send(Event::Status {
                source,
                text: "Fetching the catalogue".into(),
            });
            if let Err(e) = backend.run(&Op::Refresh, &mut |_| {}) {
                send(Event::Loaded {
                    source,
                    result: Err(e),
                });
            }
        }
        let started = Instant::now();
        let loaded = backend.load();
        if let Ok(entries) = &loaded {
            eprintln!(
                "{NAME}: source {source}: {} entries in {} ms",
                entries.len(),
                started.elapsed().as_millis()
            );
        }
        let ok = loaded.is_ok();
        if !send(Event::Loaded {
            source,
            result: loaded,
        }) {
            return;
        }
        if ok
            && !send(Event::Installed {
                source,
                result: backend.installed(),
            })
        {
            return;
        }
        for op in queue {
            let result = backend.run(&op, &mut |line| {
                let _ = events.send(Event::Progress {
                    source,
                    line: line.to_owned(),
                });
            });
            let refreshed = op == Op::Refresh && result.is_ok();
            if !send(Event::Finished { source, op, result }) {
                return;
            }
            if refreshed {
                send(Event::Loaded {
                    source,
                    result: backend.load(),
                });
            }
            if !send(Event::Installed {
                source,
                result: backend.installed(),
            }) {
                return;
            }
        }
    });
    ops
}

struct StoreApp {
    appearance: Appearance,
    fonts: Fonts,
    icons: Icons,
    pictures: Pictures,
    /// Where fetched screenshots are posted.
    sender: Sender<Event>,
    store: Store,
    layout: Option<Layout>,
    size: Size,
    scale: u32,
    workers: Vec<mpsc::Sender<Op>>,
    launchers: Vec<Arc<dyn Source>>,
    preferred: (u32, u32),
}

impl StoreApp {
    fn relayout(&mut self) {
        let layout = Layout::new(
            &self.appearance,
            &mut self.fonts,
            &self.store,
            self.size,
            self.scale,
        );
        self.store.set_page(layout.rows);
        self.layout = Some(layout);
    }

    fn apply(&mut self, outcome: store::Outcome) -> Outcome {
        match outcome {
            store::Outcome::Unchanged => Outcome::Unchanged,
            store::Outcome::Redraw => Outcome::Redraw,
            store::Outcome::Close => Outcome::Close,
            store::Outcome::Run(commands) => {
                for command in commands {
                    self.carry_out(command);
                }
                Outcome::Redraw
            }
        }
    }

    fn carry_out(&mut self, command: Command) {
        match command {
            Command::Run { source, op } => {
                if let Some(worker) = self.workers.get(usize::from(source)) {
                    let _ = worker.send(op);
                }
            }
            Command::Launch { source, id } => {
                let Some(mut command) = self
                    .launchers
                    .get(usize::from(source))
                    .and_then(|s| s.launch(&id))
                else {
                    return;
                };
                spawn_detached(&mut command);
            }
            Command::Fetch(url) => {
                if self.pictures.request(&url) {
                    let sender = self.sender.clone();
                    std::thread::spawn(move || {
                        let result = pictures::fetch(&url);
                        let _ = sender.send(Event::Picture { url, result });
                    });
                }
            }
            Command::OpenUrl(url) => {
                let mut command = std::process::Command::new("sh");
                command.args(["-c", "exec ${BROWSER:-xdg-open} \"$1\"", "sh", &url]);
                spawn_detached(&mut command);
            }
        }
    }

    fn target(&self, at: Point) -> Option<Target> {
        self.layout.as_ref().and_then(|l| l.hit(&self.store, at))
    }
}

/// Start something that outlives the store.
fn spawn_detached(command: &mut std::process::Command) {
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

impl App for StoreApp {
    type Event = Event;

    fn title(&self) -> String {
        "Store".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        self.preferred
    }

    fn resize(&mut self, size: Size, scale: u32) {
        self.size = size;
        self.scale = scale;
        self.relayout();
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if self.layout.is_none() {
            self.relayout();
        }
        if let Some(layout) = &self.layout {
            view::paint(
                frame,
                layout,
                &self.appearance,
                &mut self.fonts,
                &mut self.icons,
                &mut self.pictures,
                &self.store,
            );
        }
    }

    fn key(&mut self, key: window::Key, mods: Mods) -> Outcome {
        use window::Key as W;
        let key = match key {
            W::Up => Key::Up,
            W::Down => Key::Down,
            W::Left => Key::Left,
            W::Right => Key::Right,
            W::PageUp => Key::PageUp,
            W::PageDown => Key::PageDown,
            W::Home => Key::Home,
            W::End => Key::End,
            W::Tab => Key::Tab,
            W::BackTab => Key::BackTab,
            W::Enter => Key::Enter,
            W::Space => Key::Space,
            W::Escape => Key::Escape,
            W::Backspace | W::Chord('u') if mods.ctrl => Key::Clear,
            W::Backspace => Key::Backspace,
            W::Delete => Key::Delete,
            W::Chord('q' | 'w') if mods.ctrl => Key::Quit,
            W::Chord('r') if mods.ctrl => Key::Refresh,
            W::Chord('k' | 'p') if mods.ctrl => Key::Up,
            W::Chord('j' | 'n') if mods.ctrl => Key::Down,
            W::Chord(d @ '0'..='9') if mods.ctrl || mods.alt => {
                Key::Source(u8::try_from(d.to_digit(10).unwrap_or(0)).unwrap_or(0))
            }
            W::Chord(_) => return Outcome::Unchanged,
        };
        let outcome = self.store.key(key);
        self.apply(outcome)
    }

    fn text(&mut self, ch: char) -> Outcome {
        let outcome = self.store.text(ch);
        self.apply(outcome)
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let target = at.and_then(|p| self.target(p));
        let outcome = self.store.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> Outcome {
        let outcome = match self.target(at) {
            Some(target) => self.store.click(target),
            None => store::Outcome::Unchanged,
        };
        self.apply(outcome)
    }

    fn scroll(&mut self, rows: i32) -> Outcome {
        let outcome = self.store.scroll_by(rows);
        self.apply(outcome)
    }

    fn row_height(&self) -> f64 {
        self.layout
            .as_ref()
            .map_or(48.0, |l| f64::from(l.row_h) / f64::from(l.scale))
    }

    fn cursor(&self, at: Point) -> Cursor {
        match self.target(at) {
            Some(Target::Search) => Cursor::Text,
            Some(_) => Cursor::Pointer,
            None => Cursor::Default,
        }
    }

    fn animating(&self) -> bool {
        let fetching = self
            .store
            .detail
            .and_then(|at| self.store.catalog.get(at))
            .and_then(|e| e.screenshots.get(self.store.shot))
            .is_some_and(|s| matches!(self.pictures.get(&s.url), Some(Picture::Loading)));
        self.store.animating() || self.store.confirm.is_some() || fetching
    }

    fn tick(&mut self) -> Outcome {
        let outcome = self.store.tick();
        self.apply(outcome)
    }

    fn event(&mut self, event: Event) -> Outcome {
        if let Event::Picture { url, result } = event {
            self.pictures.arrived(url, result);
            return Outcome::Redraw;
        }
        let outcome = self.store.event(event);
        self.apply(outcome)
    }

    fn message(&mut self, line: &str) -> Outcome {
        let words: Vec<&str> = line.split_whitespace().collect();
        let outcome = match words.as_slice() {
            ["show", source, id] => self.store.open_by_id(source, id),
            ["view", "installed"] => self.store.show(store::View::Installed),
            ["view", "updates"] => self.store.show(store::View::Updates),
            _ => store::Outcome::Redraw,
        };
        self.apply(outcome)
    }
}

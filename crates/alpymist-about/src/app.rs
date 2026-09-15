//! The About box in a window.

use alpymist_about::dialog::{self, Dialog, Key};
use alpymist_about::info::About;
use alpymist_about::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Sender};
use alpymist_widget::instance;
use alpymist_widget::window::{self, App, Cursor, Mods};
use alpymist_widget::{Appearance, Outcome};
use denise::Frame;
use denise::geom::{Point, Size};
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Stdio};

/// The app id, and the socket's name.
const NAME: &str = "alpymist-about";

/// What the clipboard thread sends back.
type Copied = Result<(), String>;

struct AboutApp {
    dialog: Dialog,
    appearance: Appearance,
    fonts: Fonts,
    layout: Layout,
    sender: Sender<Copied>,
}

/// Open the box, or raise the one already open.
pub fn run(about: About) -> Result<(), String> {
    let socket = instance::socket_path(NAME);
    if let Some(path) = &socket
        && let Ok(mut stream) = UnixStream::connect(path)
    {
        stream
            .write_all(b"raise\n")
            .map_err(|e| format!("the open About box did not answer: {e}"))?;
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
    let dialog = Dialog::new(about);
    let layout = Layout::new(&appearance, &dialog, 1);
    let fixed = (layout.size.width, layout.size.height);
    let (sender, events) = host::events();
    let app = AboutApp {
        dialog,
        appearance,
        fonts,
        layout,
        sender,
    };
    let options = window::Options {
        app_id: NAME.into(),
        min_size: fixed,
        max_size: Some(fixed),
    };
    let result = window::run(app, &options, events, listener);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result
}

impl AboutApp {
    fn apply(&mut self, outcome: dialog::Outcome) -> Outcome {
        match outcome {
            dialog::Outcome::Unchanged => Outcome::Unchanged,
            dialog::Outcome::Redraw => Outcome::Redraw,
            dialog::Outcome::Close => Outcome::Close,
            dialog::Outcome::Copy(text) => {
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(copy(&text));
                });
                Outcome::Unchanged
            }
        }
    }
}

/// Put `text` on the Wayland clipboard with wl-copy, which the Wayland tiers
/// install.
fn copy(text: &str) -> Copied {
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
    match child.wait() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("Could not copy: wl-copy {s}")),
        Err(e) => Err(format!("Could not copy: {e}")),
    }
}

impl App for AboutApp {
    type Event = Copied;

    fn title(&self) -> String {
        "About Alpymist".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        (self.layout.size.width, self.layout.size.height)
    }

    fn resize(&mut self, _size: Size, scale: u32) {
        self.layout = Layout::new(&self.appearance, &self.dialog, scale);
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        view::paint(
            frame,
            &self.layout,
            &self.appearance,
            &mut self.fonts,
            &self.dialog,
        );
    }

    fn key(&mut self, key: window::Key, mods: Mods) -> Outcome {
        use window::Key as W;
        let key = match key {
            W::Tab => Key::Tab,
            W::BackTab => Key::BackTab,
            W::Enter | W::Space => Key::Activate,
            W::Escape => Key::Close,
            W::Chord('q' | 'w') if mods.ctrl => Key::Close,
            W::Chord('c') if mods.ctrl => Key::Copy,
            _ => return Outcome::Unchanged,
        };
        let outcome = self.dialog.key(key);
        self.apply(outcome)
    }

    fn text(&mut self, _ch: char) -> Outcome {
        Outcome::Unchanged
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let target = at.and_then(|p| self.layout.hit(p));
        let outcome = self.dialog.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> Outcome {
        match self.layout.hit(at) {
            Some(target) => {
                let outcome = self.dialog.click(target);
                self.apply(outcome)
            }
            None => Outcome::Unchanged,
        }
    }

    fn scroll(&mut self, _rows: i32) -> Outcome {
        Outcome::Unchanged
    }

    fn cursor(&self, at: Point) -> Cursor {
        if self.layout.hit(at).is_some() {
            Cursor::Pointer
        } else {
            Cursor::Default
        }
    }

    fn event(&mut self, copied: Copied) -> Outcome {
        let outcome = self.dialog.finished_copy(copied);
        self.apply(outcome)
    }

    fn message(&mut self, _line: &str) -> Outcome {
        Outcome::Unchanged
    }
}

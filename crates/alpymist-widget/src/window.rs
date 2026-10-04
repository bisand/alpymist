//! An application window: an ordinary xdg toplevel, drawn in shared memory.
//!
//! The popups under the bar are layer surfaces the compositor places for
//! them. Something people keep open beside their work — the store — wants a
//! window instead: one the compositor tiles or floats, focuses and resizes
//! like any other. This is that host, with the same conventions as
//! [`crate::host`]: an [`App`] lays out at the size it is given and paints
//! itself, keys and clicks come in already mapped, and its own threads post
//! into the event loop through [`crate::host::Events`].
//!
//! Beyond the popup host it has more keys, cursor shapes, a timer for
//! animations, and a listener through which another run of the program
//! hands the running window something to show.

use crate::Outcome;
use crate::host::Events;
use alpymist_wayland::{
    BTN_LEFT, Event, KeyEvent, Keysym, Modifiers, Picture, Pixels, PointerEvent, PointerKind,
    Shape, Timer, Wayland, Window, WindowOptions,
};
use denise::geom::{Point, Size};
use denise::{BufferAge, Frame, PixelFormat};
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixListener;
use std::time::Duration;

/// How often an animating app is painted.
const FRAME: Duration = Duration::from_millis(40);

/// A key, as an application window sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Enter.
    Enter,
    /// Space.
    Space,
    /// Escape.
    Escape,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// A letter or digit with Ctrl or Alt held, lowercase.
    Chord(char),
}

/// Modifiers held with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    /// Ctrl.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
}

/// What the pointer should look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// The arrow.
    Default,
    /// A hand, over something that takes a click.
    Pointer,
    /// An I-beam, over text that can be typed into.
    Text,
}

/// An application a [`run`] window hosts.
pub trait App: 'static {
    /// What the application's threads send it.
    type Event: Send + 'static;

    /// The window's title.
    fn title(&self) -> String;

    /// The size to ask for before the compositor decides, in logical pixels.
    fn preferred_size(&self) -> (u32, u32);

    /// Lay out for a window of `size` physical pixels at an output scale.
    fn resize(&mut self, size: Size, scale: u32);

    /// Paint the whole window into `frame`.
    fn paint(&mut self, frame: &mut Frame<'_>);

    /// A key was pressed.
    fn key(&mut self, key: Key, mods: Mods) -> Outcome;

    /// Text was typed.
    fn text(&mut self, ch: char) -> Outcome;

    /// The pointer is at `at`, in physical pixels, or has left.
    fn pointer(&mut self, at: Option<Point>) -> Outcome;

    /// The left button was pressed at `at`.
    fn press(&mut self, at: Point) -> Outcome;

    /// The left button was let go at `at`: the end of a click or a drag.
    fn release(&mut self, at: Point) -> Outcome {
        let _ = at;
        Outcome::Unchanged
    }

    /// The wheel turned by `rows`, downwards positive.
    fn scroll(&mut self, rows: i32) -> Outcome;

    /// How far a touchpad must scroll for one row, in logical pixels.
    fn row_height(&self) -> f64 {
        48.0
    }

    /// The cursor over `at`.
    fn cursor(&self, at: Point) -> Cursor {
        let _ = at;
        Cursor::Default
    }

    /// Whether the window should be painted again every frame for now.
    fn animating(&self) -> bool {
        false
    }

    /// A frame passed while animating.
    fn tick(&mut self) -> Outcome {
        Outcome::Redraw
    }

    /// Something arrived from the application's threads.
    fn event(&mut self, event: Self::Event) -> Outcome;

    /// Another run of the program sent a line.
    fn message(&mut self, line: &str) -> Outcome {
        let _ = line;
        Outcome::Unchanged
    }
}

/// How the window is set up.
#[derive(Debug, Clone)]
pub struct Options {
    /// The app id compositors match window rules on.
    pub app_id: String,
    /// The smallest size, in logical pixels.
    pub min_size: (u32, u32),
    /// The largest, when there is one. The same as `min_size` makes a
    /// window of fixed size, which tiling compositors float, as a dialog.
    pub max_size: Option<(u32, u32)>,
}

// The flags are frame pacing, as in the popup host.
#[allow(clippy::struct_excessive_bools)]
struct Host<A: App> {
    wayland: Wayland,
    window: Window,
    /// The picture, painted here and handed over whole.
    canvas: Vec<u32>,
    cursor_serial: u32,
    cursor_shown: Option<Cursor>,
    app_id: String,

    app: A,
    scale: u32,
    /// Logical size.
    size: (u32, u32),
    modifiers: Modifiers,
    pointer_at: Option<(f64, f64)>,
    scroll_rest: f64,

    configured: bool,
    frame_pending: bool,
    dirty: bool,
    /// What paints the next frame of an animation, while there is one.
    ticking: Option<Timer>,
    exit: bool,
}

/// Open a window for `app` and run it until it closes.
///
/// `listener`, when given, is where other runs of the program connect to
/// send a line, which goes to [`App::message`].
///
/// # Errors
/// No Wayland session, or a compositor without the xdg shell.
// Owned, though only looked at: they are this loop's for as long as it
// runs, and gone when it ends.
#[allow(clippy::needless_pass_by_value)]
pub fn run<A: App>(
    app: A,
    options: &Options,
    events: Events<A::Event>,
    listener: Option<UnixListener>,
) -> Result<(), String> {
    let mut wayland = Wayland::connect()?;
    let size = app.preferred_size();
    let window = wayland.window(&WindowOptions {
        title: &app.title(),
        app_id: &options.app_id,
        min_size: options.min_size,
        max_size: options.max_size,
    })?;

    let posted = wayland
        .watch(events.fd())
        .map_err(|e| format!("event loop: {e}"))?;
    let called = match &listener {
        Some(listener) => {
            listener.set_nonblocking(true).ok();
            Some(
                wayland
                    .watch(listener.as_fd())
                    .map_err(|e| format!("event loop: {e}"))?,
            )
        }
        None => None,
    };

    let mut host = Host {
        wayland,
        window,
        canvas: Vec::new(),
        cursor_serial: 0,
        cursor_shown: None,
        app_id: options.app_id.clone(),
        app,
        scale: 1,
        size,
        modifiers: Modifiers::default(),
        pointer_at: None,
        scroll_rest: 0.0,
        configured: false,
        frame_pending: false,
        dirty: false,
        ticking: None,
        exit: false,
    };

    while !host.exit {
        for event in host.wayland.wait()? {
            match (event, &listener) {
                (Event::Ready(source), _) if source == posted => {
                    for event in events.take() {
                        let outcome = host.app.event(event);
                        host.apply(outcome);
                    }
                }
                (Event::Ready(source), Some(listener)) if Some(source) == called => {
                    host.told(listener);
                }
                (event, _) => host.on(event),
            }
        }
    }
    host.wayland.flush();
    Ok(())
}

impl<A: App> Host<A> {
    fn physical_size(&self) -> Size {
        Size::new(self.size.0 * self.scale, self.size.1 * self.scale)
    }

    fn draw(&mut self) {
        if !self.configured || self.exit {
            return;
        }
        if self.frame_pending {
            self.dirty = true;
            return;
        }
        let size = self.physical_size();
        let (Ok(w), Ok(h)) = (i32::try_from(size.width), i32::try_from(size.height)) else {
            return;
        };
        if w == 0 || h == 0 {
            return;
        }
        self.app.resize(size, self.scale);
        if let Some(at) = self.pointer_at {
            let p = self.physical(at);
            let _ = self.app.pointer(Some(p));
        }
        // The app paints the whole window, so what the frame before left
        // needs no clearing.
        self.canvas
            .resize(size.width as usize * size.height as usize, 0);
        if let Ok(mut frame) = Frame::new(
            &mut self.canvas,
            size,
            size.width,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        ) {
            self.app.paint(&mut frame);
        }
        let shown = self.wayland.show(
            self.window.surface(),
            Picture {
                size: (size.width, size.height),
                scale: self.scale,
                pixels: Pixels::Whole(&self.canvas),
                opaque: false,
                damage: None,
                paced: true,
            },
        );
        if let Err(e) = shown {
            eprintln!("{e}");
            self.exit = true;
            return;
        }
        self.frame_pending = true;
        self.dirty = false;
        self.update_cursor();
        self.arm_timer();
    }

    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Unchanged => self.update_cursor(),
            Outcome::Redraw => self.draw(),
            Outcome::Close => self.exit = true,
        }
    }

    /// Keep painting while the app animates.
    fn arm_timer(&mut self) {
        if self.ticking.is_none() && self.app.animating() {
            self.ticking = Some(self.wayland.after(FRAME));
        }
    }

    /// The animation's next frame is due.
    fn tick(&mut self) {
        self.ticking = None;
        if self.exit || !self.app.animating() {
            return;
        }
        let outcome = self.app.tick();
        self.apply(outcome);
        self.arm_timer();
    }

    fn update_cursor(&mut self) {
        let Some(at) = self.pointer_at else {
            return;
        };
        let wanted = self.app.cursor(self.physical(at));
        if self.cursor_shown == Some(wanted) {
            return;
        }
        let shape = match wanted {
            Cursor::Default => Shape::Default,
            Cursor::Pointer => Shape::Pointer,
            Cursor::Text => Shape::Text,
        };
        self.wayland.cursor(self.cursor_serial, shape);
        self.cursor_shown = Some(wanted);
    }

    /// Another run of the program has something to say: take it, and come
    /// forward.
    fn told(&mut self, listener: &UnixListener) {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        stream.set_nonblocking(false).ok();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        let mut text = String::new();
        let _ = stream.take(4096).read_to_string(&mut text);
        self.wayland.raise(&self.window, &self.app_id);
        let mut outcome = Outcome::Unchanged;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            outcome = outcome.and(self.app.message(line));
        }
        self.apply(outcome);
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let m = self.modifiers;
        let mods = Mods {
            ctrl: m.ctrl,
            shift: m.shift,
            alt: m.alt,
        };
        let key = match event.keysym {
            Keysym::Escape => Some(Key::Escape),
            Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
            Keysym::Up | Keysym::KP_Up => Some(Key::Up),
            Keysym::Down | Keysym::KP_Down => Some(Key::Down),
            Keysym::Left | Keysym::KP_Left => Some(Key::Left),
            Keysym::Right | Keysym::KP_Right => Some(Key::Right),
            Keysym::Prior | Keysym::KP_Prior => Some(Key::PageUp),
            Keysym::Next | Keysym::KP_Next => Some(Key::PageDown),
            Keysym::Home | Keysym::KP_Home => Some(Key::Home),
            Keysym::End | Keysym::KP_End => Some(Key::End),
            Keysym::ISO_Left_Tab => Some(Key::BackTab),
            Keysym::Tab if m.shift => Some(Key::BackTab),
            Keysym::Tab => Some(Key::Tab),
            Keysym::BackSpace => Some(Key::Backspace),
            Keysym::Delete | Keysym::KP_Delete => Some(Key::Delete),
            Keysym::space if !m.ctrl && !m.alt => Some(Key::Space),
            _ if m.ctrl || m.alt => event
                .keysym
                .key_char()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| Key::Chord(c.to_ascii_lowercase())),
            _ => None,
        };
        if let Some(key) = key {
            let outcome = self.app.key(key, mods);
            self.apply(outcome);
            return;
        }
        if m.ctrl || m.alt || m.logo {
            return;
        }
        if let Some(text) = &event.utf8 {
            let mut outcome = Outcome::Unchanged;
            for ch in text.chars().filter(|c| !c.is_control()) {
                outcome = outcome.and(self.app.text(ch));
            }
            self.apply(outcome);
        }
    }

    fn physical(&self, (x, y): (f64, f64)) -> Point {
        let s = f64::from(self.scale);
        #[allow(clippy::cast_possible_truncation)]
        Point::new((x * s) as i32, (y * s) as i32)
    }
}

impl<A: App> Host<A> {
    /// One thing the compositor said.
    fn on(&mut self, event: Event) {
        match event {
            Event::Configure { size: (w, h), .. } => {
                // Nothing for a side is the compositor leaving it to the
                // window, which keeps what it had.
                if w != 0 {
                    self.size.0 = w;
                }
                if h != 0 {
                    self.size.1 = h;
                }
                self.configured = true;
                // A buffer is owed for every configure, pending frame or not.
                self.frame_pending = false;
                self.draw();
            }
            Event::Closed(_) => self.exit = true,
            Event::Scale { scale, .. } => {
                if scale != self.scale {
                    self.scale = scale;
                    self.draw();
                }
            }
            Event::Frame(_) => {
                self.frame_pending = false;
                if self.dirty {
                    self.draw();
                }
            }
            // Pressed, or held long enough to count again: the same.
            Event::Key(key) => self.on_key(&key),
            Event::Modifiers(modifiers) => self.modifiers = modifiers,
            Event::Pointer(event) => self.on_pointer(&event),
            Event::Timer(timer) if self.ticking == Some(timer) => self.tick(),
            _ => {}
        }
    }

    fn on_pointer(&mut self, event: &PointerEvent) {
        if &event.surface != self.window.surface() {
            return;
        }
        let at = self.physical(event.position);
        let outcome = match event.kind {
            PointerKind::Enter { serial } => {
                self.cursor_serial = serial;
                self.cursor_shown = None;
                self.pointer_at = Some(event.position);
                self.app.pointer(Some(at))
            }
            PointerKind::Motion => {
                self.pointer_at = Some(event.position);
                self.app.pointer(Some(at))
            }
            PointerKind::Leave => {
                self.pointer_at = None;
                self.app.pointer(None)
            }
            PointerKind::Press { button } if button == BTN_LEFT => self.app.press(at),
            PointerKind::Release { button } if button == BTN_LEFT => self.app.release(at),
            PointerKind::Scroll { distance, steps } => {
                let rows = if steps != 0 {
                    steps
                } else {
                    self.scroll_rest += distance;
                    let row_h = self.app.row_height().max(1.0);
                    #[allow(clippy::cast_possible_truncation)]
                    let whole = (self.scroll_rest / row_h).trunc() as i32;
                    self.scroll_rest -= f64::from(whole) * row_h;
                    whole
                };
                if rows == 0 {
                    Outcome::Unchanged
                } else {
                    self.app.scroll(rows)
                }
            }
            _ => Outcome::Unchanged,
        };
        self.apply(outcome);
    }
}

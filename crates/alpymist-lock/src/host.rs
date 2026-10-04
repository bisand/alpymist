//! The lock on screen.
//!
//! An `ext-session-lock-v1` client, drawn in shared memory like every other
//! Alpymist surface. What that protocol gives, and a layer surface does not,
//! is the guarantee: the compositor blanks every output the moment the lock is
//! asked for, shows nothing but this client's surfaces until it is told the
//! session is unlocked, and keeps the session covered even if this process
//! dies. There is no window to click behind, and killing the lock does not
//! take it away.
//!
//! It also answers ADR 0003's lookalike question by construction. A fake lock
//! screen can be drawn — anything may cover the screen with a layer surface —
//! but it cannot be *the* lock: the compositor grants one session lock at a
//! time, and while a real one is up there is nothing else on screen to draw a
//! fake one with. If you are ever unsure which you are looking at,
//! Ctrl+Alt+Delete still answers.
//!
//! One thing is drawn here that the login screen draws for itself: the
//! pointer. The compositor's own is hidden while the lock is up, because the
//! screen behind this is the greeter's and it draws a pointer of Denise's.
//! Two cursors chasing each other is worse than either.

use crate::finger::{self, Heard};
use alpymist_greeter::app::{Action, App, Status};
use alpymist_greeter::clock;
use alpymist_wayland::{
    Area, BTN_LEFT, Event, KeyEvent, Keysym, LockSurface, Modifiers, Output, Picture, PointerEvent,
    PointerKind, Receiver, SessionLock, Shape, Surface, Timer, Wayland,
};
use denise::geom::Size;
use denise::{BufferAge, Frame, PixelFormat};
use denise_render::Canvas;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How often a password being checked is asked after. PAM waits a couple of
/// seconds before admitting a password was wrong, and this is what turns that
/// into "Checking" on screen rather than a screen that has stopped.
const CHECKING: Duration = Duration::from_millis(30);

/// What the message line says while Caps Lock is on.
const CAPS: &str = "Caps Lock is on";

/// How long to wait for a compositor's permission to draw before drawing
/// anyway.
///
/// A client is meant to paint when its frame callback arrives, and everything
/// here does. But a compositor that has nothing else to repaint may not
/// schedule one at all for a surface whose damage it has already decided it
/// does not need — and that is exactly the state a locked screen is in, with
/// no windows, no animations and nobody moving the mouse. The symptom is a
/// password that appears a character at a time only when the mouse is jogged.
///
/// So the callback is a hint rather than a condition: if one has not arrived
/// in this long and there is something to show, it is shown. Eighty
/// milliseconds is below what anybody notices between a key and a dot, and
/// far above the frame time of any machine this runs on, so a compositor
/// behaving normally never reaches it.
const IMPATIENCE: Duration = Duration::from_millis(80);

/// How the lock ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The password was right and the session is back.
    Unlocked,
    /// The compositor never granted the lock. Nearly always because one is
    /// already up — a second Super+L, or a lid closed on a locked screen — so
    /// the session is locked either way, and this is not a failure.
    Refused,
}

/// One output, and the lock's surface on it.
// The flags are the frame-pacing state machine, as in the widget host's.
#[allow(clippy::struct_excessive_bools)]
struct Screen {
    output: Output,
    surface: LockSurface,
    /// What the compositor asked for, in logical pixels.
    size: (u32, u32),
    scale: u32,
    configured: bool,
    frame_pending: bool,
    dirty: bool,
    /// Whether this surface has been given a whole frame yet. Partial damage
    /// says "the rest is as it was", and before the first commit there is no
    /// "as it was".
    fresh: bool,
    /// What is waiting to draw a frame that is owed without being asked,
    /// when something is. See [`IMPATIENCE`].
    impatient: Option<Timer>,
}

struct Lock {
    wayland: Wayland,
    /// The lock itself, for as long as the compositor grants it.
    session: Option<SessionLock>,
    screens: Vec<Screen>,

    app: App,
    /// One frame in ordinary memory, as wide and as tall as the largest
    /// surface drawn so far.
    ///
    /// Painted here, in memory that is warm and the same every frame, and
    /// handed to the compositor whole. `alpymist_ui`'s `Screen` says why for
    /// the DRM scanout, and `alpymist_wayland` does the same for a surface.
    shadow: Vec<u32>,
    modifiers: Modifiers,
    /// Called once, when the compositor grants the lock: what tells a `-f` run
    /// that the screen is covered and it may return.
    covered: Option<Box<dyn FnOnce()>>,
    /// What asks after a password being checked, while one is.
    polling: Option<Timer>,
    /// What turns the clock's minute over.
    minute: Option<Timer>,
    ending: Ending,
    exit: bool,
}

/// Lock the session and show `app` until it is unlocked.
///
/// `covered` is called once the compositor has granted the lock: from there on
/// the session is hidden, and a caller waiting to suspend may let go.
///
/// `fingers` is what the fingerprint reader says, when a finger may unlock
/// the screen too ([`finger`]).
///
/// # Errors
/// No Wayland session, or a compositor that does not speak
/// `ext-session-lock-v1`. Neither has locked anything, which is the point of
/// finding out here rather than afterwards.
pub fn run(
    app: App,
    covered: Box<dyn FnOnce()>,
    fingers: Option<Receiver<Heard>>,
) -> Result<Ending, String> {
    let mut wayland = Wayland::connect()?;
    // From here on the session is the compositor's to cover: anything that
    // fails now leaves the screen locked with nothing on it, so nothing is
    // left to fail but drawing.
    let lock = wayland.lock()?;

    let mut host = Lock {
        wayland,
        session: Some(lock),
        screens: Vec::new(),
        app,
        shadow: Vec::new(),
        modifiers: Modifiers::default(),
        covered: Some(covered),
        polling: None,
        minute: None,
        ending: Ending::Refused,
        exit: false,
    };
    host.follow_the_clock();
    let heard = match &fingers {
        Some(fingers) => Some(
            host.wayland
                .watch(fingers.fd())
                .map_err(|e| format!("event loop: {e}"))?,
        ),
        None => None,
    };

    while !host.exit {
        for event in host.wayland.wait()? {
            match (event, &fingers) {
                (Event::Ready(source), Some(fingers)) if Some(source) == heard => {
                    for said in fingers.take() {
                        host.heard(said);
                    }
                }
                (event, _) => host.on(event),
            }
        }
    }

    // Nobody is listening for a finger any more, and the reader stops when
    // it finds that out.
    drop(fingers);

    // The compositor is told, and told in the right order: unlocked first,
    // then the surfaces that are no longer wanted. A lock that is dropped
    // without this stays locked, which is the right way for this to fail and
    // the wrong way for it to finish.
    let lock = host.session.take();
    match lock {
        Some(lock) if host.ending == Ending::Unlocked => {
            host.wayland.unlock(lock);
            host.screens.clear();
        }
        lock => {
            host.screens.clear();
            drop(lock);
        }
    }
    // Without this the process could exit before the compositor has read the
    // unlock, and the session would stay locked with nothing left to unlock it.
    host.wayland.roundtrip().ok();
    Ok(host.ending)
}

impl Lock {
    /// A lock surface for `output`, unless it already has one.
    ///
    /// The protocol asks for one on every output there is and on every output
    /// that arrives, whether or not the lock has been granted yet — and makes
    /// a second one for the same output a protocol error, which is what the
    /// first check is for. The second is `finished`: there is no lock left to
    /// hang a surface on.
    fn cover(&mut self, output: &Output) {
        let Some(lock) = &self.session else {
            return;
        };
        if self.screens.iter().any(|screen| &screen.output == output) {
            return;
        }
        let surface = self.wayland.cover(lock, output);
        self.screens.push(Screen {
            output: output.clone(),
            surface,
            size: (0, 0),
            scale: 1,
            configured: false,
            frame_pending: false,
            dirty: false,
            fresh: true,
            impatient: None,
        });
    }

    fn screen_of(&self, surface: &Surface) -> Option<usize> {
        self.screens
            .iter()
            .position(|screen| screen.surface.surface() == surface)
    }

    /// Draw one screen, or note that it wants drawing once the frame in flight
    /// is done with.
    fn draw(&mut self, index: usize) {
        if self.exit {
            return;
        }
        let Some(screen) = self.screens.get_mut(index) else {
            return;
        };
        if !screen.configured {
            return;
        }
        if screen.frame_pending {
            screen.dirty = true;
            self.wait_for_it(index);
            return;
        }
        let scale = screen.scale.max(1);
        let size = Size::new(screen.size.0 * scale, screen.size.1 * scale);
        let fresh = std::mem::replace(&mut screen.fresh, false);
        let surface = screen.surface.surface().clone();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let words = size.width as usize * size.height as usize;
        if self.shadow.len() < words {
            self.shadow.resize(words, 0);
        }
        // The screen is painted edge to edge — sky, ridges, card — so there is
        // nothing to clear first, and what comes back is the part of it that
        // differs from the frame before.
        let changed = {
            let Ok(mut frame) = Frame::new(
                &mut self.shadow[..words],
                size,
                size.width,
                PixelFormat::Argb8888,
                BufferAge::Undefined,
            ) else {
                return;
            };
            let mut painting = Canvas::new(&mut frame);
            self.app.draw(&mut painting)
        };
        // Every buffer is handed over holding the whole picture, so the
        // compositor may keep whatever it has outside this rectangle: the
        // pixels there are the ones it already has. Telling it otherwise is
        // what makes typing on a machine that composites in software crawl —
        // a screen's worth of upload for a character in a field.
        let damage = (!fresh).then_some(Area {
            x: changed.x,
            y: changed.y,
            width: changed.width,
            height: changed.height,
        });
        let shown = self.wayland.show(
            &surface,
            &Picture {
                size: (size.width, size.height),
                scale,
                pixels: &self.shadow[..words],
                // Opaque: every pixel of this is painted and none of it is
                // meant to show anything through, and a compositor told
                // there is an alpha channel has to blend a screen's worth of
                // it over whatever it thinks is behind.
                opaque: true,
                damage,
                paced: true,
            },
        );
        if let Err(e) = shown {
            eprintln!("alpymist-lock: {e}, for {}x{}", size.width, size.height);
            return;
        }
        if let Some(screen) = self.screens.get_mut(index) {
            screen.frame_pending = true;
            screen.dirty = false;
        }
    }

    /// Draw `index` anyway, shortly, if the compositor has not asked by then.
    ///
    /// See [`IMPATIENCE`]. One timer at a time per screen: it is armed when a
    /// frame is owed and disarmed by the callback that makes it unnecessary.
    fn wait_for_it(&mut self, index: usize) {
        if let Some(screen) = self.screens.get_mut(index)
            && screen.impatient.is_none()
        {
            screen.impatient = Some(self.wayland.after(IMPATIENCE));
        }
    }

    /// The compositor never asked for the frame a screen owes. Draw anyway:
    /// what is on screen is older than what somebody just typed.
    fn out_of_patience(&mut self, index: usize) {
        let Some(screen) = self.screens.get_mut(index) else {
            return;
        };
        screen.impatient = None;
        if self.exit || !screen.dirty {
            return;
        }
        screen.frame_pending = false;
        self.draw(index);
    }

    /// Draw every output: what they show is one screen's worth of state.
    fn redraw(&mut self) {
        for index in 0..self.screens.len() {
            self.draw(index);
        }
    }

    /// Do what an action asks, and deal with what follows from it.
    fn act(&mut self, action: Action) {
        let changed = self.app.act(action);
        self.settle(changed);
    }

    fn settle(&mut self, changed: bool) {
        if self.app.started {
            // The password was right. Nothing more is drawn: the next thing on
            // screen is the session that was there all along.
            self.ending = Ending::Unlocked;
            self.exit = true;
            return;
        }
        if self.app.checking() {
            self.poll();
        }
        if changed {
            self.redraw();
        }
    }

    /// The fingerprint reader said something.
    fn heard(&mut self, heard: Heard) {
        let hint = match heard {
            Heard::Matched => {
                self.app.let_in();
                self.settle(true);
                return;
            }
            // A finger that was not recognised stays said until another does
            // or does not: pam_fprintd asks for the next straight away.
            Heard::Waiting if self.app.hint.as_deref() == Some(finger::NOT_THAT_FINGER) => {
                return;
            }
            Heard::Waiting => Some(finger::WAITING.to_owned()),
            Heard::Idle => None,
            Heard::NotThatFinger => Some(finger::NOT_THAT_FINGER.to_owned()),
            Heard::GaveUp(why) => Some(why),
        };
        let changed = self.app.set_hint(hint);
        self.settle(changed);
    }

    /// Ask after a password being checked until there is an answer.
    fn poll(&mut self) {
        if self.polling.is_none() {
            self.polling = Some(self.wayland.after(CHECKING));
        }
    }

    fn polled(&mut self) {
        self.polling = None;
        if self.exit || !self.app.checking() {
            return;
        }
        let changed = self.app.tick();
        self.settle(changed);
        if !self.exit {
            self.poll();
        }
    }

    /// Keep the clock right, waking once a minute rather than once a second.
    fn follow_the_clock(&mut self) {
        let (time, date) = clock::now();
        if self.app.set_time(time, date) {
            self.redraw();
        }
        self.minute = Some(self.wayland.after(until_the_next_minute()));
    }

    /// A timer ran out: which, and what it was for.
    fn timed(&mut self, timer: Timer) {
        if self.polling == Some(timer) {
            self.polled();
        } else if self.minute == Some(timer) {
            self.follow_the_clock();
        } else if let Some(index) = self
            .screens
            .iter()
            .position(|screen| screen.impatient == Some(timer))
        {
            self.out_of_patience(index);
        }
    }

    /// Caps Lock, said where a refused password would be said.
    ///
    /// Only over an idle line: why the last password was refused is worth more
    /// than a note about the keyboard, and the next keystroke clears that
    /// anyway.
    fn caps_lock(&mut self, on: bool) {
        let ours = matches!(&self.app.status, Status::Notice(text) if text == CAPS);
        if on && matches!(self.app.status, Status::Idle) {
            self.app.status = Status::Notice(CAPS.to_owned());
            self.redraw();
        } else if !on && ours {
            self.app.status = Status::Idle;
            self.redraw();
        }
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let ctrl = self.modifiers.ctrl;
        let action = match event.keysym {
            Keysym::Return | Keysym::KP_Enter => Some(Action::Submit),
            Keysym::Escape => Some(Action::Clear),
            Keysym::u if ctrl => Some(Action::Clear),
            Keysym::BackSpace => Some(Action::Backspace),
            Keysym::Delete | Keysym::KP_Delete => Some(Action::Delete),
            Keysym::Left | Keysym::KP_Left => Some(Action::CaretLeft),
            Keysym::Right | Keysym::KP_Right => Some(Action::CaretRight),
            Keysym::Home | Keysym::KP_Home => Some(Action::CaretHome),
            Keysym::End | Keysym::KP_End => Some(Action::CaretEnd),
            _ => None,
        };
        if let Some(action) = action {
            self.act(action);
            return;
        }
        // There are no shortcuts here: one field, and a modifier means a
        // keystroke meant for a desktop that is not listening.
        if ctrl || self.modifiers.alt || self.modifiers.logo {
            return;
        }
        let Some(text) = &event.utf8 else {
            return;
        };
        let mut changed = false;
        for ch in text.chars().filter(|ch| !ch.is_control()) {
            changed |= self.app.act(Action::Type(ch));
        }
        self.settle(changed);
    }

    /// A position on a surface, in the physical pixels the screen is drawn in.
    fn physical(&self, index: usize, (x, y): (f64, f64)) -> (i32, i32) {
        let scale = f64::from(
            self.screens
                .get(index)
                .map_or(1, |screen| screen.scale)
                .max(1),
        );
        #[allow(clippy::cast_possible_truncation)]
        let at = |v: f64| (v * scale) as i32;
        (at(x), at(y))
    }
}

/// How long until the clock's minute turns over.
///
/// Every time zone in use is a whole number of minutes from UTC, so the
/// seconds are the same everywhere and the system clock is enough to know
/// them without asking for the zone.
fn until_the_next_minute() -> Duration {
    let second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() % 60);
    Duration::from_secs(60 - second.min(59))
}

impl Lock {
    /// One thing the compositor said.
    fn on(&mut self, event: Event) {
        match event {
            Event::Locked => {
                // The surfaces are made as the outputs are announced, whether
                // or not the lock has been granted; this is where an output
                // that was already known when the lock was asked for is
                // caught up with.
                for output in self.wayland.outputs() {
                    self.cover(&output);
                }
                // And this is the moment somebody waiting on `-f` is waiting
                // for. Not the first frame: the compositor covers every
                // output the moment it grants the lock, drawn on or not, so
                // by here the session is already hidden — and a compositor
                // with no output at all would otherwise leave the caller
                // waiting for a frame that never comes, which is a machine
                // that will not suspend.
                if let Some(covered) = self.covered.take() {
                    covered();
                }
            }
            Event::Refused => {
                // Either the compositor refused, or a lock is already up.
                // Nothing of ours is on screen, and nothing of ours may
                // unlock what is.
                //
                // The lock object is dead from here: asking it for a surface
                // after this is an invalid object and a protocol error, and
                // that is not hypothetical — a refusal arrives within a round
                // trip of the request, which is before the registry has
                // finished announcing the outputs every one of those surfaces
                // is for. So the surfaces made while it was alive go now, in
                // the order the protocol asks for, and `cover` makes no more.
                self.screens.clear();
                self.session = None;
                self.ending = Ending::Refused;
                self.exit = true;
            }
            Event::Configure { surface, size } => self.configure(&surface, size),
            Event::Scale { surface, scale } => {
                let Some(index) = self.screen_of(&surface) else {
                    return;
                };
                match self.screens.get_mut(index) {
                    Some(screen) if screen.scale != scale => {
                        screen.scale = scale;
                        screen.fresh = true;
                    }
                    _ => return,
                }
                self.draw(index);
            }
            Event::Frame(surface) => {
                let Some(index) = self.screen_of(&surface) else {
                    return;
                };
                let dirty = match self.screens.get_mut(index) {
                    Some(screen) => {
                        screen.frame_pending = false;
                        screen.dirty
                    }
                    None => return,
                };
                if dirty {
                    self.draw(index);
                }
            }
            // Including the outputs that were there before the lock was
            // asked for: this is how every one of them is covered, not only
            // the ones plugged in afterwards.
            Event::Output(output) => self.cover(&output),
            Event::OutputGone(output) => self.screens.retain(|screen| screen.output != output),
            // Pressed, or held long enough to count again: the same.
            Event::Key(key) => self.on_key(&key),
            Event::Modifiers(modifiers) => {
                let changed = modifiers.caps_lock != self.modifiers.caps_lock;
                self.modifiers = modifiers;
                if changed {
                    self.caps_lock(modifiers.caps_lock);
                }
            }
            Event::Pointer(event) => self.on_pointer(&event),
            Event::Timer(timer) => self.timed(timer),
            // The keyboard going elsewhere among them. A popup closes when
            // it does; a lock screen has nowhere else for it to go, and
            // closing is the one thing it must not do.
            _ => {}
        }
    }

    fn configure(&mut self, surface: &Surface, (width, height): (u32, u32)) {
        let Some(index) = self.screen_of(surface) else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        if let Some(screen) = self.screens.get_mut(index) {
            let resized = screen.size != (width, height);
            screen.size = (width, height);
            screen.configured = true;
            screen.fresh |= resized;
            // A buffer is owed for every configure, pending frame or not.
            screen.frame_pending = false;
        }
        // And say that it is opaque, which it is: the mountains reach every
        // edge. A compositor that knows has nothing to blend and nothing
        // behind this to draw at all.
        self.wayland.opaque(surface, width, height);
        self.draw(index);
    }

    fn on_pointer(&mut self, event: &PointerEvent) {
        let Some(index) = self.screen_of(&event.surface) else {
            return;
        };
        match event.kind {
            PointerKind::Enter { serial } => {
                // Whatever the pointer was wearing over a window it can no
                // longer reach is not what it should wear here. The
                // compositor draws it, and nothing on this screen needs to
                // know where it is until it is clicked: following it would
                // mean a frame per motion event, and a screen's worth of
                // compositing behind every one of them.
                self.wayland.cursor(serial, Shape::Default);
            }
            PointerKind::Press { button } if button == BTN_LEFT => {
                let at = self.physical(index, event.position);
                // Where the card is depends on the screen's size: ask with
                // the layout of the screen that was clicked.
                let screen = &self.screens[index];
                let scale = screen.scale.max(1);
                self.app
                    .resize(screen.size.0 * scale, screen.size.1 * scale);
                self.act(Action::ClickAt(at.0, at.1));
            }
            _ => {}
        }
    }
}

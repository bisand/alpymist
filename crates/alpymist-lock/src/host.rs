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

use alpymist_greeter::app::{Action, App, Status};
use alpymist_greeter::clock;
use denise::geom::Size;
use denise::{BufferAge, Frame, PixelFormat};
use denise_render::Canvas;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_session_lock, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `BTN_LEFT` from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;

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
    output: wl_output::WlOutput,
    surface: SessionLockSurface,
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
    /// Whether a frame is owed and something is already waiting to draw it
    /// without being asked. See [`IMPATIENCE`].
    impatient: bool,
}

struct Lock {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    compositor: CompositorState,
    /// The lock itself, for as long as the compositor grants it.
    session: Option<SessionLock>,
    screens: Vec<Screen>,
    cursor_shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    loop_handle: LoopHandle<'static, Lock>,
    qh: QueueHandle<Lock>,

    app: App,
    /// One frame in ordinary memory, as wide and as tall as the largest
    /// surface drawn so far.
    ///
    /// Nothing rasterises into a mapping it does not own. `alpymist_ui`'s
    /// `Screen` says why for the DRM scanout, and a shm buffer has the same
    /// shape of problem for a different reason: a slot fresh from the pool is
    /// cold, so every scattered write faults a page in and every alpha blend
    /// reads it back. The screen is painted here, where the memory is warm and
    /// the same memory every frame, and copied across in one sequential pass —
    /// the trade the installer made when it went from five frames a second to
    /// sixty.
    shadow: Vec<u32>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: Modifiers,
    /// Called once, when the compositor grants the lock: what tells a `-f` run
    /// that the screen is covered and it may return.
    covered: Option<Box<dyn FnOnce()>>,
    /// Whether a password being checked is already being waited for.
    polling: bool,
    ending: Ending,
    exit: bool,
}

/// Lock the session and show `app` until it is unlocked.
///
/// `covered` is called once the compositor has granted the lock: from there on
/// the session is hidden, and a caller waiting to suspend may let go.
///
/// # Errors
/// No Wayland session, or a compositor that does not speak
/// `ext-session-lock-v1`. Neither has locked anything, which is the point of
/// finding out here rather than afterwards.
pub fn run(app: App, covered: Box<dyn FnOnce()>) -> Result<Ending, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init::<Lock>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<'static, Lock> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("event loop: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no wl_shm")?;
    // It grows to whatever the outputs turn out to need; a screen's worth
    // cannot be guessed before the first configure says how big one is.
    let pool = SlotPool::new(4096, &shm).map_err(|e| format!("shared memory: {e}"))?;
    let locks = SessionLockState::new(&globals, &qh);
    // From here on the session is the compositor's to cover: anything that
    // fails now leaves the screen locked with nothing on it, so nothing is
    // left to fail but drawing.
    let lock = locks
        .lock(&qh)
        .map_err(|_| "this compositor cannot lock a session: it has no ext-session-lock-v1")?;

    let mut host = Lock {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        compositor,
        session: Some(lock),
        screens: Vec::new(),
        cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
        cursor: None,
        loop_handle: event_loop.handle(),
        qh: qh.clone(),
        app,
        shadow: Vec::new(),
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::default(),
        covered: Some(covered),
        polling: false,
        ending: Ending::Refused,
        exit: false,
    };
    host.follow_the_clock();

    while !host.exit {
        event_loop
            .dispatch(None, &mut host)
            .map_err(|e| format!("Wayland: {e}"))?;
    }

    // The compositor is told, and told in the right order: unlocked first,
    // then the surfaces that are no longer wanted. A lock that is dropped
    // without this stays locked, which is the right way for this to fail and
    // the wrong way for it to finish.
    let lock = host.session.take();
    if host.ending == Ending::Unlocked
        && let Some(lock) = &lock
    {
        lock.unlock();
    }
    host.screens.clear();
    drop(lock);
    // Without this the process could exit before the compositor has read the
    // unlock, and the session would stay locked with nothing left to unlock it.
    conn.roundtrip().ok();
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
    fn cover(&mut self, output: &wl_output::WlOutput) {
        let Some(lock) = self.session.clone() else {
            return;
        };
        if self.screens.iter().any(|screen| &screen.output == output) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let surface = lock.create_lock_surface(surface, output, &self.qh);
        self.screens.push(Screen {
            output: output.clone(),
            surface,
            size: (0, 0),
            scale: 1,
            configured: false,
            frame_pending: false,
            dirty: false,
            fresh: true,
            impatient: false,
        });
    }

    fn screen_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.screens
            .iter()
            .position(|screen| screen.surface.wl_surface() == surface)
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
        let surface = screen.surface.wl_surface().clone();
        let (Ok(w), Ok(h)) = (i32::try_from(size.width), i32::try_from(size.height)) else {
            return;
        };
        if w == 0 || h == 0 {
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
        // Xrgb, not Argb: every pixel of this is painted and none of it is
        // meant to show anything through, and a compositor told there is an
        // alpha channel has to blend a screen's worth of it over whatever it
        // thinks is behind.
        let Ok((buffer, canvas)) = self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Xrgb8888)
        else {
            eprintln!("alpymist-lock: could not allocate a buffer for {w}x{h}");
            return;
        };
        let Ok(shared) = bytemuck::try_cast_slice_mut::<u8, u32>(canvas) else {
            return;
        };
        let Some(shared) = shared.get_mut(..words) else {
            return;
        };
        shared.copy_from_slice(&self.shadow[..words]);

        if fresh {
            // Only when it changes: a surface told its scale again has been
            // told something about every pixel of itself, and a compositor may
            // reasonably take that as "all of this is new".
            surface.set_buffer_scale(i32::try_from(scale).unwrap_or(1));
        }
        // Every buffer is handed over holding the whole picture, so the
        // compositor may keep whatever it has outside this rectangle: the
        // pixels there are the ones it already has. Telling it otherwise is
        // what makes typing on a machine that composites in software crawl —
        // a screen's worth of upload for a character in a field.
        if fresh {
            surface.damage_buffer(0, 0, w, h);
        } else {
            surface.damage_buffer(changed.x, changed.y, changed.width, changed.height);
        }
        surface.frame(&self.qh, surface.clone());
        if buffer.attach_to(&surface).is_err() {
            return;
        }
        surface.commit();
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
        match self.screens.get_mut(index) {
            Some(screen) if !screen.impatient => screen.impatient = true,
            _ => return,
        }
        let armed = self.loop_handle.insert_source(
            Timer::from_duration(IMPATIENCE),
            move |_, (), host: &mut Lock| {
                let Some(screen) = host.screens.get_mut(index) else {
                    return TimeoutAction::Drop;
                };
                screen.impatient = false;
                if host.exit || !screen.dirty {
                    return TimeoutAction::Drop;
                }
                // The compositor never asked. Draw anyway: what is on screen
                // is older than what somebody just typed.
                screen.frame_pending = false;
                host.draw(index);
                TimeoutAction::Drop
            },
        );
        if armed.is_err()
            && let Some(screen) = self.screens.get_mut(index)
        {
            screen.impatient = false;
        }
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

    /// Ask after a password being checked until there is an answer.
    fn poll(&mut self) {
        if self.polling {
            return;
        }
        self.polling = true;
        let armed = self.loop_handle.insert_source(
            Timer::from_duration(CHECKING),
            |_, (), host: &mut Lock| {
                if host.exit || !host.app.checking() {
                    host.polling = false;
                    return TimeoutAction::Drop;
                }
                let changed = host.app.tick();
                host.settle(changed);
                TimeoutAction::ToDuration(CHECKING)
            },
        );
        if armed.is_err() {
            self.polling = false;
        }
    }

    /// Keep the clock right, waking once a minute rather than once a second.
    fn follow_the_clock(&mut self) {
        let (time, date) = clock::now();
        self.app.set_time(time, date);
        self.loop_handle
            .insert_source(
                Timer::from_duration(until_the_next_minute()),
                |_, (), host: &mut Lock| {
                    if host.exit {
                        return TimeoutAction::Drop;
                    }
                    let (time, date) = clock::now();
                    if host.app.set_time(time, date) {
                        host.redraw();
                    }
                    TimeoutAction::ToDuration(until_the_next_minute())
                },
            )
            .ok();
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

impl SessionLockHandler for Lock {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // The surfaces are made as the outputs are advertised, whether or not
        // the lock has been granted; this is where an output that was already
        // known when the lock was asked for is caught up with.
        for output in self.outputs.outputs() {
            self.cover(&output);
        }
        // And this is the moment somebody waiting on `-f` is waiting for.
        // Not the first frame: the compositor covers every output the moment
        // it grants the lock, drawn on or not, so by here the session is
        // already hidden — and a compositor with no output at all would
        // otherwise leave the caller waiting for a frame that never comes,
        // which is a machine that will not suspend.
        if let Some(covered) = self.covered.take() {
            covered();
        }
    }

    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // Either the compositor refused, or a lock is already up. Nothing of
        // ours is on screen, and nothing of ours may unlock what is.
        //
        // The lock object is dead from here: asking it for a surface after
        // this is an invalid object and a protocol error, and that is not
        // hypothetical — a refusal arrives within a round trip of the request,
        // which is before the registry has finished advertising the outputs
        // every one of those surfaces is for. So the surfaces made while it
        // was alive go now, in the order the protocol asks for, and `cover`
        // makes no more.
        self.screens.clear();
        self.session = None;
        self.ending = Ending::Refused;
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self.screen_of(surface.wl_surface()) else {
            return;
        };
        let (width, height) = configure.new_size;
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
        if let Ok(region) = Region::new(&self.compositor) {
            region.add(
                0,
                0,
                i32::try_from(width).unwrap_or(0),
                i32::try_from(height).unwrap_or(0),
            );
            surface
                .wl_surface()
                .set_opaque_region(Some(region.wl_region()));
        }
        self.draw(index);
    }
}

impl CompositorHandler for Lock {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        let scale = u32::try_from(factor).unwrap_or(1).max(1);
        let Some(index) = self.screen_of(surface) else {
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

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let Some(index) = self.screen_of(surface) else {
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

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Lock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        // Including the outputs that were there before the lock was asked for:
        // this is how every one of them is covered, not only the ones plugged
        // in afterwards.
        self.cover(&output);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.screens.retain(|screen| screen.output != output);
    }
}

impl SeatHandler for Lock {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let handle = self.loop_handle.clone();
            self.keyboard = self
                .seats
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    handle,
                    Box::new(|host: &mut Self, _, event| host.on_key(&event)),
                )
                .ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
            if let (Some(pointer), Some(shapes)) = (&self.pointer, &self.cursor_shapes) {
                self.cursor = Some(shapes.get_shape_device(pointer, qh));
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Lock {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        // A popup closes when the keyboard goes elsewhere. A lock screen has
        // nowhere else for it to go, and closing is the one thing it must not
        // do.
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.on_key(&event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        let changed = modifiers.caps_lock != self.modifiers.caps_lock;
        self.modifiers = modifiers;
        if changed {
            self.caps_lock(modifiers.caps_lock);
        }
    }
}

impl PointerHandler for Lock {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(index) = self.screen_of(&event.surface) else {
                continue;
            };
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    // Whatever the pointer was wearing over a window it can no
                    // longer reach is not what it should wear here. The
                    // compositor draws it, and nothing on this screen needs
                    // to know where it is until it is clicked: following it
                    // would mean a frame per motion event, and a screen's
                    // worth of compositing behind every one of them.
                    if let Some(cursor) = &self.cursor {
                        cursor.set_shape(serial, Shape::Default);
                    }
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    let at = self.physical(index, event.position);
                    self.act(Action::ClickAt(at.0, at.1));
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Lock {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Lock {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Lock);
delegate_output!(Lock);
delegate_shm!(Lock);
delegate_seat!(Lock);
delegate_keyboard!(Lock);
delegate_pointer!(Lock);
delegate_session_lock!(Lock);
delegate_registry!(Lock);

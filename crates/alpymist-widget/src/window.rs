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
use denise::geom::{Point, Size};
use denise::{BufferAge, Frame, PixelFormat};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::reexports::calloop::channel;
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction};
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
    compositor::{CompositorHandler, CompositorState},
    delegate_activation, delegate_compositor, delegate_keyboard, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::time::Duration;

/// `BTN_LEFT` from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;

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
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    activation: Option<ActivationState>,
    cursor_shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    cursor_serial: u32,
    cursor_shown: Option<Cursor>,
    loop_handle: LoopHandle<'static, Host<A>>,
    qh: QueueHandle<Host<A>>,
    app_id: String,

    app: A,
    scale: u32,
    /// Logical size.
    size: (u32, u32),
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: Modifiers,
    pointer_at: Option<(f64, f64)>,
    scroll_rest: f64,

    configured: bool,
    frame_pending: bool,
    dirty: bool,
    ticking: bool,
    exit: bool,
}

/// Open a window for `app` and run it until it closes.
///
/// `listener`, when given, is where other runs of the program connect to
/// send a line, which goes to [`App::message`].
///
/// # Errors
/// No Wayland session, or a compositor without the xdg shell.
pub fn run<A: App>(
    app: A,
    options: &Options,
    events: Events<A::Event>,
    listener: Option<UnixListener>,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init::<Host<A>>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<'static, Host<A>> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("event loop: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let xdg = XdgShell::bind(&globals, &qh).map_err(|_| "the compositor has no xdg shell")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no wl_shm")?;

    let size = app.preferred_size();
    let window = xdg.create_window(
        compositor.create_surface(&qh),
        WindowDecorations::ServerDefault,
        &qh,
    );
    window.set_title(app.title());
    window.set_app_id(&options.app_id);
    window.set_min_size(Some(options.min_size));
    window.set_max_size(options.max_size);
    window.commit();

    let pool = SlotPool::new(size.0 as usize * size.1 as usize * 4, &shm)
        .map_err(|e| format!("shared memory: {e}"))?;

    event_loop
        .handle()
        .insert_source(events, |event, (), host: &mut Host<A>| {
            if let channel::Event::Msg(event) = event {
                let outcome = host.app.event(event);
                host.apply(outcome);
            }
        })
        .map_err(|e| format!("event loop: {e}"))?;

    if let Some(listener) = listener {
        listener.set_nonblocking(true).ok();
        event_loop
            .handle()
            .insert_source(
                Generic::new(listener, Interest::READ, Mode::Level),
                |_, listener, host: &mut Host<A>| {
                    if let Ok((stream, _)) = listener.as_ref().accept() {
                        stream.set_nonblocking(false).ok();
                        stream
                            .set_read_timeout(Some(Duration::from_millis(200)))
                            .ok();
                        let mut text = String::new();
                        let _ = stream.take(4096).read_to_string(&mut text);
                        host.raise();
                        let mut outcome = Outcome::Unchanged;
                        for line in text.lines().filter(|l| !l.trim().is_empty()) {
                            outcome = outcome.and(host.app.message(line));
                        }
                        host.apply(outcome);
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| format!("event loop: {e}"))?;
    }

    let mut host = Host {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        activation: ActivationState::bind(&globals, &qh).ok(),
        cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
        cursor: None,
        cursor_serial: 0,
        cursor_shown: None,
        loop_handle: event_loop.handle(),
        qh: qh.clone(),
        app_id: options.app_id.clone(),
        app,
        scale: 1,
        size,
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::default(),
        pointer_at: None,
        scroll_rest: 0.0,
        configured: false,
        frame_pending: false,
        dirty: false,
        ticking: false,
        exit: false,
    };

    while !host.exit {
        event_loop
            .dispatch(None, &mut host)
            .map_err(|e| format!("Wayland: {e}"))?;
    }
    conn.flush().ok();
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
        let Ok((buffer, canvas)) = self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        else {
            eprintln!("could not allocate a buffer");
            self.exit = true;
            return;
        };
        if let Ok(words) = bytemuck::try_cast_slice_mut::<u8, u32>(canvas)
            && let Ok(mut frame) = Frame::new(
                words,
                size,
                size.width,
                PixelFormat::Argb8888,
                BufferAge::Undefined,
            )
        {
            self.app.paint(&mut frame);
        }
        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, w, h);
        surface.frame(&self.qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        self.window.commit();
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
        if self.ticking || !self.app.animating() {
            return;
        }
        self.ticking = true;
        let armed = self.loop_handle.insert_source(
            Timer::from_duration(FRAME),
            |_, (), host: &mut Host<A>| {
                if host.exit || !host.app.animating() {
                    host.ticking = false;
                    return TimeoutAction::Drop;
                }
                let outcome = host.app.tick();
                host.apply(outcome);
                TimeoutAction::ToDuration(FRAME)
            },
        );
        if armed.is_err() {
            self.ticking = false;
        }
    }

    fn update_cursor(&mut self) {
        let (Some(device), Some(at)) = (&self.cursor, self.pointer_at) else {
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
        device.set_shape(self.cursor_serial, shape);
        self.cursor_shown = Some(wanted);
    }

    /// Ask the compositor to bring the window forward.
    fn raise(&self) {
        if let Some(activation) = &self.activation {
            activation.request_token(
                &self.qh,
                RequestData {
                    seat_and_serial: None,
                    surface: Some(self.window.wl_surface().clone()),
                    app_id: Some(self.app_id.clone()),
                },
            );
        }
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

impl<A: App> CompositorHandler for Host<A> {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        let scale = u32::try_from(factor).unwrap_or(1).max(1);
        if surface != self.window.wl_surface() || scale == self.scale {
            return;
        }
        surface.set_buffer_scale(factor);
        self.scale = scale;
        self.draw();
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_pending = false;
        if self.dirty {
            self.draw();
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

impl<A: App> OutputHandler for Host<A> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl<A: App> WindowHandler for Host<A> {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let (w, h) = configure.new_size;
        self.size = (
            w.map_or(self.size.0, std::num::NonZeroU32::get),
            h.map_or(self.size.1, std::num::NonZeroU32::get),
        );
        self.configured = true;
        // A buffer is owed for every configure, pending frame or not.
        self.frame_pending = false;
        self.draw();
    }
}

impl<A: App> ActivationHandler for Host<A> {
    type RequestData = RequestData;

    fn new_token(&mut self, token: String, _: &Self::RequestData) {
        if let Some(activation) = &self.activation {
            activation.activate::<Self>(self.window.wl_surface(), token);
        }
    }
}

impl<A: App> SeatHandler for Host<A> {
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
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
        if capability == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            p.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl<A: App> KeyboardHandler for Host<A> {
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
        self.modifiers = modifiers;
    }
}

impl<A: App> PointerHandler for Host<A> {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            let at = self.physical(event.position);
            let outcome = match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.cursor_serial = serial;
                    self.cursor_shown = None;
                    self.pointer_at = Some(event.position);
                    self.app.pointer(Some(at))
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_at = Some(event.position);
                    self.app.pointer(Some(at))
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_at = None;
                    self.app.pointer(None)
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => self.app.press(at),
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    self.app.release(at)
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let rows = if vertical.discrete != 0 {
                        vertical.discrete
                    } else {
                        self.scroll_rest += vertical.absolute;
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
}

impl<A: App> ShmHandler for Host<A> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl<A: App> ProvidesRegistryState for Host<A> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(@<A: App> Host<A>);
delegate_output!(@<A: App> Host<A>);
delegate_shm!(@<A: App> Host<A>);
delegate_seat!(@<A: App> Host<A>);
delegate_keyboard!(@<A: App> Host<A>);
delegate_pointer!(@<A: App> Host<A>);
delegate_xdg_shell!(@<A: App> Host<A>);
delegate_xdg_window!(@<A: App> Host<A>);
delegate_activation!(@<A: App> Host<A>);
delegate_registry!(@<A: App> Host<A>);

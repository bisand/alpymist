//! The popup on screen: a wlr layer surface under the bar, drawn in shared
//! memory — the menu's host, placed in the corner instead of the middle.
//!
//! The surface covers the whole output, bar included, and is transparent
//! except for the panel drawn in its corner. That is what makes a click
//! anywhere else close the popup — on a window, the desktop, or the Wi-Fi icon
//! that opened it. A layer with the keyboard exclusively is also given the
//! pointer exclusively by Hyprland, so a click outside it reaches no other
//! surface at all, not even one of the popup's own: whatever catches those
//! clicks has to be the popup's surface itself.
//!
//! Where the bar ends is the compositor's to know. A probe — a second layer
//! surface that keeps clear of exclusive zones and is never drawn — is sized
//! to the space the bar leaves, and the difference from the full output is
//! where the panel starts.
//!
//! iwd is driven from two threads so the surface never waits on it: a worker
//! runs one [`Command`] at a time (joining blocks for seconds), and a watcher
//! reads the state again whenever iwd signals a change, and every few seconds
//! besides, since signal strength drifts without a signal. Both post into the
//! event loop through a calloop channel, which wakes it.

use crate::Worker;
use alpymist_menu::config::Appearance;
use alpymist_wifi::popup::{Command, Key, Outcome, Popup};
use alpymist_wifi::view::{self, Fonts, Layout};
use denise::geom::Point;
use denise::{BufferAge, Frame, PixelFormat};
use smithay_client_toolkit::reexports::calloop::channel::{self, Channel};
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction,
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
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
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use std::os::unix::net::UnixListener;

/// The layer surface's namespace, for compositor rules:
/// `layerrule = match:namespace alpymist-wifi, …` in Hyprland.
pub const NAMESPACE: &str = "alpymist-wifi";

/// The namespace of the probe that measures the space the bar leaves.
pub const PROBE_NAMESPACE: &str = "alpymist-wifi-probe";

/// Gap between the popup and the bar above it, and the screen's edge.
const MARGIN: i32 = 6;

/// `BTN_LEFT` from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;

/// What the worker and watcher send.
pub enum Event {
    /// A fresh reading of iwd.
    State(alpymist_wifi::model::State),
    /// A command finished.
    Done(Command, Result<(), String>),
}

// The flags are the frame-pacing state machine, as in the menu's host.
#[allow(clippy::struct_excessive_bools)]
struct Host {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    probe: Option<LayerSurface>,
    cursor_shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    loop_handle: LoopHandle<'static, Host>,
    qh: QueueHandle<Host>,

    appearance: Appearance,
    scale: u32,
    layout: Layout,
    /// The whole output, in logical pixels, once configured.
    screen: Option<(u32, u32)>,
    /// The part of it the bar leaves, once the probe is configured.
    free: Option<(u32, u32)>,
    /// Where the panel was last drawn, in physical pixels, for damage.
    drawn: Option<denise::geom::Rect>,
    popup: Popup,
    fonts: Fonts,
    worker: Worker,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: Modifiers,
    pointer_at: Option<(f64, f64)>,
    scroll_rest: f64,

    configured: bool,
    frame_pending: bool,
    dirty: bool,
    exit: bool,
    dismissed: bool,
}

/// Open the popup and run it until it is dismissed. Returns whether it was
/// dismissed from outside — a click elsewhere, or focus going elsewhere.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell.
#[allow(clippy::too_many_lines)] // setting up two surfaces, in order
pub fn run(
    appearance: Appearance,
    fonts: Fonts,
    worker: Worker,
    events: Channel<Event>,
    toggle: Option<UnixListener>,
) -> Result<bool, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init::<Host>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<'static, Host> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("event loop: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|_| "this compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no wl_shm")?;

    let popup = Popup::new(view::ROWS);
    let layout = Layout::new(&appearance, &popup, 1);

    // Anchored to every edge, the compositor sizes it; an exclusive zone of
    // -1 puts it over the bar as well.
    let layer = layer_shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Overlay,
        Some(NAMESPACE),
        None,
    );
    layer.set_anchor(Anchor::all());
    layer.set_exclusive_zone(-1);
    // Exclusive, as the menu: typing works the moment the popup is up.
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_size(0, 0);
    layer.commit();

    let probe = layer_shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Background,
        Some(PROBE_NAMESPACE),
        None,
    );
    probe.set_anchor(Anchor::all());
    probe.set_exclusive_zone(0);
    probe.set_keyboard_interactivity(KeyboardInteractivity::None);
    probe.set_size(0, 0);
    probe.commit();
    conn.flush().ok();

    let pool = SlotPool::new(
        layout.size.width as usize * layout.size.height as usize * 4,
        &shm,
    )
    .map_err(|e| format!("shared memory: {e}"))?;

    event_loop
        .handle()
        .insert_source(events, |event, (), host: &mut Host| {
            let channel::Event::Msg(event) = event else {
                return;
            };
            let outcome = match event {
                Event::State(state) => host.popup.update(state),
                Event::Done(command, result) => host.popup.finished(&command, result),
            };
            host.apply(outcome);
        })
        .map_err(|e| format!("event loop: {e}"))?;

    if let Some(listener) = toggle {
        listener.set_nonblocking(true).ok();
        event_loop
            .handle()
            .insert_source(
                Generic::new(listener, Interest::READ, Mode::Level),
                |_, listener, host: &mut Host| {
                    listener.as_ref().accept().ok();
                    host.exit = true;
                    Ok(PostAction::Remove)
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
        layer,
        probe: Some(probe),
        cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
        cursor: None,
        loop_handle: event_loop.handle(),
        qh: qh.clone(),
        appearance,
        scale: 1,
        layout,
        screen: None,
        free: None,
        drawn: None,
        popup,
        fonts,
        worker,
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::default(),
        pointer_at: None,
        scroll_rest: 0.0,
        configured: false,
        frame_pending: false,
        dirty: false,
        exit: false,
        dismissed: false,
    };

    while !host.exit {
        event_loop
            .dispatch(None, &mut host)
            .map_err(|e| format!("Wayland: {e}"))?;
    }
    let dismissed = host.dismissed;
    drop(host.probe.take());
    drop(host.layer);
    conn.flush().ok();
    Ok(dismissed)
}

impl Host {
    fn dismiss(&mut self) {
        self.dismissed = !self.exit;
        self.exit = true;
    }

    /// The panel's top left corner on the surface, in physical pixels: in
    /// the top right corner of the space the bar leaves.
    fn origin(&self) -> Point {
        let (Some((sw, sh)), free) = (self.screen, self.free) else {
            return Point::new(0, 0);
        };
        let (_, fh) = free.unwrap_or((sw, sh));
        let s = i32::try_from(self.scale).unwrap_or(1);
        let logical = |v: u32| i32::try_from(v).unwrap_or(0);
        let (pw, _) = self.layout.logical_size();
        let x = (logical(sw) - logical(pw) - MARGIN).max(0);
        // A bar at the top takes the difference; one at the bottom would
        // too, and the panel would sit a bar's height low, which is harmless.
        let y = logical(sh.saturating_sub(fh)) + MARGIN;
        Point::new(x * s, y * s)
    }

    /// The panel's rectangle on the surface, in physical pixels.
    fn panel(&self) -> denise::geom::Rect {
        let o = self.origin();
        let size = self.layout.size;
        denise::geom::Rect::new(
            o.x,
            o.y,
            i32::try_from(size.width).unwrap_or(0),
            i32::try_from(size.height).unwrap_or(0),
        )
    }

    fn draw(&mut self) {
        // The probe is waited for, so the panel appears where it stays; a
        // compositor that never configures it still gets a popup, under the
        // top edge, once the surface itself is configured.
        if !self.configured || self.exit {
            return;
        }
        self.layout = Layout::new(&self.appearance, &self.popup, self.scale);
        if self.frame_pending {
            self.dirty = true;
            return;
        }
        let Some((sw, sh)) = self.screen else {
            return;
        };
        // The hover follows the layout, which may have moved under a still
        // pointer.
        if let Some(at) = self.pointer_at {
            let target = self.hit(at);
            if target != self.popup.hover() {
                self.popup.hover_over(target);
            }
        }
        let (Ok(w), Ok(h)) = (
            i32::try_from(sw * self.scale),
            i32::try_from(sh * self.scale),
        ) else {
            return;
        };
        let panel = self
            .panel()
            .intersect(&denise::geom::Rect::new(0, 0, w, h))
            .unwrap_or_default();
        let size = self.layout.size;
        let stride = u32::try_from(w).unwrap_or(0);
        let start = usize::try_from(panel.y).unwrap_or(0) * usize::try_from(w).unwrap_or(0)
            + usize::try_from(panel.x).unwrap_or(0);
        let fits = panel.width == i32::try_from(size.width).unwrap_or(-1)
            && panel.height == i32::try_from(size.height).unwrap_or(-1);
        let Ok((buffer, canvas)) = self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        else {
            eprintln!("alpymist-wifi: could not allocate a buffer");
            self.exit = true;
            return;
        };
        // Everything outside the panel is transparent; a slot may come back
        // holding an older frame, so the whole buffer is cleared.
        canvas.fill(0);

        if fits && let Ok(words) = bytemuck::try_cast_slice_mut::<u8, u32>(canvas) {
            if let Some(region) = words.get_mut(start..)
                && let Ok(mut frame) = Frame::new(
                    region,
                    size,
                    stride,
                    PixelFormat::Argb8888,
                    BufferAge::Undefined,
                )
            {
                view::paint(
                    &mut frame,
                    &self.layout,
                    &self.appearance,
                    &mut self.fonts,
                    &self.popup,
                );
            }
        } else if fits {
            // The pool's slots are aligned for u32, so this is not expected;
            // drawing through a copy is still better than not drawing.
            let mut words = vec![0u32; size.width as usize * size.height as usize];
            if let Ok(mut frame) = Frame::new(
                &mut words,
                size,
                size.width,
                PixelFormat::Argb8888,
                BufferAge::Undefined,
            ) {
                view::paint(
                    &mut frame,
                    &self.layout,
                    &self.appearance,
                    &mut self.fonts,
                    &self.popup,
                );
            }
            let row = size.width as usize;
            for (y, line) in words.chunks_exact(row).enumerate() {
                let at = (start + y * usize::try_from(w).unwrap_or(0)) * 4;
                if let Some(dst) = canvas.get_mut(at..at + row * 4) {
                    for (d, src) in dst.chunks_exact_mut(4).zip(line) {
                        d.copy_from_slice(&src.to_ne_bytes());
                    }
                }
            }
        }

        let surface = self.layer.wl_surface();
        // What changed is where the panel was and where it is.
        let damage = self.drawn.map_or(panel, |old| old.union(&panel));
        surface.damage_buffer(damage.x, damage.y, damage.width, damage.height);
        surface.frame(&self.qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        self.layer.commit();
        self.drawn = Some(panel);
        self.frame_pending = true;
        self.dirty = false;
    }

    /// What is under a surface-local position, if it is on the panel.
    fn hit(&self, at: (f64, f64)) -> Option<alpymist_wifi::popup::Target> {
        let point = self.physical(at);
        let o = self.origin();
        self.layout.hit(Point::new(point.x - o.x, point.y - o.y))
    }

    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Unchanged => {}
            Outcome::Redraw => self.draw(),
            Outcome::Run(command) => {
                self.worker.send(command);
                self.draw();
            }
            Outcome::Close => self.exit = true,
        }
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let ctrl = self.modifiers.ctrl;
        let key = match event.keysym {
            Keysym::Escape => Some(Key::Escape),
            Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
            Keysym::Up | Keysym::KP_Up => Some(Key::Up),
            Keysym::Down | Keysym::KP_Down => Some(Key::Down),
            Keysym::Home | Keysym::KP_Home => Some(Key::Home),
            Keysym::End | Keysym::KP_End => Some(Key::End),
            // Shift+Tab arrives as ISO_Left_Tab with most keymaps, and as Tab
            // with Shift held with the rest.
            Keysym::ISO_Left_Tab => Some(Key::BackTab),
            Keysym::Tab if self.modifiers.shift => Some(Key::BackTab),
            Keysym::Tab => Some(Key::Tab),
            Keysym::space if !ctrl => Some(Key::Space),
            Keysym::BackSpace => Some(Key::Backspace),
            Keysym::u if ctrl => Some(Key::Clear),
            Keysym::k | Keysym::p if ctrl => Some(Key::Up),
            Keysym::j | Keysym::n if ctrl => Some(Key::Down),
            Keysym::c | Keysym::g if ctrl => {
                self.exit = true;
                return;
            }
            _ => None,
        };
        if let Some(key) = key {
            let outcome = self.popup.key(key);
            self.apply(outcome);
            return;
        }
        if ctrl || self.modifiers.logo || self.modifiers.alt {
            return;
        }
        if let Some(text) = &event.utf8 {
            let mut outcome = Outcome::Unchanged;
            for ch in text.chars() {
                if self.popup.text(ch) != Outcome::Unchanged {
                    outcome = Outcome::Redraw;
                }
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

impl CompositorHandler for Host {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        let scale = u32::try_from(factor).unwrap_or(1).max(1);
        if surface != self.layer.wl_surface() || scale == self.scale {
            return;
        }
        surface.set_buffer_scale(factor);
        self.scale = scale;
        self.drawn = None;
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

impl OutputHandler for Host {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Host {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        // Either surface: the popup means nothing without the other.
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (w, h) = configure.new_size;
        if self
            .probe
            .as_ref()
            .is_some_and(|p| p.wl_surface() == layer.wl_surface())
        {
            // Measured; it is never drawn, and goes.
            self.free = Some((w, h));
            self.probe = None;
        } else {
            if w == 0 || h == 0 {
                return;
            }
            self.screen = Some((w, h));
            self.configured = true;
            // A buffer is owed for every configure, pending frame or not.
            self.frame_pending = false;
            self.drawn = None;
        }
        if self.screen.is_some() {
            self.draw();
        }
    }
}

impl SeatHandler for Host {
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
                    Box::new(|host: &mut Host, _, event| host.on_key(&event)),
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

impl KeyboardHandler for Host {
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
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Focus taken without a click — a workspace switch from the keyboard:
        // a popup left open behind it is in the way.
        if self.layer.wl_surface() == surface {
            self.dismiss();
        }
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

impl PointerHandler for Host {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            let target = self.hit(event.position);
            let outcome = match event.kind {
                PointerEventKind::Enter { serial } => {
                    // Nothing else sets the cursor over the empty part of the
                    // surface, so whatever the pointer last wore would stay.
                    if let Some(cursor) = &self.cursor {
                        cursor.set_shape(serial, Shape::Default);
                    }
                    self.pointer_at = Some(event.position);
                    self.popup.hover_over(target)
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_at = Some(event.position);
                    self.popup.hover_over(target)
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_at = None;
                    self.popup.hover_over(None)
                }
                PointerEventKind::Press { .. }
                    if !self.panel().contains(self.physical(event.position)) =>
                {
                    self.dismiss();
                    return;
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    target.map_or(Outcome::Unchanged, |t| self.popup.click(t))
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let rows = if vertical.discrete != 0 {
                        vertical.discrete
                    } else {
                        self.scroll_rest += vertical.absolute;
                        let row_h = f64::from(self.layout.unit * 9 / 4) / f64::from(self.scale);
                        #[allow(clippy::cast_possible_truncation)]
                        let whole = (self.scroll_rest / row_h).trunc() as i32;
                        self.scroll_rest -= f64::from(whole) * row_h;
                        whole
                    };
                    if rows == 0 {
                        Outcome::Unchanged
                    } else {
                        self.popup.scroll_by(rows)
                    }
                }
                _ => Outcome::Unchanged,
            };
            self.apply(outcome);
        }
    }
}

impl ShmHandler for Host {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Host {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Host);
delegate_output!(Host);
delegate_shm!(Host);
delegate_seat!(Host);
delegate_keyboard!(Host);
delegate_pointer!(Host);
delegate_layer!(Host);
delegate_registry!(Host);

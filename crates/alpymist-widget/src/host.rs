//! A widget on screen: a wlr layer surface under the bar, drawn in shared
//! memory — the menu's host, placed in the corner instead of the middle.
//!
//! The surface covers the whole output, bar included, and is transparent
//! except for the panel drawn in its corner. That is what makes a click
//! anywhere else close the popup — on a window, the desktop, or the bar icon
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
//! A widget's own threads — reading a daemon, running a command that blocks
//! — post into the event loop through the [`Events`] channel, which wakes it.

use crate::{Key, Outcome, Widget};
use denise::geom::{Point, Rect};
use denise::{BufferAge, Frame, PixelFormat};
use smithay_client_toolkit::reexports::calloop::channel::{self, Channel};
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
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

/// The sending end of a widget's events: clone it into each thread.
pub type Sender<E> = channel::Sender<E>;

/// The receiving end of a widget's events, handed to [`run`].
pub type Events<E> = Channel<E>;

/// A channel for a widget's threads to post into the event loop.
#[must_use]
pub fn events<E>() -> (Sender<E>, Events<E>) {
    channel::channel()
}

/// `BTN_LEFT` from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;

/// How a widget is put on screen.
#[derive(Debug, Clone)]
pub struct Options {
    /// The layer surface's namespace, for compositor rules:
    /// `layerrule = match:namespace NAME, no_anim on` in Hyprland. The probe
    /// takes the same with `-probe` after it.
    pub namespace: String,
    /// Gap between the panel and the bar above it, and the screen's edge, in
    /// logical pixels.
    pub margin: i32,
    /// Where the panel goes.
    pub placement: Placement,
    /// What covers the rest of the output: nothing, or a dimming shade,
    /// premultiplied `0xAARRGGBB`, for a dialog that wants the whole
    /// screen's attention.
    pub backdrop: u32,
}

/// Where a panel is put on the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// In the top right corner, under the bar: a popup.
    UnderBar,
    /// In the middle of the output: a dialog.
    Centre,
}

impl Options {
    /// The defaults, under `namespace`.
    #[must_use]
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
            margin: 6,
            placement: Placement::UnderBar,
            backdrop: 0,
        }
    }
}

// The flags are the frame-pacing state machine, as in the menu's host.
#[allow(clippy::struct_excessive_bools)]
struct Host<W: Widget> {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    probe: Option<LayerSurface>,
    cursor_shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    loop_handle: LoopHandle<'static, Host<W>>,
    qh: QueueHandle<Host<W>>,

    widget: W,
    margin: i32,
    placement: Placement,
    backdrop: u32,
    scale: u32,
    /// The panel's size, physical pixels, as last laid out.
    size: denise::geom::Size,
    /// The whole output, in logical pixels, once configured.
    screen: Option<(u32, u32)>,
    /// The part of it the bar leaves, once the probe is configured.
    free: Option<(u32, u32)>,
    /// Where the panel was last drawn, in physical pixels, for damage.
    drawn: Option<Rect>,

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
    dismissed: bool,
}

/// Open `widget` and run it until it closes. Returns whether it was
/// dismissed from outside — a click elsewhere, or focus going elsewhere —
/// which is what [`crate::instance::toggle`] wants to know.
///
/// `toggle` is the listener from [`crate::instance::toggle`]: a connection to
/// it closes the popup.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell.
#[allow(clippy::too_many_lines)] // setting up two surfaces, in order
pub fn run<W: Widget>(
    mut widget: W,
    options: &Options,
    events: Events<W::Event>,
    toggle: Option<UnixListener>,
) -> Result<bool, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init::<Host<W>>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<'static, Host<W>> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("event loop: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|_| "this compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no wl_shm")?;

    let size = widget.layout(1);

    // Anchored to every edge, the compositor sizes it; an exclusive zone of
    // -1 puts it over the bar as well.
    let layer = layer_shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Overlay,
        Some(options.namespace.clone()),
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
        Some(format!("{}-probe", options.namespace)),
        None,
    );
    probe.set_anchor(Anchor::all());
    probe.set_exclusive_zone(0);
    probe.set_keyboard_interactivity(KeyboardInteractivity::None);
    probe.set_size(0, 0);
    probe.commit();
    conn.flush().ok();

    let pool = SlotPool::new(size.width as usize * size.height as usize * 4, &shm)
        .map_err(|e| format!("shared memory: {e}"))?;

    event_loop
        .handle()
        .insert_source(events, |event, (), host: &mut Host<W>| {
            if let channel::Event::Msg(event) = event {
                let outcome = host.widget.event(event);
                host.apply(outcome);
            }
        })
        .map_err(|e| format!("event loop: {e}"))?;

    if let Some(listener) = toggle {
        listener.set_nonblocking(true).ok();
        event_loop
            .handle()
            .insert_source(
                Generic::new(listener, Interest::READ, Mode::Level),
                |_, listener, host: &mut Host<W>| {
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
        widget,
        margin: options.margin,
        placement: options.placement,
        backdrop: options.backdrop,
        scale: 1,
        size,
        screen: None,
        free: None,
        drawn: None,
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

impl<W: Widget> Host<W> {
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
        let pw = logical(self.size.width / self.scale.max(1));
        if self.placement == Placement::Centre {
            let ph = logical(self.size.height / self.scale.max(1));
            let x = ((logical(sw) - pw) / 2).max(0);
            // A little above the middle, where a dialog is looked for.
            let y = ((logical(sh) - ph) * 2 / 5).max(0);
            return Point::new(x * s, y * s);
        }
        let x = (logical(sw) - pw - self.margin).max(0);
        // A bar at the top takes the difference; one at the bottom would
        // too, and the panel would sit a bar's height low, which is harmless.
        let y = logical(sh.saturating_sub(fh)) + self.margin;
        Point::new(x * s, y * s)
    }

    /// The panel's rectangle on the surface, in physical pixels.
    fn panel(&self) -> Rect {
        let o = self.origin();
        Rect::new(
            o.x,
            o.y,
            i32::try_from(self.size.width).unwrap_or(0),
            i32::try_from(self.size.height).unwrap_or(0),
        )
    }

    /// A surface-local position, in physical pixels on the panel, if it is
    /// on the panel.
    fn on_panel(&self, at: (f64, f64)) -> Option<Point> {
        let point = self.physical(at);
        if !self.panel().contains(point) {
            return None;
        }
        let o = self.origin();
        Some(Point::new(point.x - o.x, point.y - o.y))
    }

    fn draw(&mut self) {
        // The probe is waited for, so the panel appears where it stays; a
        // compositor that never configures it still gets a popup, under the
        // top edge, once the surface itself is configured.
        if !self.configured || self.exit {
            return;
        }
        self.size = self.widget.layout(self.scale);
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
            let _ = self.widget.pointer(self.on_panel(at));
        }
        let (Ok(w), Ok(h)) = (
            i32::try_from(sw * self.scale),
            i32::try_from(sh * self.scale),
        ) else {
            return;
        };
        let panel = self
            .panel()
            .intersect(&Rect::new(0, 0, w, h))
            .unwrap_or_default();
        let size = self.size;
        let stride = u32::try_from(w).unwrap_or(0);
        let start = usize::try_from(panel.y).unwrap_or(0) * usize::try_from(w).unwrap_or(0)
            + usize::try_from(panel.x).unwrap_or(0);
        let fits = panel.width == i32::try_from(size.width).unwrap_or(-1)
            && panel.height == i32::try_from(size.height).unwrap_or(-1);
        let Ok((buffer, canvas)) = self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        else {
            eprintln!("{}: could not allocate a buffer", program());
            self.exit = true;
            return;
        };
        // Everything outside the panel is transparent, or the backdrop; a
        // slot may come back holding an older frame, so the whole buffer is
        // cleared.
        if self.backdrop == 0 {
            canvas.fill(0);
        } else {
            let shade = self.backdrop.to_ne_bytes();
            for px in canvas.chunks_exact_mut(4) {
                px.copy_from_slice(&shade);
            }
        }

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
                self.widget.paint(&mut frame);
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
                self.widget.paint(&mut frame);
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
        // What changed is where the panel was and where it is; with a
        // backdrop, the backdrop too, the first time.
        let whole = Rect::new(0, 0, w, h);
        let damage = match self.drawn {
            None if self.backdrop != 0 => whole,
            None => panel,
            Some(old) => old.union(&panel),
        };
        surface.damage_buffer(damage.x, damage.y, damage.width, damage.height);
        surface.frame(&self.qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        self.layer.commit();
        self.drawn = Some(panel);
        self.frame_pending = true;
        self.dirty = false;
        self.arm_timer();
    }

    /// Keep painting while the widget animates.
    fn arm_timer(&mut self) {
        if self.ticking || !self.widget.animating() {
            return;
        }
        self.ticking = true;
        let armed = self.loop_handle.insert_source(
            Timer::from_duration(std::time::Duration::from_millis(40)),
            |_, (), host: &mut Host<W>| {
                if host.exit || !host.widget.animating() {
                    host.ticking = false;
                    return TimeoutAction::Drop;
                }
                let outcome = host.widget.tick();
                host.apply(outcome);
                TimeoutAction::ToDuration(std::time::Duration::from_millis(40))
            },
        );
        if armed.is_err() {
            self.ticking = false;
        }
    }

    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Unchanged => {}
            Outcome::Redraw => self.draw(),
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
            Keysym::Left | Keysym::KP_Left => Some(Key::Left),
            Keysym::Right | Keysym::KP_Right => Some(Key::Right),
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
            let outcome = self.widget.key(key);
            self.apply(outcome);
            return;
        }
        if ctrl || self.modifiers.logo || self.modifiers.alt {
            return;
        }
        if let Some(text) = &event.utf8 {
            let mut outcome = Outcome::Unchanged;
            for ch in text.chars() {
                outcome = outcome.and(self.widget.text(ch));
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

/// The program's name, for the log.
fn program() -> String {
    std::env::args()
        .next()
        .and_then(|a| a.rsplit('/').next().map(str::to_owned))
        .unwrap_or_else(|| "widget".into())
}

impl<W: Widget> CompositorHandler for Host<W> {
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

impl<W: Widget> OutputHandler for Host<W> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl<W: Widget> LayerShellHandler for Host<W> {
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

impl<W: Widget> SeatHandler for Host<W> {
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

impl<W: Widget> KeyboardHandler for Host<W> {
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
        let caps = modifiers.caps_lock != self.modifiers.caps_lock;
        self.modifiers = modifiers;
        if caps {
            let outcome = self.widget.caps_lock(modifiers.caps_lock);
            self.apply(outcome);
        }
    }
}

impl<W: Widget> PointerHandler for Host<W> {
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
            let at = self.on_panel(event.position);
            let outcome = match event.kind {
                PointerEventKind::Enter { serial } => {
                    // Nothing else sets the cursor over the empty part of the
                    // surface, so whatever the pointer last wore would stay.
                    if let Some(cursor) = &self.cursor {
                        cursor.set_shape(serial, Shape::Default);
                    }
                    self.pointer_at = Some(event.position);
                    self.widget.pointer(at)
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_at = Some(event.position);
                    self.widget.pointer(at)
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_at = None;
                    self.widget.pointer(None)
                }
                PointerEventKind::Press { .. } if at.is_none() => {
                    self.dismiss();
                    return;
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    at.map_or(Outcome::Unchanged, |p| self.widget.press(p))
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let rows = if vertical.discrete != 0 {
                        vertical.discrete
                    } else {
                        self.scroll_rest += vertical.absolute;
                        let row_h = self.widget.row_height().max(1.0);
                        #[allow(clippy::cast_possible_truncation)]
                        let whole = (self.scroll_rest / row_h).trunc() as i32;
                        self.scroll_rest -= f64::from(whole) * row_h;
                        whole
                    };
                    if rows == 0 {
                        Outcome::Unchanged
                    } else {
                        self.widget.scroll(rows)
                    }
                }
                _ => Outcome::Unchanged,
            };
            self.apply(outcome);
        }
    }
}

impl<W: Widget> ShmHandler for Host<W> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl<W: Widget> ProvidesRegistryState for Host<W> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(@<W: Widget> Host<W>);
delegate_output!(@<W: Widget> Host<W>);
delegate_shm!(@<W: Widget> Host<W>);
delegate_seat!(@<W: Widget> Host<W>);
delegate_keyboard!(@<W: Widget> Host<W>);
delegate_pointer!(@<W: Widget> Host<W>);
delegate_layer!(@<W: Widget> Host<W>);
delegate_registry!(@<W: Widget> Host<W>);

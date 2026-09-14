//! The menu on screen: a wlr layer surface, drawn in shared memory.
//!
//! Why this rather than a window through winit: a launcher is judged in the
//! first hundred milliseconds, and most of what a toolkit does at start-up —
//! a GPU context, a window with decorations, a negotiation about where it
//! goes — is time the menu does not need. A layer surface is placed by the
//! compositor, over everything, with the keyboard; `wl_shm` is a file both
//! sides map, so a frame is Denise writing pixels and one commit. Nothing is
//! uploaded that the compositor would not have read anyway.
//!
//! The surface is asked for as early as possible — before the fonts load or
//! a single desktop entry is read — so the compositor's configure is already
//! on its way back while the menu builds itself.

use alpymist_menu::config::Appearance;
use alpymist_menu::menu::{Key, Menu, Outcome};
use alpymist_menu::tree::EntryId;
use alpymist_menu::view::{self, Fonts, Layout};
use denise::geom::Point;
use denise::{BufferAge, Frame, PixelFormat};
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
use std::os::unix::net::UnixListener;
use std::time::Instant;

/// The layer surface's namespace, for compositor rules:
/// `layerrule = match:namespace alpymist-menu, …` in Hyprland.
pub const NAMESPACE: &str = "alpymist-menu";

/// `BTN_LEFT` from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;

/// A connection with the surface already requested, waiting for the menu.
pub struct Pending {
    conn: Connection,
    event_loop: EventLoop<'static, Host>,
    host: Host,
}

/// Everything the Wayland handlers need.
// The flags are the frame-pacing state machine, each read in its own place;
// folding them into an enum would only hide which handler owns which.
#[allow(clippy::struct_excessive_bools)]
struct Host {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    loop_handle: LoopHandle<'static, Host>,
    qh: QueueHandle<Host>,

    appearance: Appearance,
    layout: Layout,
    menu: Option<Menu>,
    fonts: Option<Fonts>,
    notice: Option<String>,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: Modifiers,
    scroll_rest: f64,

    configured: bool,
    frame_pending: bool,
    dirty: bool,
    exit: bool,
    chosen: Option<EntryId>,

    trace: Option<Instant>,
    first_frame_logged: bool,
}

/// Connect, and ask for the surface.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell — labwc and
/// Hyprland both have it; GNOME does not.
pub fn connect(appearance: Appearance, trace: Option<Instant>) -> Result<Pending, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init::<Host>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = event_queue.handle();
    let event_loop: EventLoop<'static, Host> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("event loop: {e}"))?;

    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|_| "this compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no wl_shm")?;

    let layout = Layout::new(&appearance, 1);
    let (w, h) = layout.logical_size();
    let surface = compositor.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some(NAMESPACE), None);
    // No anchor: the compositor centres it on the focused output.
    layer.set_anchor(Anchor::empty());
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_exclusive_zone(-1);
    layer.set_size(w, h);
    layer.commit();
    conn.flush().ok();

    let pool = SlotPool::new(
        layout.size.width as usize * layout.size.height as usize * 4,
        &shm,
    )
    .map_err(|e| format!("shared memory: {e}"))?;

    let host = Host {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer,
        loop_handle: event_loop.handle(),
        qh: qh.clone(),
        appearance,
        layout,
        menu: None,
        fonts: None,
        notice: None,
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::default(),
        scroll_rest: 0.0,
        configured: false,
        frame_pending: false,
        dirty: false,
        exit: false,
        chosen: None,
        trace,
        first_frame_logged: false,
    };
    Ok(Pending {
        conn,
        event_loop,
        host,
    })
}

impl Pending {
    /// Run the menu until something is chosen or it is dismissed.
    ///
    /// The surface is destroyed before this returns, so whatever is launched
    /// next — `slurp` above all — never sees the menu still on screen.
    ///
    /// `toggle` is the listener another invocation connects to when it wants
    /// this one gone.
    ///
    /// # Errors
    /// When the connection to the compositor fails.
    pub fn run(
        mut self,
        menu: Menu,
        fonts: Fonts,
        notice: Option<String>,
        toggle: Option<UnixListener>,
    ) -> Result<(Option<EntryId>, Menu), String> {
        self.host.menu = Some(menu);
        self.host.fonts = Some(fonts);
        self.host.notice = notice;

        if let Some(listener) = toggle {
            listener.set_nonblocking(true).ok();
            self.event_loop
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

        // Nothing has been dispatched yet, so the configure the surface is
        // waiting for is read, and the first frame drawn, inside this loop.
        while !self.host.exit {
            self.event_loop
                .dispatch(None, &mut self.host)
                .map_err(|e| format!("Wayland: {e}"))?;
        }

        let Host {
            layer,
            menu,
            chosen,
            ..
        } = self.host;
        drop(layer);
        self.conn.flush().ok();
        Ok((chosen, menu.expect("the menu was set before the loop")))
    }
}

impl Host {
    fn draw(&mut self) {
        if !self.configured || self.exit {
            return;
        }
        let (Some(menu), Some(fonts)) = (self.menu.as_ref(), self.fonts.as_mut()) else {
            // Still building: paint as soon as it is ready.
            self.dirty = true;
            return;
        };
        if self.frame_pending {
            self.dirty = true;
            return;
        }
        let layout = self.layout;
        let (w, h) = (layout.size.width, layout.size.height);
        let (Ok(wi), Ok(hi)) = (i32::try_from(w), i32::try_from(h)) else {
            return;
        };
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(wi, hi, wi * 4, wl_shm::Format::Argb8888)
        else {
            eprintln!("alpymist-menu: could not allocate a buffer");
            self.exit = true;
            return;
        };

        let notice = self.notice.as_deref();
        let size = layout.size;
        if let Ok(words) = bytemuck::try_cast_slice_mut::<u8, u32>(canvas) {
            if let Ok(mut frame) =
                Frame::new(words, size, w, PixelFormat::Argb8888, BufferAge::Undefined)
            {
                view::paint(&mut frame, &layout, &self.appearance, fonts, menu, notice);
            }
        } else {
            // The pool's slots are 64-byte aligned in a page-aligned mapping,
            // so this is not expected; drawing through a copy is still better
            // than not drawing.
            let mut words = vec![0u32; (w * h) as usize];
            if let Ok(mut frame) = Frame::new(
                &mut words,
                size,
                w,
                PixelFormat::Argb8888,
                BufferAge::Undefined,
            ) {
                view::paint(&mut frame, &layout, &self.appearance, fonts, menu, notice);
            }
            for (dst, src) in canvas.chunks_exact_mut(4).zip(&words) {
                dst.copy_from_slice(&src.to_ne_bytes());
            }
        }

        let surface = self.layer.wl_surface();
        surface.damage_buffer(0, 0, wi, hi);
        surface.frame(&self.qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        self.layer.commit();
        self.frame_pending = true;
        self.dirty = false;

        if let Some(start) = self.trace
            && !self.first_frame_logged
        {
            self.first_frame_logged = true;
            eprintln!("trace: first frame committed at {:?}", start.elapsed());
        }
    }

    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Unchanged => {}
            Outcome::Redraw => self.draw(),
            Outcome::Run(entry) => {
                self.chosen = Some(entry);
                self.exit = true;
            }
            Outcome::Close => self.exit = true,
        }
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        let ctrl = self.modifiers.ctrl;
        let key = match event.keysym {
            Keysym::Escape => Some(Key::Escape),
            Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
            Keysym::Up | Keysym::KP_Up | Keysym::ISO_Left_Tab => Some(Key::Up),
            Keysym::Down | Keysym::KP_Down | Keysym::Tab => Some(Key::Down),
            Keysym::Page_Up | Keysym::KP_Page_Up => Some(Key::PageUp),
            Keysym::Page_Down | Keysym::KP_Page_Down => Some(Key::PageDown),
            Keysym::Home | Keysym::KP_Home => Some(Key::Home),
            Keysym::End | Keysym::KP_End => Some(Key::End),
            Keysym::Left | Keysym::KP_Left => Some(Key::Left),
            Keysym::Right | Keysym::KP_Right => Some(Key::Right),
            Keysym::BackSpace | Keysym::w if ctrl => Some(Key::DeleteWord),
            Keysym::BackSpace => Some(Key::Backspace),
            Keysym::u if ctrl => Some(Key::ClearQuery),
            Keysym::k | Keysym::p if ctrl => Some(Key::Up),
            Keysym::j | Keysym::n if ctrl => Some(Key::Down),
            Keysym::c | Keysym::g if ctrl => Some(Key::Escape),
            _ => None,
        };
        if let Some(key) = key {
            let outcome = match key {
                // Ctrl+C closes outright, wherever you are.
                Key::Escape if ctrl => Outcome::Close,
                key => menu.key(key),
            };
            self.apply(outcome);
            return;
        }
        if ctrl || self.modifiers.logo || self.modifiers.alt {
            return;
        }
        if let Some(text) = &event.utf8 {
            let mut outcome = Outcome::Unchanged;
            for ch in text.chars() {
                if menu.text(ch) != Outcome::Unchanged {
                    outcome = Outcome::Redraw;
                }
            }
            self.apply(outcome);
        }
    }

    fn physical(&self, (x, y): (f64, f64)) -> Point {
        let s = f64::from(self.layout.scale);
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
        if scale == self.layout.scale {
            return;
        }
        surface.set_buffer_scale(factor);
        self.layout = Layout::new(&self.appearance, scale);
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
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        // The size is ours to choose and does not change; a configure only
        // says the surface may now be drawn. A buffer is still owed for every
        // configure, so a pending frame does not excuse this one.
        self.configured = true;
        self.frame_pending = false;
        if let Some(start) = self.trace {
            eprintln!("trace: configured at {:?}", start.elapsed());
        }
        self.draw();
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
        // Focus went elsewhere — a click on another window, a workspace
        // switch. A menu left open behind that is a menu in the way.
        if self.layer.wl_surface() == surface {
            self.exit = true;
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
            let point = self.physical(event.position);
            let row = self.layout.row_at(point);
            let Some(menu) = self.menu.as_mut() else {
                return;
            };
            let outcome = match event.kind {
                // Motion only, not Enter: a menu that opens under a resting
                // pointer must not have its selection moved by it.
                PointerEventKind::Motion { .. } => {
                    row.map_or(Outcome::Unchanged, |r| menu.hover(r))
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    row.map_or(Outcome::Unchanged, |r| menu.click(r))
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let rows = if vertical.discrete != 0 {
                        vertical.discrete
                    } else {
                        // Touchpads report distance: a row's worth per row.
                        self.scroll_rest += vertical.absolute;
                        let row_h =
                            f64::from(self.layout.row.height) / f64::from(self.layout.scale);
                        #[allow(clippy::cast_possible_truncation)]
                        let whole = (self.scroll_rest / row_h).trunc() as i32;
                        self.scroll_rest -= f64::from(whole) * row_h;
                        whole
                    };
                    if rows == 0 {
                        Outcome::Unchanged
                    } else {
                        menu.scroll_by(rows)
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

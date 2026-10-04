//! What the compositor says, turned into [`Event`]s.
//!
//! Everything here is the receiving half: one `Dispatch` per kind of object,
//! each writing down what it heard and queueing what a host would want to
//! know. Nothing here decides anything.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region,
    wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::keyboard::{KeyEvent, Keyboard};
use crate::shm::Buffer;
use crate::{Event, Output, PointerEvent, PointerKind, Surface};

/// The newest `wl_output` understood: names and descriptions.
pub(crate) const OUTPUT: u32 = 4;

/// The newest `wl_seat` asked for. Version 10 has the compositor repeat held
/// keys itself; up to 9 the client does, and one way of doing it is enough.
pub(crate) const SEAT: u32 = 9;

/// A wheel's notch, in the protocol's hundred-and-twentieths.
const NOTCH: i32 = 120;

/// A surface, and what the compositor has said about it.
pub(crate) struct Known {
    pub(crate) surface: wl_surface::WlSurface,
    /// The scale the compositor wants it drawn at.
    pub(crate) scale: i32,
    /// The scale its last buffer was said to be at.
    pub(crate) buffer_scale: i32,
    /// What enlarges its buffer to its size, once it has been asked to.
    pub(crate) viewport: Option<wp_viewport::WpViewport>,
    /// The outputs it is on, for a compositor too old to say the scale.
    on: Vec<wl_output::WlOutput>,
    /// A toplevel's size, said before the configure it belongs to.
    pending: (u32, u32),
}

impl Known {
    pub(crate) fn new(surface: wl_surface::WlSurface) -> Self {
        Self {
            surface,
            scale: 1,
            buffer_scale: 1,
            viewport: None,
            on: Vec::new(),
            pending: (0, 0),
        }
    }
}

/// An output, and what it has said about itself.
pub(crate) struct Monitor {
    /// Its number in the registry, which is how its going is announced.
    global: u32,
    pub(crate) wl: wl_output::WlOutput,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) make: String,
    pub(crate) model: String,
    scale: i32,
    /// Whether a host has been told of it: once it has said everything.
    announced: bool,
}

/// A key held, and when it next counts as pressed again.
pub(crate) struct Repeat {
    raw: u32,
    pub(crate) event: KeyEvent,
    pub(crate) due: Instant,
    pub(crate) gap: Duration,
}

#[derive(Default)]
struct Pointing {
    over: Option<wl_surface::WlSurface>,
    at: (f64, f64),
    /// What has arrived since the last `frame`.
    pending: Vec<PointerEvent>,
    /// Whether this frame's scrolling came in a wheel's notches.
    notched: bool,
    /// Parts of a notch not yet a whole one.
    rest: i32,
}

pub(crate) struct State {
    pub(crate) events: VecDeque<Event>,
    pub(crate) compositor: wl_compositor::WlCompositor,
    pub(crate) shm: wl_shm::WlShm,
    pub(crate) layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub(crate) wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub(crate) decorations: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    pub(crate) activation: Option<xdg_activation_v1::XdgActivationV1>,
    pub(crate) cursor_shapes: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub(crate) viewporter: Option<wp_viewporter::WpViewporter>,
    pub(crate) lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    /// Whether the compositor has granted a session lock that is still up.
    pub(crate) locked: bool,
    pub(crate) surfaces: Vec<Known>,
    pub(crate) buffers: Vec<Buffer>,
    pub(crate) outputs: Vec<Monitor>,
    seats: Vec<(u32, wl_seat::WlSeat)>,
    /// Seats there were on connecting, not yet asked for. See
    /// [`State::seat_late`].
    waiting: Vec<(u32, u32)>,
    keyboard: Option<(wl_seat::WlSeat, wl_keyboard::WlKeyboard)>,
    /// Made when there is a keyboard to read: libxkbcommon takes a few
    /// milliseconds to start, and a first frame should not wait for them.
    keys: Option<Keyboard>,
    pub(crate) focus: Option<wl_surface::WlSurface>,
    pub(crate) repeat: Option<Repeat>,
    /// How long a key is held before it repeats, and how long between.
    repeating: Option<(Duration, Duration)>,
    pointer: Option<(wl_seat::WlSeat, wl_pointer::WlPointer)>,
    pub(crate) cursor: Option<wp_cursor_shape_device_v1::WpCursorShapeDeviceV1>,
    pointing: Pointing,
}

impl State {
    pub(crate) fn new(compositor: wl_compositor::WlCompositor, shm: wl_shm::WlShm) -> Self {
        Self {
            events: VecDeque::new(),
            compositor,
            shm,
            layer_shell: None,
            wm_base: None,
            decorations: None,
            activation: None,
            cursor_shapes: None,
            viewporter: None,
            lock_manager: None,
            locked: false,
            surfaces: Vec::new(),
            buffers: Vec::new(),
            outputs: Vec::new(),
            seats: Vec::new(),
            waiting: Vec::new(),
            keyboard: None,
            keys: None,
            focus: None,
            repeat: None,
            repeating: None,
            pointer: None,
            cursor: None,
            pointing: Pointing::default(),
        }
    }

    /// Bind a global the registry announced, if it is one of the two kinds
    /// there can be several of and that come and go.
    pub(crate) fn announced(
        &mut self,
        registry: &wl_registry::WlRegistry,
        qh: &QueueHandle<Self>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        if interface == wl_output::WlOutput::interface().name {
            let wl: wl_output::WlOutput = registry.bind(name, version.min(OUTPUT), qh, ());
            // Before version 2 an output never says it has finished.
            let announced = wl.version() < 2;
            if announced {
                self.events.push_back(Event::Output(Output(wl.clone())));
            }
            self.outputs.push(Monitor {
                global: name,
                wl,
                name: None,
                description: None,
                make: String::new(),
                model: String::new(),
                scale: 1,
                announced,
            });
        } else if interface == wl_seat::WlSeat::interface().name {
            let seat = registry.bind(name, version.min(SEAT), qh, ());
            self.seats.push((name, seat));
        }
    }

    /// Note a seat there was on connecting, to be asked for once the first
    /// surface has been.
    ///
    /// What is asked for first is answered first. A seat asked for before the
    /// surface has its keyboard set up — libxkbcommon started, a layout
    /// compiled — before the surface's configure is read, and the first frame
    /// waits behind something it does not need.
    pub(crate) fn seat_late(&mut self, name: u32, version: u32) {
        self.waiting.push((name, version));
    }

    /// Ask for the seats that were left waiting.
    pub(crate) fn seats_now(&mut self, registry: &wl_registry::WlRegistry, qh: &QueueHandle<Self>) {
        for (name, version) in std::mem::take(&mut self.waiting) {
            let seat = registry.bind(name, version.min(SEAT), qh, ());
            self.seats.push((name, seat));
        }
    }

    /// Have the compositor enlarge what is on `surface` from here on.
    pub(crate) fn enlarge(
        &mut self,
        surface: &wl_surface::WlSurface,
        qh: &QueueHandle<Self>,
        enlarged: crate::Enlarged,
    ) {
        let (Some(viewporter), Some(known)) = (
            &self.viewporter,
            self.surfaces.iter_mut().find(|k| &k.surface == surface),
        ) else {
            return;
        };
        let viewport = known
            .viewport
            .get_or_insert_with(|| viewporter.get_viewport(surface, qh, ()));
        viewport.set_source(0.0, 0.0, enlarged.of.0, enlarged.of.1);
        viewport.set_destination(
            i32::try_from(enlarged.to.0).unwrap_or(1).max(1),
            i32::try_from(enlarged.to.1).unwrap_or(1).max(1),
        );
    }

    fn known(&mut self, surface: &wl_surface::WlSurface) -> Option<&mut Known> {
        self.surfaces.iter_mut().find(|k| &k.surface == surface)
    }

    /// Work out a surface's scale from the outputs it is on: what a
    /// compositor older than `wl_compositor` 6 leaves to the client.
    fn rescale(&mut self, surface: &wl_surface::WlSurface) {
        if surface.version() >= 6 {
            return;
        }
        let outputs = &self.outputs;
        let Some(known) = self.surfaces.iter_mut().find(|k| &k.surface == surface) else {
            return;
        };
        let Some(scale) = known
            .on
            .iter()
            .filter_map(|on| outputs.iter().find(|m| &m.wl == on))
            .map(|monitor| monitor.scale)
            .max()
        else {
            return;
        };
        if scale != known.scale {
            known.scale = scale;
            self.events.push_back(Event::Scale {
                surface: Surface(surface.clone()),
                scale: u32::try_from(scale).unwrap_or(1).max(1),
            });
        }
    }

    fn configured(&mut self, surface: &wl_surface::WlSurface, size: (u32, u32)) {
        self.events.push_back(Event::Configure {
            surface: Surface(surface.clone()),
            size,
        });
    }

    /// Queue something the pointer did, or hold it until the frame it is
    /// part of is complete.
    fn pointed(
        &mut self,
        pointer: &wl_pointer::WlPointer,
        left: Option<wl_surface::WlSurface>,
        kind: PointerKind,
    ) {
        let Some(surface) = left.or_else(|| self.pointing.over.clone()) else {
            return;
        };
        let event = PointerEvent {
            surface: Surface(surface),
            position: self.pointing.at,
            kind,
        };
        if pointer.version() < 5 {
            // No frames: each event stands alone.
            self.events.push_back(Event::Pointer(event));
            return;
        }
        // Scrolling arrives in pieces — the distance, then the notches — that
        // are one thing to a host.
        if let (
            Some(PointerEvent {
                kind: PointerKind::Scroll { distance, steps },
                ..
            }),
            PointerKind::Scroll {
                distance: more,
                steps: further,
            },
        ) = (self.pointing.pending.last_mut(), &event.kind)
        {
            *distance += more;
            *steps += further;
            return;
        }
        self.pointing.pending.push(event);
    }

    fn pointer_frame(&mut self) {
        let notched = std::mem::take(&mut self.pointing.notched);
        for mut event in self.pointing.pending.drain(..) {
            if let PointerKind::Scroll { distance, .. } = &mut event.kind
                && notched
            {
                // A wheel says both how far and how many notches. The
                // notches are the truth; the distance is a guess at pixels.
                *distance = 0.0;
            }
            self.events.push_back(Event::Pointer(event));
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => state.announced(registry, qh, name, &interface, version),
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = state.outputs.iter().position(|m| m.global == name) {
                    let gone = state.outputs.remove(index);
                    for known in &mut state.surfaces {
                        known.on.retain(|on| on != &gone.wl);
                    }
                    state
                        .events
                        .push_back(Event::OutputGone(Output(gone.wl.clone())));
                    if gone.wl.version() >= 3 {
                        gone.wl.release();
                    }
                }
                state.seats.retain(|(seat, _)| *seat != name);
                state.waiting.retain(|(seat, _)| *seat != name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(monitor) = state.outputs.iter_mut().find(|m| &m.wl == output) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { make, model, .. } => {
                monitor.make = make;
                monitor.model = model;
            }
            wl_output::Event::Scale { factor } => monitor.scale = factor,
            wl_output::Event::Name { name } => monitor.name = Some(name),
            wl_output::Event::Description { description } => {
                monitor.description = Some(description);
            }
            wl_output::Event::Done => {
                if !std::mem::replace(&mut monitor.announced, true) {
                    state
                        .events
                        .push_back(Event::Output(Output(output.clone())));
                }
                // Its scale may be what changed.
                let surfaces: Vec<_> = state.surfaces.iter().map(|k| k.surface.clone()).collect();
                for surface in &surfaces {
                    state.rescale(surface);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        state: &mut Self,
        surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_surface::Event::Enter { output } => {
                if let Some(known) = state.known(surface) {
                    known.on.push(output.clone());
                }
                state.rescale(surface);
                state.events.push_back(Event::Entered {
                    surface: Surface(surface.clone()),
                    output: Output(output),
                });
            }
            wl_surface::Event::Leave { output } => {
                if let Some(known) = state.known(surface) {
                    known.on.retain(|on| on != &output);
                }
                state.rescale(surface);
            }
            wl_surface::Event::PreferredBufferScale { factor } => {
                let Some(known) = state.known(surface) else {
                    return;
                };
                if known.scale != factor {
                    known.scale = factor;
                    state.events.push_back(Event::Scale {
                        surface: Surface(surface.clone()),
                        scale: u32::try_from(factor).unwrap_or(1).max(1),
                    });
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state
                .events
                .push_back(Event::Frame(Surface(surface.clone())));
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event
            && let Some(ours) = state.buffers.iter_mut().find(|b| &b.wl == buffer)
        {
            ours.busy = false;
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                state.configured(surface, (width, height));
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state
                    .events
                    .push_back(Event::Closed(Surface(surface.clone())));
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // "Are you still there": a window that does not answer is offered to
        // be killed.
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        xdg: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg.ack_configure(serial);
            let size = state.known(surface).map_or((0, 0), |known| known.pending);
            state.configured(surface, size);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if let Some(known) = state.known(surface) {
                    known.pending = (
                        u32::try_from(width).unwrap_or(0),
                        u32::try_from(height).unwrap_or(0),
                    );
                }
            }
            xdg_toplevel::Event::Close => {
                state
                    .events
                    .push_back(Event::Closed(Surface(surface.clone())));
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, wl_surface::WlSurface> for State {
    fn event(
        state: &mut Self,
        token: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token: granted } = event {
            if let Some(activation) = &state.activation {
                activation.activate(granted, surface);
            }
            token.destroy();
        }
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => {
                state.locked = true;
                state.events.push_back(Event::Locked);
            }
            ext_session_lock_v1::Event::Finished => {
                state.locked = false;
                state.events.push_back(Event::Refused);
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, wl_surface::WlSurface>
    for State
{
    fn event(
        state: &mut Self,
        lock_surface: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            lock_surface.ack_configure(serial);
            state.configured(surface, (width, height));
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(has),
        } = event
        else {
            return;
        };
        // One keyboard and one pointer: the first seat's that has them.
        if has.contains(wl_seat::Capability::Keyboard) {
            if state.keyboard.is_none() {
                let keyboard = seat.get_keyboard(qh, ());
                if keyboard.version() < 4 {
                    // Too old to say how keys repeat: what X always did.
                    state.repeating = Some((Duration::from_millis(600), Duration::from_millis(40)));
                }
                state.keys.get_or_insert_with(Keyboard::new);
                state.keyboard = Some((seat.clone(), keyboard));
            }
        } else if state.keyboard.as_ref().is_some_and(|(of, _)| of == seat)
            && let Some((_, keyboard)) = state.keyboard.take()
        {
            state.repeat = None;
            if keyboard.version() >= 3 {
                keyboard.release();
            }
        }
        if has.contains(wl_seat::Capability::Pointer) {
            if state.pointer.is_none() {
                let pointer = seat.get_pointer(qh, ());
                state.cursor = state
                    .cursor_shapes
                    .as_ref()
                    .map(|shapes| shapes.get_pointer(&pointer, qh, ()));
                state.pointer = Some((seat.clone(), pointer));
            }
        } else if state.pointer.as_ref().is_some_and(|(of, _)| of == seat)
            && let Some((_, pointer)) = state.pointer.take()
        {
            if let Some(cursor) = state.cursor.take() {
                cursor.destroy();
            }
            state.pointing = Pointing::default();
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
                fd,
                size,
            } => {
                if let Some(keys) = &mut state.keys {
                    keys.keymap(fd, size);
                }
            }
            wl_keyboard::Event::Enter { surface, .. } => state.focus = Some(surface),
            wl_keyboard::Event::Leave { surface, .. } => {
                state.repeat = None;
                state.focus = None;
                state.events.push_back(Event::Unfocused(Surface(surface)));
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Pressed),
                ..
            } => {
                let Some(keys) = &mut state.keys else {
                    return;
                };
                let Some(event) = keys.pressed(key) else {
                    return;
                };
                state.events.push_back(Event::Key(event.clone()));
                if keys.repeats(key) {
                    // A second key held takes over from the first.
                    state.repeat = state.repeating.map(|(delay, gap)| Repeat {
                        raw: key,
                        event,
                        due: Instant::now() + delay,
                        gap,
                    });
                }
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Released),
                ..
            } => {
                if state.repeat.as_ref().is_some_and(|held| held.raw == key) {
                    state.repeat = None;
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                let Some(keys) = &mut state.keys else {
                    return;
                };
                let Some(modifiers) =
                    keys.modifiers(mods_depressed, mods_latched, mods_locked, group)
                else {
                    return;
                };
                // Shift pressed under a held key changes what it types.
                if let Some(held) = &mut state.repeat {
                    held.event.utf8 = keys.text(held.raw);
                }
                state.events.push_back(Event::Modifiers(modifiers));
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                state.repeating = match (u64::try_from(rate), u64::try_from(delay)) {
                    (Ok(rate), Ok(delay)) if rate > 0 => Some((
                        Duration::from_millis(delay),
                        Duration::from_micros(1_000_000 / rate),
                    )),
                    // A rate of nothing is the compositor saying not to.
                    _ => None,
                };
                if state.repeating.is_none() {
                    state.repeat = None;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let vertical = |axis| axis == WEnum::Value(wl_pointer::Axis::VerticalScroll);
        match event {
            wl_pointer::Event::Enter {
                serial,
                surface,
                surface_x,
                surface_y,
            } => {
                state.pointing.over = Some(surface);
                state.pointing.at = (surface_x, surface_y);
                state.pointed(pointer, None, PointerKind::Enter { serial });
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if state.pointing.over.as_ref() == Some(&surface) {
                    state.pointing.over = None;
                }
                state.pointed(pointer, Some(surface), PointerKind::Leave);
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointing.at = (surface_x, surface_y);
                state.pointed(pointer, None, PointerKind::Motion);
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(pressed),
                ..
            } => {
                let kind = match pressed {
                    wl_pointer::ButtonState::Pressed => PointerKind::Press { button },
                    wl_pointer::ButtonState::Released => PointerKind::Release { button },
                    _ => return,
                };
                state.pointed(pointer, None, kind);
            }
            wl_pointer::Event::Axis { axis, value, .. } if vertical(axis) => {
                state.pointed(
                    pointer,
                    None,
                    PointerKind::Scroll {
                        distance: value,
                        steps: 0,
                    },
                );
            }
            wl_pointer::Event::AxisDiscrete { axis, discrete } if vertical(axis) => {
                state.pointing.notched = true;
                state.pointed(
                    pointer,
                    None,
                    PointerKind::Scroll {
                        distance: 0.0,
                        steps: discrete,
                    },
                );
            }
            wl_pointer::Event::AxisValue120 { axis, value120 } if vertical(axis) => {
                // A wheel finer than a notch a turn: whole notches count, and
                // the rest waits for more.
                state.pointing.notched = true;
                let total = state.pointing.rest + value120;
                state.pointing.rest = total % NOTCH;
                state.pointed(
                    pointer,
                    None,
                    PointerKind::Scroll {
                        distance: 0.0,
                        steps: total / NOTCH,
                    },
                );
            }
            wl_pointer::Event::Frame => state.pointer_frame(),
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wp_viewporter::WpViewporter);
delegate_noop!(State: ignore wp_viewport::WpViewport);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_region::WlRegion);
delegate_noop!(State: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(State: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(State: ignore zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1);
delegate_noop!(State: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(State: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
delegate_noop!(State: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
delegate_noop!(State: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);

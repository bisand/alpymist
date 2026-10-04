//! What Alpymist's popups, windows and lock screen stand on.
//!
//! A Wayland client needs the same few things whatever it shows: a surface
//! with a role, pictures put on it, a keyboard that knows its layout, a
//! pointer, and a loop that sleeps until one of them has something to say.
//! This is those, over `wayland-client` — the protocol itself, which is not
//! worth writing again — and libxkbcommon for the keyboard, and nothing else:
//! no toolkit, no event-loop library, no cursor themes. ADR 0026 says why.
//!
//! It is shaped as a queue to read rather than handlers to fill in. A host
//! owns a [`Wayland`], asks it for surfaces, and calls [`Wayland::wait`] in a
//! loop; what comes back is a list of [`Event`]s, each naming the [`Surface`]
//! it is about. Timers ([`Wayland::after`]) and anything else worth waking
//! for ([`Wayland::watch`], which is how a [`channel`] gets in) arrive in the
//! same list, so a host has one place where things happen.
//!
//! Pictures are handed over whole, by [`Wayland::show`], from memory the host
//! owns; `shm.rs` says why they are written rather than drawn in place.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

mod channel;
mod keyboard;
mod shm;
mod state;

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::backend::WaylandError;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_registry, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, EventQueue, Proxy, QueueHandle};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_surface_v1, ext_session_lock_v1,
};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1;
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

pub use channel::{Receiver, Sender, channel};
pub use keyboard::{KeyEvent, Keysym, Modifiers};

use shm::Buffer;
use state::{Known, State};

/// `BTN_LEFT` from linux/input-event-codes.h.
pub const BTN_LEFT: u32 = 0x110;

/// A surface: what an [`Event`] is about, and what a picture is put on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface(wl_surface::WlSurface);

/// One of the compositor's outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output(wl_output::WlOutput);

/// What an output says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    /// What the compositor calls it: `eDP-1`, `DP-3`. Compositors older than
    /// `wl_output` 4 do not say.
    pub name: Option<String>,
    /// The compositor's own description of it.
    pub description: Option<String>,
    /// Who made it.
    pub make: String,
    /// And what they called it.
    pub model: String,
}

/// A timer set with [`Wayland::after`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer(u64);

/// Something watched with [`Wayland::watch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(u64);

/// Something that happened.
#[derive(Debug, Clone)]
pub enum Event {
    /// The compositor has decided a surface's size, in logical pixels, and it
    /// may be drawn — and is owed a picture. A window may be told `0` for
    /// either side: the choice is the window's.
    Configure {
        /// Which surface.
        surface: Surface,
        /// How large.
        size: (u32, u32),
    },
    /// A layer surface was taken away, or a window asked to close.
    Closed(Surface),
    /// The scale a surface should be drawn at changed.
    Scale {
        /// Which surface.
        surface: Surface,
        /// Physical pixels to a logical one.
        scale: u32,
    },
    /// The compositor is ready for a surface's next picture: the answer to a
    /// [`Picture`] shown `paced`.
    Frame(Surface),
    /// A surface is now on an output.
    Entered {
        /// Which surface.
        surface: Surface,
        /// Which output.
        output: Output,
    },
    /// An output there was at the start, or one plugged in since.
    Output(Output),
    /// An output went away.
    OutputGone(Output),
    /// The session lock was granted: every output is covered.
    Locked,
    /// The session lock was refused or taken back. It can no longer be used.
    Refused,
    /// A key was pressed, or held long enough to count again.
    Key(KeyEvent),
    /// The modifiers held or locked changed.
    Modifiers(Modifiers),
    /// The keyboard left a surface.
    Unfocused(Surface),
    /// The pointer did something.
    Pointer(PointerEvent),
    /// A timer ran out.
    Timer(Timer),
    /// Something watched can be read.
    Ready(Source),
}

/// Something the pointer did.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerEvent {
    /// The surface it was over.
    pub surface: Surface,
    /// Where on it, in logical pixels.
    pub position: (f64, f64),
    /// What.
    pub kind: PointerKind,
}

/// What a pointer did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerKind {
    /// Came onto the surface. The serial is what setting its shape asks for.
    Enter {
        /// The compositor's number for this event.
        serial: u32,
    },
    /// Left it.
    Leave,
    /// Moved.
    Motion,
    /// A button went down: [`BTN_LEFT`] and its neighbours.
    Press {
        /// Which.
        button: u32,
    },
    /// A button came up.
    Release {
        /// Which.
        button: u32,
    },
    /// Scrolled, downwards positive. A wheel says `steps`, in notches, and a
    /// touchpad `distance`, in logical pixels; never both.
    Scroll {
        /// How far, for something that scrolls smoothly.
        distance: f64,
        /// How many notches, for a wheel.
        steps: i32,
    },
}

/// What the pointer looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The arrow.
    Default,
    /// A hand, over something that takes a click.
    Pointer,
    /// An I-beam, over text that can be typed into.
    Text,
}

/// Which layer of the layer shell a surface goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stratum {
    /// Under every window: the wallpaper's.
    Background,
    /// Over everything, full-screen windows included.
    Overlay,
}

/// How a layer surface is asked for.
#[derive(Debug, Clone)]
pub struct LayerOptions<'a> {
    /// The namespace compositor rules match on.
    pub namespace: &'a str,
    /// Which layer.
    pub stratum: Stratum,
    /// The output to be on; `None` leaves it to the compositor, which picks
    /// the one with focus.
    pub output: Option<&'a Output>,
    /// Anchored to every edge, and so sized by the compositor to the whole
    /// output; otherwise to none, and centred at [`LayerOptions::size`].
    pub fill: bool,
    /// Whether it takes the keyboard, exclusively, while it is up.
    pub keyboard: bool,
    /// `-1` to lie over panels as well, `0` to keep clear of them.
    pub exclusive_zone: i32,
    /// The size to ask for, in logical pixels. `(0, 0)` with `fill`.
    pub size: (u32, u32),
}

/// How a window is asked for.
#[derive(Debug, Clone)]
pub struct WindowOptions<'a> {
    /// The title.
    pub title: &'a str,
    /// The app id compositors match window rules on.
    pub app_id: &'a str,
    /// The smallest size, in logical pixels.
    pub min_size: (u32, u32),
    /// The largest, when there is one.
    pub max_size: Option<(u32, u32)>,
}

/// A rectangle of a picture, in its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
}

/// A picture for a surface.
#[derive(Debug, Clone, Copy)]
pub struct Picture<'a> {
    /// Width and height, in physical pixels.
    pub size: (u32, u32),
    /// Physical pixels to a logical one.
    pub scale: u32,
    /// Every pixel, row after row: `0xAARRGGBB`, premultiplied.
    pub pixels: &'a [u32],
    /// Whether none of it shows anything through, which saves the compositor
    /// blending a surface's worth over whatever is behind.
    pub opaque: bool,
    /// The parts that differ from the picture shown on this surface before.
    /// `None` is all of it, and an empty list none of it.
    ///
    /// It is believed: only these are written to the compositor, and
    /// whatever changed outside them stays as it was on screen.
    pub damage: Option<&'a [Area]>,
    /// Whether to be told, with [`Event::Frame`], when the next is wanted.
    pub paced: bool,
}

/// A layer surface. Dropping it takes it off the screen.
#[derive(Debug)]
pub struct Layer {
    surface: Surface,
    role: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
}

impl Layer {
    /// The surface it is.
    #[must_use]
    pub fn surface(&self) -> &Surface {
        &self.surface
    }
}

impl Drop for Layer {
    fn drop(&mut self) {
        // The role before the surface it was given to.
        self.role.destroy();
        self.surface.0.destroy();
    }
}

/// A window. Dropping it closes it.
#[derive(Debug)]
pub struct Window {
    surface: Surface,
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    decoration: Option<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1>,
}

impl Window {
    /// The surface it is.
    #[must_use]
    pub fn surface(&self) -> &Surface {
        &self.surface
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // In the order the protocols ask for: decoration, role, xdg surface,
        // surface.
        if let Some(decoration) = &self.decoration {
            decoration.destroy();
        }
        self.toplevel.destroy();
        self.xdg.destroy();
        self.surface.0.destroy();
    }
}

/// A session lock: asked for, and the compositor's to grant
/// ([`Event::Locked`]) or not ([`Event::Refused`]).
///
/// Dropping it does not unlock. A lock that was granted and is dropped
/// without [`Wayland::unlock`] leaves the session locked, which is the right
/// way for a lock screen to fail.
#[derive(Debug)]
pub struct SessionLock {
    lock: ext_session_lock_v1::ExtSessionLockV1,
    unlocked: bool,
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        if !self.unlocked {
            self.lock.destroy();
        }
    }
}

/// A session lock's surface on one output.
#[derive(Debug)]
pub struct LockSurface {
    surface: Surface,
    role: ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
}

impl LockSurface {
    /// The surface it is.
    #[must_use]
    pub fn surface(&self) -> &Surface {
        &self.surface
    }
}

impl Drop for LockSurface {
    fn drop(&mut self) {
        self.role.destroy();
        self.surface.0.destroy();
    }
}

/// A connection to the compositor, and everything that happens on it.
pub struct Wayland {
    conn: Connection,
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    registry: wl_registry::WlRegistry,
    state: State,
    timers: Vec<(Instant, u64)>,
    sources: Vec<(u64, OwnedFd)>,
    next: u64,
}

impl Wayland {
    /// Connect to the session's compositor.
    ///
    /// One round trip, to learn what the compositor has; nothing is waited
    /// for after that, so a surface asked for next is on its way before
    /// anything else is read. The keyboard and pointer are asked for when the
    /// loop first runs, behind that surface.
    ///
    /// # Errors
    /// No Wayland session, or a compositor without the two things every
    /// surface needs.
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
        let (globals, queue) =
            registry_queue_init::<State>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
        let qh = queue.handle();
        let compositor = globals
            .bind(&qh, 1..=6, ())
            .map_err(|_| "the compositor has no wl_compositor")?;
        let shm = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| "the compositor has no wl_shm")?;
        let mut state = State::new(compositor, shm);
        state.layer_shell = globals.bind(&qh, 1..=4, ()).ok();
        state.wm_base = globals.bind(&qh, 1..=6, ()).ok();
        state.decorations = globals.bind(&qh, 1..=1, ()).ok();
        state.activation = globals.bind(&qh, 1..=1, ()).ok();
        state.cursor_shapes = globals.bind(&qh, 1..=1, ()).ok();
        state.lock_manager = globals.bind(&qh, 1..=1, ()).ok();
        for global in globals.contents().clone_list() {
            if global.interface == wl_seat::WlSeat::interface().name {
                state.seat_late(global.name, global.version);
            } else {
                state.announced(
                    globals.registry(),
                    &qh,
                    global.name,
                    &global.interface,
                    global.version,
                );
            }
        }
        Ok(Self {
            conn,
            queue,
            qh,
            registry: globals.registry().clone(),
            state,
            timers: Vec::new(),
            sources: Vec::new(),
            next: 0,
        })
    }

    fn new_surface(&mut self) -> Surface {
        let surface = self.state.compositor.create_surface(&self.qh, ());
        self.state.surfaces.push(Known::new(surface.clone()));
        Surface(surface)
    }

    /// Ask for a layer surface. It may be drawn once its
    /// [`Event::Configure`] arrives.
    ///
    /// # Errors
    /// A compositor without the layer shell: Hyprland and Sway have it, GNOME
    /// does not.
    pub fn layer(&mut self, options: &LayerOptions<'_>) -> Result<Layer, String> {
        use zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity};
        let Some(shell) = self.state.layer_shell.clone() else {
            return Err("this compositor does not support wlr-layer-shell".into());
        };
        let surface = self.new_surface();
        let role = shell.get_layer_surface(
            &surface.0,
            options.output.map(|output| &output.0),
            match options.stratum {
                Stratum::Background => zwlr_layer_shell_v1::Layer::Background,
                Stratum::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
            },
            options.namespace.to_owned(),
            &self.qh,
            surface.0.clone(),
        );
        role.set_anchor(if options.fill {
            Anchor::all()
        } else {
            Anchor::empty()
        });
        role.set_keyboard_interactivity(if options.keyboard {
            KeyboardInteractivity::Exclusive
        } else {
            KeyboardInteractivity::None
        });
        role.set_exclusive_zone(options.exclusive_zone);
        role.set_size(options.size.0, options.size.1);
        surface.0.commit();
        Ok(Layer { surface, role })
    }

    /// Ask for a window, decorated however the compositor decorates.
    ///
    /// # Errors
    /// A compositor without the xdg shell.
    pub fn window(&mut self, options: &WindowOptions<'_>) -> Result<Window, String> {
        let Some(wm_base) = self.state.wm_base.clone() else {
            return Err("the compositor has no xdg shell".into());
        };
        let surface = self.new_surface();
        let xdg = wm_base.get_xdg_surface(&surface.0, &self.qh, surface.0.clone());
        let toplevel = xdg.get_toplevel(&self.qh, surface.0.clone());
        // With no mode asked for: the compositor's own choice.
        let decoration = self
            .state
            .decorations
            .as_ref()
            .map(|manager| manager.get_toplevel_decoration(&toplevel, &self.qh, ()));
        let side = |v: u32| i32::try_from(v).unwrap_or(0);
        toplevel.set_title(options.title.to_owned());
        toplevel.set_app_id(options.app_id.to_owned());
        toplevel.set_min_size(side(options.min_size.0), side(options.min_size.1));
        let (width, height) = options.max_size.unwrap_or((0, 0));
        toplevel.set_max_size(side(width), side(height));
        surface.0.commit();
        Ok(Window {
            surface,
            xdg,
            toplevel,
            decoration,
        })
    }

    /// Ask the compositor to bring a window forward.
    pub fn raise(&mut self, window: &Window, app_id: &str) {
        let Some(activation) = &self.state.activation else {
            return;
        };
        // A token is asked for, and the window activated with it when it
        // comes: the compositor decides whether that is allowed.
        let token = activation.get_activation_token(&self.qh, window.surface.0.clone());
        token.set_app_id(app_id.to_owned());
        token.set_surface(&window.surface.0);
        token.commit();
    }

    /// Ask for the session to be locked.
    ///
    /// # Errors
    /// A compositor without `ext-session-lock-v1`, which has locked nothing.
    pub fn lock(&mut self) -> Result<SessionLock, String> {
        let Some(manager) = &self.state.lock_manager else {
            return Err(
                "this compositor cannot lock a session: it has no ext-session-lock-v1".into(),
            );
        };
        Ok(SessionLock {
            lock: manager.lock(&self.qh, ()),
            unlocked: false,
        })
    }

    /// A lock's surface for `output`. One, and only one, is owed to every
    /// output.
    pub fn cover(&mut self, lock: &SessionLock, output: &Output) -> LockSurface {
        let surface = self.new_surface();
        let role = lock
            .lock
            .get_lock_surface(&surface.0, &output.0, &self.qh, surface.0.clone());
        LockSurface { surface, role }
    }

    /// Give the session back, if the lock was ever granted.
    ///
    /// Follow it with [`Wayland::roundtrip`]: a process that exits before the
    /// compositor has read this leaves a locked session with nothing to
    /// unlock it.
    pub fn unlock(&mut self, mut lock: SessionLock) {
        if self.state.locked {
            lock.lock.unlock_and_destroy();
            lock.unlocked = true;
            self.state.locked = false;
        }
    }

    /// Every output the compositor has.
    #[must_use]
    pub fn outputs(&self) -> Vec<Output> {
        self.state
            .outputs
            .iter()
            .map(|monitor| Output(monitor.wl.clone()))
            .collect()
    }

    /// What an output has said about itself so far.
    #[must_use]
    pub fn info(&self, output: &Output) -> Option<OutputInfo> {
        let monitor = self.state.outputs.iter().find(|m| m.wl == output.0)?;
        Some(OutputInfo {
            name: monitor.name.clone(),
            description: monitor.description.clone(),
            make: monitor.make.clone(),
            model: monitor.model.clone(),
        })
    }

    /// Put a picture on a surface.
    ///
    /// # Errors
    /// A picture of no size, one with fewer pixels than it says, or no memory
    /// to share it in.
    pub fn show(&mut self, surface: &Surface, picture: &Picture<'_>) -> Result<(), String> {
        let (Ok(width), Ok(height)) =
            (i32::try_from(picture.size.0), i32::try_from(picture.size.1))
        else {
            return Err("a picture too large".into());
        };
        let count = picture.size.0 as usize * picture.size.1 as usize;
        let Some(pixels) = picture.pixels.get(..count) else {
            return Err("a picture with fewer pixels than its size".into());
        };
        let format = if picture.opaque {
            wl_shm::Format::Xrgb8888
        } else {
            wl_shm::Format::Argb8888
        };
        let state = &mut self.state;
        let fits = |b: &Buffer| b.width == width && b.height == height && b.format == format;
        // What this surface has that is the wrong size now, and the
        // compositor is done with, goes.
        state
            .buffers
            .retain(|b| b.owner != surface.0 || b.busy || fits(b));
        let free = state
            .buffers
            .iter()
            .position(|b| b.owner == surface.0 && !b.busy && fits(b));
        let index = if let Some(index) = free {
            index
        } else {
            let buffer = Buffer::new(&state.shm, &self.qh, (width, height), format, &surface.0)
                .map_err(|e| format!("shared memory: {e}"))?;
            state.buffers.push(buffer);
            state.buffers.len() - 1
        };
        // Every buffer this surface has is now behind by what changed, the
        // one about to be written included.
        for buffer in state.buffers.iter_mut().filter(|b| b.owner == surface.0) {
            match picture.damage {
                Some(areas) => areas.iter().for_each(|area| buffer.changed(Some(area))),
                None => buffer.changed(None),
            }
        }
        let buffer = &mut state.buffers[index];
        buffer
            .fill(pixels)
            .map_err(|e| format!("shared memory: {e}"))?;

        let scale = i32::try_from(picture.scale).unwrap_or(1).max(1);
        if let Some(known) = state.surfaces.iter_mut().find(|k| k.surface == surface.0)
            && known.buffer_scale != scale
        {
            // Only when it changes: a surface told its scale again has been
            // told something about every pixel of itself.
            known.buffer_scale = scale;
            surface.0.set_buffer_scale(scale);
        }
        surface.0.attach(Some(&buffer.wl), 0, 0);
        // The compositor is told of one rectangle, around all of them.
        // Measured on the Atom (ADR 0026): Hyprland given a few dozen bands
        // a frame spends more on being told than it saves on what it need
        // not repaint.
        let around = picture.damage.map_or(
            Some(Area {
                x: 0,
                y: 0,
                width,
                height,
            }),
            |areas| {
                areas
                    .iter()
                    .filter(|area| area.width > 0 && area.height > 0)
                    .map(|area| {
                        (
                            area.x,
                            area.y,
                            area.x.saturating_add(area.width),
                            area.y.saturating_add(area.height),
                        )
                    })
                    .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
                    .map(|(left, top, right, bottom)| Area {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    })
            },
        );
        if let Some(area) = around {
            surface
                .0
                .damage_buffer(area.x, area.y, area.width, area.height);
        }
        if picture.paced {
            surface.0.frame(&self.qh, surface.0.clone());
        }
        surface.0.commit();
        buffer.busy = true;
        Ok(())
    }

    /// Say that a surface shows nothing through, over this much of itself in
    /// logical pixels.
    pub fn opaque(&self, surface: &Surface, width: u32, height: u32) {
        let region = self.state.compositor.create_region(&self.qh, ());
        region.add(
            0,
            0,
            i32::try_from(width).unwrap_or(0),
            i32::try_from(height).unwrap_or(0),
        );
        surface.0.set_opaque_region(Some(&region));
        region.destroy();
    }

    /// What the pointer wears, from here on. `serial` is the
    /// [`PointerKind::Enter`] it came onto the surface with.
    ///
    /// Where the compositor cannot be asked for a shape by name, nothing
    /// happens, and the pointer keeps what it wore.
    pub fn cursor(&self, serial: u32, shape: Shape) {
        use wp_cursor_shape_device_v1::Shape as Wire;
        if let Some(device) = &self.state.cursor {
            device.set_shape(
                serial,
                match shape {
                    Shape::Default => Wire::Default,
                    Shape::Pointer => Wire::Pointer,
                    Shape::Text => Wire::Text,
                },
            );
        }
    }

    /// Be told, once, when `delay` has passed.
    pub fn after(&mut self, delay: Duration) -> Timer {
        self.next += 1;
        self.timers.push((Instant::now() + delay, self.next));
        Timer(self.next)
    }

    /// Forget a timer that has not run out.
    pub fn cancel(&mut self, timer: Timer) {
        self.timers.retain(|(_, id)| *id != timer.0);
    }

    /// Be told, with [`Event::Ready`], whenever `fd` can be read — and again
    /// each time round for as long as it still can, so read it.
    ///
    /// # Errors
    /// When the descriptor cannot be duplicated.
    pub fn watch(&mut self, fd: BorrowedFd<'_>) -> io::Result<Source> {
        self.next += 1;
        self.sources.push((self.next, fd.try_clone_to_owned()?));
        Ok(Source(self.next))
    }

    /// Stop watching.
    pub fn unwatch(&mut self, source: Source) {
        self.sources.retain(|(id, _)| *id != source.0);
    }

    /// Send what has been asked for so far, without waiting for anything.
    pub fn flush(&self) {
        let _ = self.conn.flush();
    }

    /// Send what has been asked for and wait until the compositor has read
    /// it. What it said meanwhile is in the next [`Wayland::wait`].
    ///
    /// # Errors
    /// When the connection to the compositor fails.
    pub fn roundtrip(&mut self) -> Result<(), String> {
        self.state.seats_now(&self.registry, &self.qh);
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|e| format!("Wayland: {e}"))
    }

    /// Sleep until something happens, and say what.
    ///
    /// # Errors
    /// When the connection to the compositor fails: it has gone, or it has
    /// thrown this client out.
    pub fn wait(&mut self) -> Result<Vec<Event>, String> {
        self.state.seats_now(&self.registry, &self.qh);
        loop {
            self.tidy();
            self.due();
            if !self.state.events.is_empty() {
                return Ok(self.state.events.drain(..).collect());
            }
            match self.conn.flush() {
                Ok(()) => {}
                Err(WaylandError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("Wayland: {e}")),
            }
            let Some(guard) = self.queue.prepare_read() else {
                // Something was read already and not yet handed out.
                self.dispatch()?;
                continue;
            };
            let timeout = self.deadline().map(|at| {
                let left = at.saturating_duration_since(Instant::now());
                Timespec {
                    tv_sec: i64::try_from(left.as_secs()).unwrap_or(i64::MAX),
                    tv_nsec: left.subsec_nanos().into(),
                }
            });
            let ready: Vec<bool> = {
                let socket = guard.connection_fd();
                let mut fds = vec![PollFd::new(&socket, PollFlags::IN)];
                fds.extend(
                    self.sources
                        .iter()
                        .map(|(_, fd)| PollFd::from_borrowed_fd(fd.as_fd(), PollFlags::IN)),
                );
                match poll(&mut fds, timeout.as_ref()) {
                    Ok(_) => fds.iter().map(|fd| !fd.revents().is_empty()).collect(),
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(e) => return Err(format!("Wayland: {e}")),
                }
            };
            if ready[0] {
                match guard.read() {
                    Ok(_) => {}
                    Err(WaylandError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(format!("Wayland: {e}")),
                }
            } else {
                drop(guard);
            }
            self.dispatch()?;
            for ((id, _), _) in self
                .sources
                .iter()
                .zip(&ready[1..])
                .filter(|(_, ready)| **ready)
            {
                self.state.events.push_back(Event::Ready(Source(*id)));
            }
        }
    }

    fn dispatch(&mut self) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|e| format!("Wayland: {e}"))
    }

    /// When the loop next has to wake whether or not anything is said.
    fn deadline(&self) -> Option<Instant> {
        let held = self.state.repeat.as_ref().map(|held| held.due);
        self.timers.iter().map(|(at, _)| *at).chain(held).min()
    }

    /// Queue the timers that have run out, and a held key's next turn.
    fn due(&mut self) {
        let now = Instant::now();
        let mut ran: Vec<(Instant, u64)> = Vec::new();
        self.timers.retain(|timer| {
            let out = timer.0 <= now;
            if out {
                ran.push(*timer);
            }
            !out
        });
        ran.sort_unstable();
        for (_, id) in ran {
            self.state.events.push_back(Event::Timer(Timer(id)));
        }
        // A surface destroyed with the keyboard on it is never told the
        // keyboard left.
        if !self.state.focus.as_ref().is_some_and(Proxy::is_alive) {
            self.state.repeat = None;
        }
        if let Some(held) = &mut self.state.repeat
            && held.due <= now
        {
            // One at a time, however late: a loop that was busy does not owe
            // the keys it missed.
            held.due = now + held.gap;
            self.state.events.push_back(Event::Key(held.event.clone()));
        }
    }

    /// Forget surfaces that have been dropped, and their buffers.
    fn tidy(&mut self) {
        self.state.surfaces.retain(|k| k.surface.is_alive());
        self.state.buffers.retain(|b| b.owner.is_alive());
    }
}

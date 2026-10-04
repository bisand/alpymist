//! The menu on screen: a wlr layer surface, drawn in shared memory.
//!
//! Why this rather than a window through winit: a launcher is judged in the
//! first hundred milliseconds, and most of what a toolkit does at start-up —
//! a GPU context, a window with decorations, a negotiation about where it
//! goes — is time the menu does not need. A layer surface is placed by the
//! compositor, over everything, with the keyboard; `wl_shm` is a file both
//! sides hold, so a frame is Denise painting, one write and one commit.
//! Nothing is uploaded that the compositor would not have read anyway.
//!
//! The surface is asked for as early as possible — before the fonts load or
//! a single desktop entry is read — so the compositor's configure is already
//! on its way back while the menu builds itself.

use alpymist_menu::config::Appearance;
use alpymist_menu::menu::{Key, Menu, Outcome};
use alpymist_menu::tree::{Alternate, EntryId};
use alpymist_menu::view::{self, Fonts, Layout};
use alpymist_wayland::{
    BTN_LEFT, Event, KeyEvent, Keysym, Layer, LayerOptions, Modifiers, Picture, PointerEvent,
    PointerKind, Source, Stratum, Wayland,
};
use denise::geom::Point;
use denise::{BufferAge, Frame, PixelFormat};
use std::os::fd::AsFd;
use std::os::unix::net::UnixListener;
use std::time::Instant;

/// The layer surface's namespace, for compositor rules:
/// `layerrule = match:namespace alpymist-menu, …` in Hyprland.
pub const NAMESPACE: &str = "alpymist-menu";

/// A connection with the surface already requested, waiting for the menu.
pub struct Pending {
    host: Host,
}

/// Everything the Wayland events need.
// The flags are the frame-pacing state machine, each read in its own place;
// folding them into an enum would only hide which event owns which.
#[allow(clippy::struct_excessive_bools)]
struct Host {
    wayland: Wayland,
    layer: Layer,
    /// The picture, painted here and handed over whole.
    canvas: Vec<u32>,

    appearance: Appearance,
    layout: Layout,
    menu: Option<Menu>,
    fonts: Option<Fonts>,
    notice: Option<String>,

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
/// No Wayland session, or a compositor without the layer shell — Hyprland
/// has it; GNOME does not.
pub fn connect(appearance: Appearance, trace: Option<Instant>) -> Result<Pending, String> {
    let mut wayland = Wayland::connect()?;
    let layout = Layout::new(&appearance, 1);
    let layer = wayland.layer(&LayerOptions {
        namespace: NAMESPACE,
        stratum: Stratum::Overlay,
        output: None,
        // No anchor: the compositor centres it on the focused output.
        fill: false,
        keyboard: true,
        exclusive_zone: -1,
        size: layout.logical_size(),
    })?;
    wayland.flush();

    Ok(Pending {
        host: Host {
            wayland,
            layer,
            canvas: Vec::new(),
            appearance,
            layout,
            menu: None,
            fonts: None,
            notice: None,
            modifiers: Modifiers::default(),
            scroll_rest: 0.0,
            configured: false,
            frame_pending: false,
            dirty: false,
            exit: false,
            chosen: None,
            trace,
            first_frame_logged: false,
        },
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
    // Owned, though only looked at: they are this loop's for as long as it
    // runs, and gone when it ends.
    #[allow(clippy::needless_pass_by_value)]
    pub fn run(
        self,
        menu: Menu,
        fonts: Fonts,
        notice: Option<String>,
        toggle: Option<UnixListener>,
    ) -> Result<(Option<EntryId>, Menu), String> {
        let mut host = self.host;
        host.menu = Some(menu);
        host.fonts = Some(fonts);
        host.notice = notice;

        let toggled: Option<Source> = match &toggle {
            Some(listener) => Some(
                host.wayland
                    .watch(listener.as_fd())
                    .map_err(|e| format!("event loop: {e}"))?,
            ),
            None => None,
        };

        // Nothing has been read yet, so the configure the surface is
        // waiting for arrives, and the first frame is drawn, inside this loop.
        while !host.exit {
            for event in host.wayland.wait()? {
                match event {
                    // Another run of the menu: this one goes.
                    Event::Ready(source) if Some(source) == toggled => host.exit = true,
                    event => host.on(event),
                }
            }
        }

        let Host {
            wayland,
            layer,
            menu,
            chosen,
            ..
        } = host;
        drop(layer);
        wayland.flush();
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
        let size = layout.size;
        let words = size.width as usize * size.height as usize;
        // Every pixel is painted, so what the last frame left needs no
        // clearing.
        self.canvas.resize(words, 0);
        let notice = self.notice.as_deref();
        if let Ok(mut frame) = Frame::new(
            &mut self.canvas,
            size,
            size.width,
            PixelFormat::Argb8888,
            BufferAge::Undefined,
        ) {
            view::paint(&mut frame, &layout, &self.appearance, fonts, menu, notice);
        }

        let shown = self.wayland.show(
            self.layer.surface(),
            &Picture {
                size: (size.width, size.height),
                scale: layout.scale,
                pixels: &self.canvas,
                opaque: false,
                damage: None,
                paced: true,
            },
        );
        if let Err(e) = shown {
            eprintln!("alpymist-menu: {e}");
            self.exit = true;
            return;
        }
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
            Outcome::Alternate(entry, which) => {
                if let Some(menu) = self.menu.as_mut() {
                    menu.perform(entry, which);
                }
                self.draw();
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
            Keysym::s if ctrl => Some(Key::Alternate(Alternate::Pin)),
            Keysym::Delete | Keysym::KP_Delete => Some(Key::Alternate(Alternate::Forget)),
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

impl Host {
    /// One thing the compositor said.
    fn on(&mut self, event: Event) {
        match event {
            Event::Configure { .. } => {
                // The size is ours to choose and does not change; a configure
                // only says the surface may now be drawn. A buffer is still
                // owed for every configure, so a pending frame does not
                // excuse this one.
                self.configured = true;
                self.frame_pending = false;
                if let Some(start) = self.trace {
                    eprintln!("trace: configured at {:?}", start.elapsed());
                }
                self.draw();
            }
            Event::Closed(_) => self.exit = true,
            Event::Scale { scale, .. } => {
                if scale != self.layout.scale {
                    self.layout = Layout::new(&self.appearance, scale);
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
            Event::Unfocused(surface) => {
                // Focus went elsewhere — a click on another window, a
                // workspace switch. A menu left open behind that is a menu
                // in the way.
                if &surface == self.layer.surface() {
                    self.exit = true;
                }
            }
            Event::Pointer(event) => self.on_pointer(&event),
            _ => {}
        }
    }

    fn on_pointer(&mut self, event: &PointerEvent) {
        if &event.surface != self.layer.surface() {
            return;
        }
        let point = self.physical(event.position);
        let row = self.layout.row_at(point);
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        let outcome = match event.kind {
            // Motion only, not Enter: a menu that opens under a resting
            // pointer must not have its selection moved by it.
            PointerKind::Motion => row.map_or(Outcome::Unchanged, |r| menu.hover(r)),
            PointerKind::Press { button } if button == BTN_LEFT => {
                row.map_or(Outcome::Unchanged, |r| menu.click(r))
            }
            PointerKind::Scroll { distance, steps } => {
                let rows = if steps != 0 {
                    steps
                } else {
                    // Touchpads report distance: a row's worth per row.
                    self.scroll_rest += distance;
                    let row_h = f64::from(self.layout.row.height) / f64::from(self.layout.scale);
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

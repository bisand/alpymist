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
use alpymist_wayland::{
    Area, BTN_LEFT, Event, KeyEvent, Keysym, Layer, LayerOptions, Modifiers, Output, Picture,
    PointerEvent, PointerKind, Shape, Stratum, Surface, Timer, Wayland,
};
use denise::geom::{Point, Rect};
use denise::{BufferAge, Frame, PixelFormat};
use std::os::fd::AsFd;
use std::os::unix::net::UnixListener;

/// The sending end of a widget's events: clone it into each thread.
pub type Sender<E> = alpymist_wayland::Sender<E>;

/// The receiving end of a widget's events, handed to [`run`].
pub type Events<E> = alpymist_wayland::Receiver<E>;

/// A channel for a widget's threads to post into the event loop.
#[must_use]
pub fn events<E>() -> (Sender<E>, Events<E>) {
    alpymist_wayland::channel()
}

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
    /// The output to be on, by name (`DP-3`). `None` is wherever the
    /// compositor puts a new surface, which is the output with focus.
    pub output: Option<String>,
    /// With [`Placement::FullScreen`], cover every other output in black as
    /// well, and any output plugged in while it is up: a screensaver on one
    /// screen and the others dark.
    pub cover_others: bool,
    /// Whether the panel takes the keyboard while it is up. A label shown
    /// over another program's window, such as the number Settings puts on
    /// each screen, leaves it where it was.
    pub keyboard: bool,
}

/// What the other outputs are covered in: opaque black, which is as dark as a
/// screen that is on gets, and on OLED as good as off.
const COVER: u32 = 0xFF00_0000;

/// A black surface on an output the widget is not on.
struct Cover {
    output: Output,
    layer: Layer,
}

/// Where a panel is put on the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// In the top right corner, under the bar: a popup.
    UnderBar,
    /// In the middle of the output: a dialog.
    Centre,
    /// The whole output: a screensaver.
    ///
    /// [`Widget::layout`] is still called, so the widget knows the scale, but
    /// what it returns is ignored — the panel is the surface the compositor
    /// gave, and the widget paints into a frame that size.
    FullScreen,
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
            output: None,
            cover_others: false,
            keyboard: true,
        }
    }
}

// The flags are the frame-pacing state machine, as in the menu's host.
#[allow(clippy::struct_excessive_bools)]
struct Host<W: Widget> {
    wayland: Wayland,
    layer: Layer,
    probe: Option<Layer>,
    /// The picture: the whole surface, painted here and handed over whole.
    canvas: Vec<u32>,
    namespace: String,
    /// Whether to cover the other outputs, and those covered so far.
    cover_others: bool,
    covers: Vec<Cover>,
    /// The output the widget's surface is on, once the compositor says, and
    /// its name.
    on: Option<Output>,
    on_name: Option<String>,
    /// Where the pointer was first seen on a cover, so only a real move counts.
    cover_pointer: Option<(f64, f64)>,

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

    modifiers: Modifiers,
    pointer_at: Option<(f64, f64)>,
    scroll_rest: f64,

    configured: bool,
    frame_pending: bool,
    dirty: bool,
    /// What paints the next frame of an animation, while there is one.
    ticking: Option<Timer>,
    exit: bool,
    dismissed: bool,
    /// The compositor closed the widget's surface: its output went away.
    lost: bool,
}

/// Open `widget` and run it until it closes. Returns whether it was
/// dismissed from outside — a click elsewhere, or focus going elsewhere —
/// which is what [`crate::instance::toggle`] wants to know.
///
/// `toggle` is the listener from [`crate::instance::toggle`]: a connection to
/// it closes the popup.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell; or
/// [`LOST`], when the compositor took the surface away.
// Owned, though only looked at: they are this loop's for as long as it
// runs, and gone when it ends.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines)] // setting up two surfaces, in order
pub fn run<W: Widget>(
    mut widget: W,
    options: &Options,
    events: Events<W::Event>,
    toggle: Option<UnixListener>,
) -> Result<bool, String> {
    let mut wayland = Wayland::connect()?;
    let size = widget.layout(1);

    // A named output costs a round trip to find; everything else starts on
    // the output with focus, with no wait.
    let output = match &options.output {
        Some(name) => Some(
            crate::outputs::on(&mut wayland)?
                .into_iter()
                .find(|(screen, _)| &screen.name == name)
                .map(|(_, output)| output)
                .ok_or_else(|| format!("no output called {name}"))?,
        ),
        None => None,
    };

    // Anchored to every edge, the compositor sizes it; an exclusive zone of
    // -1 puts it over the bar as well.
    let layer = wayland.layer(&LayerOptions {
        namespace: &options.namespace,
        stratum: Stratum::Overlay,
        output: output.as_ref(),
        fill: true,
        // Exclusive, as the menu: typing works the moment the popup is up. A
        // label over another program's window takes nothing.
        keyboard: options.keyboard,
        exclusive_zone: -1,
        size: (0, 0),
    })?;

    // What the bar leaves is only of interest to something placed under it; a
    // full-screen widget covers the bar as well, and the extra surface would be
    // one more round trip before the first frame.
    let probe = if options.placement == Placement::FullScreen {
        None
    } else {
        Some(wayland.layer(&LayerOptions {
            namespace: &format!("{}-probe", options.namespace),
            stratum: Stratum::Background,
            output: None,
            fill: true,
            keyboard: false,
            exclusive_zone: 0,
            size: (0, 0),
        })?)
    };
    wayland.flush();

    let posted = wayland
        .watch(events.fd())
        .map_err(|e| format!("event loop: {e}"))?;
    let toggled = match &toggle {
        Some(listener) => Some(
            wayland
                .watch(listener.as_fd())
                .map_err(|e| format!("event loop: {e}"))?,
        ),
        None => None,
    };

    let mut host = Host {
        wayland,
        layer,
        probe,
        canvas: Vec::new(),
        namespace: options.namespace.clone(),
        cover_others: options.cover_others && options.placement == Placement::FullScreen,
        covers: Vec::new(),
        on: None,
        on_name: options.output.clone(),
        cover_pointer: None,
        widget,
        margin: options.margin,
        placement: options.placement,
        backdrop: options.backdrop,
        scale: 1,
        size,
        screen: None,
        free: None,
        drawn: None,
        modifiers: Modifiers::default(),
        pointer_at: None,
        scroll_rest: 0.0,
        configured: false,
        frame_pending: false,
        dirty: false,
        ticking: None,
        exit: false,
        dismissed: false,
        lost: false,
    };

    while !host.exit {
        for event in host.wayland.wait()? {
            match event {
                Event::Ready(source) if source == posted => {
                    for event in events.take() {
                        let outcome = host.widget.event(event);
                        host.apply(outcome);
                    }
                }
                // Another run of the program: this one goes.
                Event::Ready(source) if Some(source) == toggled => host.exit = true,
                event => host.on(event),
            }
        }
    }
    let dismissed = host.dismissed;
    let lost = host.lost;
    drop(host.probe.take());
    host.covers.clear();
    drop(host.layer);
    host.wayland.flush();
    if lost {
        return Err(LOST.to_owned());
    }
    Ok(dismissed)
}

/// What [`run`] says when the compositor took the widget's surface away,
/// which it does when the output it was on is unplugged or turned off: not a
/// failure, and for a screensaver a reason to go to another screen.
pub const LOST: &str = "its screen went away";

impl<W: Widget> Host<W> {
    /// Put a black surface on every output the widget is not on, once it is
    /// known which one that is.
    fn cover(&mut self) {
        if !self.cover_others || (self.on.is_none() && self.on_name.is_none()) {
            return;
        }
        for output in self.wayland.outputs() {
            let name = self.wayland.info(&output).and_then(|i| i.name);
            let ours =
                self.on.as_ref() == Some(&output) || (name.is_some() && name == self.on_name);
            if ours || self.covers.iter().any(|c| c.output == output) {
                continue;
            }
            let Ok(layer) = self.wayland.layer(&LayerOptions {
                namespace: &format!("{}-cover", self.namespace),
                stratum: Stratum::Overlay,
                output: Some(&output),
                fill: true,
                keyboard: false,
                exclusive_zone: -1,
                size: (0, 0),
            }) else {
                return;
            };
            self.covers.push(Cover { output, layer });
        }
    }

    /// Fill a cover the compositor has sized. Once: it never changes.
    fn paint_cover(&mut self, index: usize, width: u32, height: u32) {
        let Some(cover) = self.covers.get(index) else {
            return;
        };
        let black = vec![COVER; width as usize * height as usize];
        let _ = self.wayland.show(
            cover.layer.surface(),
            &Picture {
                size: (width, height),
                scale: 1,
                pixels: &black,
                opaque: false,
                damage: None,
                paced: false,
            },
        );
    }

    /// Which cover, if any, `surface` is.
    fn cover_of(&self, surface: &Surface) -> Option<usize> {
        self.covers
            .iter()
            .position(|c| c.layer.surface() == surface)
    }

    fn dismiss(&mut self) {
        self.dismissed = !self.exit;
        self.exit = true;
    }

    /// The panel's top left corner on the surface, in physical pixels: in
    /// the top right corner of the space the bar leaves.
    fn origin(&self) -> Point {
        if self.placement == Placement::FullScreen {
            return Point::new(0, 0);
        }
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

    /// The whole surface, in physical pixels.
    fn surface(&self) -> denise::geom::Size {
        let (w, h) = self.screen.unwrap_or((1, 1));
        let s = self.scale.max(1);
        denise::geom::Size::new((w * s).max(1), (h * s).max(1))
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
        if self.placement == Placement::FullScreen {
            self.size = self.surface();
        }
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
        let words = usize::try_from(w).unwrap_or(0) * usize::try_from(h).unwrap_or(0);
        self.canvas.resize(words, 0);
        // Everything outside the panel is transparent, or the backdrop; the
        // canvas holds the frame before, where the panel may have been
        // somewhere else, so the whole of it is cleared. A full-screen widget
        // has nothing outside its panel and paints every visible pixel
        // itself, and clearing first would be a second pass over the whole
        // screen for nothing — which on the machines this targets is most of
        // the cost of a frame.
        if self.placement != Placement::FullScreen {
            self.canvas.fill(self.backdrop);
        }

        if fits
            && let Some(region) = self.canvas.get_mut(start..)
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

        // What changed is where the panel was and where it is; with a
        // backdrop, the backdrop too, the first time.
        let whole = Rect::new(0, 0, w, h);
        let area = |rect: Rect| Area {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        };
        let damage: Vec<Area> = match (self.drawn, self.widget.changed()) {
            (None, _) if self.backdrop != 0 => vec![area(whole)],
            (None, _) => vec![area(panel)],
            // The panel has not moved, and the widget says which parts of it
            // changed: those, where they are on the surface.
            (Some(old), Some(parts)) if old == panel => parts
                .iter()
                .filter_map(|part| {
                    Rect::new(panel.x + part.x, panel.y + part.y, part.width, part.height)
                        .intersect(&panel)
                })
                .map(area)
                .collect(),
            (Some(old), _) => vec![area(old.union(&panel))],
        };
        let shown = self.wayland.show(
            self.layer.surface(),
            &Picture {
                size: (sw * self.scale, sh * self.scale),
                scale: self.scale,
                pixels: &self.canvas,
                opaque: false,
                damage: Some(&damage),
                paced: true,
            },
        );
        if let Err(e) = shown {
            eprintln!("{}: {e}", program());
            self.exit = true;
            return;
        }
        self.drawn = Some(panel);
        self.frame_pending = true;
        self.dirty = false;
        self.arm_timer();
    }

    /// Keep painting while the widget animates.
    fn arm_timer(&mut self) {
        if self.ticking.is_none() && self.widget.animating() {
            self.ticking = Some(self.wayland.after(self.widget.frame_interval()));
        }
    }

    /// The animation's next frame is due.
    fn tick(&mut self) {
        self.ticking = None;
        if self.exit || !self.widget.animating() {
            return;
        }
        let outcome = self.widget.tick();
        self.apply(outcome);
        self.arm_timer();
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
            // Paste, as typing: a key or a passphrase is long to type.
            Keysym::v if ctrl => return self.paste(),
            Keysym::Insert if self.modifiers.shift => return self.paste(),
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

    /// Hand the widget the clipboard's first line as if it were typed. The
    /// clipboard is read through `wl-paste`, as Alpymist's clipboard history
    /// is: a widget with nowhere for text ignores it, as it does typing.
    fn paste(&mut self) {
        let Ok(output) = std::process::Command::new("wl-paste")
            .args(["--no-newline", "--type", "text"])
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return;
        };
        if !output.status.success() {
            return;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut outcome = Outcome::Unchanged;
        for ch in text.lines().next().unwrap_or("").chars().take(PASTE_MOST) {
            outcome = outcome.and(self.widget.text(ch));
        }
        self.apply(outcome);
    }

    fn physical(&self, (x, y): (f64, f64)) -> Point {
        let s = f64::from(self.scale);
        #[allow(clippy::cast_possible_truncation)]
        Point::new((x * s) as i32, (y * s) as i32)
    }
}

/// The most characters taken from one paste.
const PASTE_MOST: usize = 4096;

/// The program's name, for the log.
fn program() -> String {
    std::env::args()
        .next()
        .and_then(|a| a.rsplit('/').next().map(str::to_owned))
        .unwrap_or_else(|| "widget".into())
}

impl<W: Widget> Host<W> {
    /// One thing the compositor said.
    fn on(&mut self, event: Event) {
        match event {
            Event::Configure { surface, size } => self.configure(&surface, size),
            Event::Closed(surface) => {
                // A cover goes with its output, and the widget carries on.
                if let Some(index) = self.cover_of(&surface) {
                    self.covers.remove(index);
                    return;
                }
                // Either other surface: the popup means nothing without the
                // other. The compositor closes one when its output goes away.
                self.lost = true;
                self.exit = true;
            }
            Event::Scale { surface, scale } => {
                if &surface != self.layer.surface() || scale == self.scale {
                    return;
                }
                self.scale = scale;
                self.drawn = None;
                self.draw();
            }
            Event::Frame(surface) => {
                if &surface == self.layer.surface() {
                    self.frame_pending = false;
                    if self.dirty {
                        self.draw();
                    }
                }
            }
            Event::Entered { surface, output } => {
                if &surface == self.layer.surface() {
                    if self.on_name.is_none() {
                        self.on_name = self.wayland.info(&output).and_then(|i| i.name);
                    }
                    if self.on.is_none() {
                        self.on = Some(output);
                    }
                    self.cover();
                }
            }
            // Plugged in while the widget is up: dark as well, if the rest
            // are.
            Event::Output(_) => self.cover(),
            Event::OutputGone(output) => self.covers.retain(|c| c.output != output),
            // Pressed, or held long enough to count again: the same.
            Event::Key(key) => self.on_key(&key),
            Event::Modifiers(modifiers) => {
                let caps = modifiers.caps_lock != self.modifiers.caps_lock;
                self.modifiers = modifiers;
                if caps {
                    let outcome = self.widget.caps_lock(modifiers.caps_lock);
                    self.apply(outcome);
                }
            }
            Event::Unfocused(surface) => {
                // Focus taken without a click — a workspace switch from the
                // keyboard: a popup left open behind it is in the way. Not a
                // full-screen widget, which nothing is behind: with one on
                // each screen, each takes the keyboard from the last as it
                // comes up, and only a key or the pointer should take them
                // away.
                if self.layer.surface() == &surface && self.placement != Placement::FullScreen {
                    self.dismiss();
                }
            }
            Event::Pointer(event) => self.on_pointer(&event),
            Event::Timer(timer) if self.ticking == Some(timer) => self.tick(),
            _ => {}
        }
    }

    fn configure(&mut self, surface: &Surface, (w, h): (u32, u32)) {
        if let Some(index) = self.cover_of(surface) {
            self.paint_cover(index, w, h);
            return;
        }
        if self.probe.as_ref().is_some_and(|p| p.surface() == surface) {
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

    fn on_pointer(&mut self, event: &PointerEvent) {
        if &event.surface != self.layer.surface() {
            // Moving over a darkened screen is someone back, as it would be
            // over the widget's own. Only a move: a cover appearing under a
            // still pointer is told where the pointer is, and that is not
            // anybody.
            if self.cover_of(&event.surface).is_some() {
                match event.kind {
                    PointerKind::Enter { .. } | PointerKind::Motion => match self.cover_pointer {
                        None => self.cover_pointer = Some(event.position),
                        Some(first) if first != event.position => self.dismiss(),
                        Some(_) => {}
                    },
                    PointerKind::Leave => self.cover_pointer = None,
                    _ => {}
                }
            }
            return;
        }
        let at = self.on_panel(event.position);
        let outcome = match event.kind {
            PointerKind::Enter { serial } => {
                // Nothing else sets the cursor over the empty part of the
                // surface, so whatever the pointer last wore would stay.
                self.wayland.cursor(serial, Shape::Default);
                self.pointer_at = Some(event.position);
                self.widget.pointer(at)
            }
            PointerKind::Motion => {
                self.pointer_at = Some(event.position);
                self.widget.pointer(at)
            }
            PointerKind::Leave => {
                self.pointer_at = None;
                self.widget.pointer(None)
            }
            PointerKind::Press { .. } if at.is_none() => {
                self.dismiss();
                return;
            }
            PointerKind::Press { button } if button == BTN_LEFT => {
                at.map_or(Outcome::Unchanged, |p| self.widget.press(p))
            }
            PointerKind::Scroll { distance, steps } => {
                let rows = if steps != 0 {
                    steps
                } else {
                    self.scroll_rest += distance;
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

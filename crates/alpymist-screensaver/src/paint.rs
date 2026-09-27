//! What a screensaver program has to write, and the one call that runs it.
//!
//! A screensaver is its own program, but it does not have to be its own Wayland
//! client. It draws a small picture; this puts that picture on the screen —
//! full-screen layer surface, square-block magnification, frame pacing, the
//! keyboard and pointer that take it away, and the socket that lets
//! `alpymist-screensaver stop` reach it. A whole screensaver is:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     let values = alpymist_screensaver::values_for("mountains");
//!     alpymist_screensaver::paint::start(move |output| {
//!         Box::new(Mountains::compose(output, values.number("block", 6)))
//!     })
//! }
//! ```
//!
//! Drawing small is the whole reason this is affordable on the hardware
//! Alpymist exists for. A picture composed at a fraction of the screen's
//! resolution is a few hundred thousand pixels of work rather than a few
//! million, and the square blocks it is blown up in are the pixelated look
//! rather than an effect applied to get one.

use crate::screens::Screen;
use denise::geom::Size;

/// How often a picture is redrawn when it does not say, in milliseconds.
///
/// Eight frames a second. A picture of weather needs nothing faster than the
/// eye drifting over it, and each frame that is not drawn is a frame's worth of
/// battery: on the 1366x768 panel of an Atom laptop, every frame a second costs
/// about two per cent of a core.
pub const FRAME: u64 = 125;

/// The most frames a second a picture may ask for.
///
/// Thirty. Past that the magnification alone — a screen's worth of memory
/// copied per frame — is most of a core on the machines Alpymist exists for,
/// and a screensaver that keeps a core busy drains the battery it was there to
/// idle through.
pub const FASTEST: u64 = 30;

/// A screensaver's picture: a small one, drawn again for each frame.
pub trait Painting {
    /// The reduced size this is drawn at, in drawn pixels.
    fn small(&self) -> Size;

    /// How many physical pixels one drawn pixel covers.
    fn block(&self) -> u32;

    /// Draw the frame at `elapsed` milliseconds, and hand back the pixels.
    ///
    /// Row-major, `small().width * small().height` of them, opaque `Argb8888`.
    /// Anything transparent would show the desktop through the screensaver.
    fn frame(&mut self, elapsed: u64) -> &[u32];

    /// How long between frames, in milliseconds.
    ///
    /// [`FRAME`] unless a picture says otherwise, which is drifting mist's
    /// answer and most pictures'. Something the eye follows rather than drifts
    /// over — anything travelling in a straight line, where the judder of a
    /// slow frame is the whole of what you see — should ask for less, and pay
    /// for it: every frame is a wakeup, and a wakeup on a battery costs the
    /// same whether much moved in it or not.
    ///
    /// Clamped to at most [`FASTEST`] frames a second by [`interval`], so a
    /// picture asking for a hundred gets thirty rather than the machine's
    /// whole afternoon.
    fn interval_ms(&self) -> u64 {
        FRAME
    }
}

/// The gap between frames a picture asking for `fps` frames a second gets.
///
/// Zero, or anything absurd, is the default rather than an error: a screensaver
/// is what a machine shows when nobody is at it, and refusing to draw over a
/// mistyped number is the one failure nobody would be there to see.
#[must_use]
pub fn interval(fps: i64) -> u64 {
    let Ok(fps) = u64::try_from(fps) else {
        return FRAME;
    };
    if fps == 0 {
        return FRAME;
    }
    1000 / fps.min(FASTEST)
}

/// Composes a picture for a screen of a given size.
///
/// Called when the screensaver appears, and again if the screen it is on
/// changes size. A picture is composed for one size and drawn many times.
pub type Compose = dyn FnMut(Size) -> Box<dyn Painting>;

/// The name every screensaver runs under.
///
/// One name for all of them, not one each: only one screensaver is ever up, and
/// `alpymist-screensaver stop` has to be able to take away whichever it is
/// without knowing which it is. A copy drawing on one screen of several runs as
/// `alpymist-screensaver@DP-3`; see [`start`].
pub const NAME: &str = "alpymist-screensaver";

/// The variable that puts a screensaver on one screen, by name.
///
/// Set by [`start`] for each copy it starts in [`crate::config::Config::every_screen`]
/// mode, never by hand. A screensaver program needs to know nothing about it.
pub const OUTPUT: &str = "ALPYMIST_SCREENSAVER_OUTPUT";

/// What a copy on the screen `output` runs as.
#[must_use]
pub fn name_on(output: &str) -> String {
    format!("{NAME}@{output}")
}

/// The compositor's screens, by name, first first; empty where there is no
/// session or they have no names. What `alpymist-screensaver screens` prints.
#[must_use]
pub fn screens() -> Vec<Screen> {
    #[cfg(target_os = "linux")]
    {
        alpymist_widget::outputs::screens()
            .unwrap_or_default()
            .into_iter()
            .map(|s| Screen {
                name: s.name,
                description: s.description,
            })
            .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// Cover the screens with what `compose` draws, until somebody comes back.
///
/// On the main screen, with the others dark: the first screen the compositor
/// numbers (Hyprland's monitor 0, the laptop's own panel while it is on), or
/// the one `main-screen` in `screensaver.toml` names while that one is
/// connected. A picture costs a frame clock on each screen it is on, and the
/// others only need to stop showing the desktop.
///
/// With `every-screen` the picture is on all of them instead. Then this
/// process starts one copy of the same program per screen, each told its
/// screen in [`OUTPUT`] and each drawing at that screen's own size, and takes
/// them all away together, when any of them is left or when [`stop`] asks.
/// With one screen there is only ever the one.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell.
#[cfg(target_os = "linux")]
pub fn start(compose: impl FnMut(Size) -> Box<dyn Painting> + 'static) -> Result<(), String> {
    use alpymist_widget::host;

    let copy = std::env::var(OUTPUT).ok().filter(|o| !o.is_empty());
    let output = if let Some(output) = &copy {
        Some(output.clone())
    } else {
        let config = crate::config::Config::load().unwrap_or_default();
        let all = screens();
        if config.every_screen && all.len() > 1 {
            let names: Vec<String> = all.into_iter().map(|s| s.name).collect();
            return alpymist_widget::instance::toggle(NAME, |listener| {
                everywhere(&names, listener);
                Ok(false)
            });
        }
        // A compositor that does not name its screens: wherever it puts the
        // picture, which is the screen with focus.
        crate::screens::main_screen(&config.main_screen, &all).map(|s| s.name.clone())
    };

    let name = copy.as_deref().map_or_else(|| NAME.to_owned(), name_on);
    alpymist_widget::instance::toggle(&name, |listener| {
        let mut options = host::Options::new(NAME);
        options.placement = host::Placement::FullScreen;
        // Under the first frame, and under the edges of a screen the blocks do
        // not quite divide.
        options.backdrop = crate::saver::backdrop();
        // One copy of several has a screen of its own; the main one darkens
        // the rest.
        options.cover_others = copy.is_none();
        options.output = output;
        let (_sender, events) = host::events::<()>();
        host::run(
            crate::saver::Saver::new(Box::new(compose)),
            &options,
            events,
            listener,
        )
    })
}

/// Run a copy of this program on each of `screens`, and take them all away
/// when one of them goes or `listener` hears from [`stop`].
///
/// Blocked, not polling: a thread waits on each copy and one on the socket,
/// and the first to finish says so. A screensaver's whole job is to let the
/// machine idle.
#[cfg(target_os = "linux")]
fn everywhere(screens: &[String], listener: Option<std::os::unix::net::UnixListener>) {
    use std::sync::mpsc;

    let Ok(me) = std::env::current_exe() else {
        return;
    };
    let (done, finished) = mpsc::channel::<()>();
    for screen in screens {
        let Ok(mut copy) = std::process::Command::new(&me).env(OUTPUT, screen).spawn() else {
            continue;
        };
        let done = done.clone();
        std::thread::spawn(move || {
            copy.wait().ok();
            done.send(()).ok();
        });
    }
    if let Some(listener) = listener {
        std::thread::spawn(move || {
            listener.accept().ok();
            done.send(()).ok();
        });
    }
    finished.recv().ok();
    stop_copies();
}

/// There is no layer shell off Linux; a screensaver's own logic still builds.
///
/// # Errors
/// Always: there is nowhere to put a surface.
#[cfg(not(target_os = "linux"))]
pub fn start(_compose: impl FnMut(Size) -> Box<dyn Painting> + 'static) -> Result<(), String> {
    Err("a screensaver needs a Wayland compositor".to_owned())
}

/// Take away whichever screensaver is up, if any.
///
/// Nothing to take away is not a failure: swayidle runs this on resume whether
/// or not the timeout before it ever fired.
pub fn stop() {
    if let Some(path) = alpymist_widget::instance::socket_path(NAME) {
        // Connecting is what closes it; there is nothing to say afterwards.
        std::os::unix::net::UnixStream::connect(path).ok();
    }
    // Copies on each screen go when the one that started them does; this is
    // for any left behind by one that did not get to.
    stop_copies();
}

/// Take away every copy on a screen of its own.
fn stop_copies() {
    let Some(ours) = alpymist_widget::instance::socket_path(&name_on("")) else {
        return;
    };
    // `alpymist-screensaver@-wayland-1.sock`: each copy's socket is this with
    // its screen's name after the `@`.
    let Some(file) = ours.file_name().and_then(|f| f.to_str()) else {
        return;
    };
    let Some((prefix, suffix)) = file.split_once('@') else {
        return;
    };
    let (prefix, suffix) = (format!("{prefix}@"), suffix.to_owned());
    let Some(dir) = ours.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&prefix) && name.ends_with(&suffix) && name != file {
            std::os::unix::net::UnixStream::connect(entry.path()).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::name_on;

    #[test]
    fn a_copy_on_a_screen_is_named_after_it() {
        assert_eq!(name_on("DP-3"), "alpymist-screensaver@DP-3");
    }
}

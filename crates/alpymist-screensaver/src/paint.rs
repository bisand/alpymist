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
/// without knowing which it is.
pub const NAME: &str = "alpymist-screensaver";

/// Cover the screen with what `compose` draws, until somebody comes back.
///
/// # Errors
/// No Wayland session, or a compositor without the layer shell.
#[cfg(target_os = "linux")]
pub fn start(compose: impl FnMut(Size) -> Box<dyn Painting> + 'static) -> Result<(), String> {
    use alpymist_widget::host;

    alpymist_widget::instance::toggle(NAME, |listener| {
        let mut options = host::Options::new(NAME);
        options.placement = host::Placement::FullScreen;
        // Under the first frame, and under the edges of a screen the blocks do
        // not quite divide.
        options.backdrop = crate::saver::backdrop();
        let (_sender, events) = host::events::<()>();
        host::run(
            crate::saver::Saver::new(Box::new(compose)),
            &options,
            events,
            listener,
        )
    })
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
}

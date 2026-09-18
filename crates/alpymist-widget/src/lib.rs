//! Popups under the bar.
//!
//! Alpymist's Wi-Fi and power popups are built on this crate, and so can
//! anyone else's. A widget is three things kept apart, as in the menu: its
//! state and what every key and click does to it, as plain Rust with no
//! pixels; a layout and a painter, drawn with Denise; and the [`Widget`]
//! trait joining the two, which the [`host`] puts on screen.
//!
//! What the crate gives a widget:
//!
//! - [`host::run`]: a wlr layer surface that covers the output, transparent
//!   but for the panel in the corner under the bar, so a click anywhere else
//!   closes it; keyboard focus the moment it opens; frame pacing, output
//!   scale, key repeat and the pointer.
//! - [`instance::toggle`]: running the binary again closes the open popup,
//!   which is what makes one click on a bar icon open it and the next close
//!   it.
//! - [`draw`]: fonts, colours from the menu's appearance, and the controls —
//!   buttons, switches, segmented choices, check boxes, meters, focus rings,
//!   label and value grids.
//! - [`focus::FocusRing`]: Tab order over whatever controls are present.
//! - [`waybar`]: printing a Waybar custom module's line whenever it changes.
//! - `window`: an ordinary application window, for what stays open beside
//!   people's work rather than dropping from the bar.
//!
//! A minimal widget:
//!
//! ```ignore
//! use alpymist_widget::{Key, Outcome, Widget, draw};
//!
//! struct Hello { fonts: draw::Fonts, appearance: alpymist_widget::Appearance, scale: u32 }
//!
//! impl Widget for Hello {
//!     type Event = ();
//!     fn layout(&mut self, scale: u32) -> denise::geom::Size {
//!         self.scale = scale;
//!         denise::geom::Size::new(200 * scale, 80 * scale)
//!     }
//!     fn paint(&mut self, frame: &mut denise::Frame<'_>) { /* draw::panel, … */ }
//!     fn key(&mut self, key: Key) -> Outcome {
//!         if key == Key::Escape { Outcome::Close } else { Outcome::Unchanged }
//!     }
//! }
//!
//! alpymist_widget::instance::toggle("hello", |listener| {
//!     let (_sender, events) = alpymist_widget::host::events();
//!     alpymist_widget::host::run(hello, &alpymist_widget::host::Options::new("hello"), events, listener)
//! })
//! ```

#![forbid(unsafe_code)]

pub mod draw;
pub mod focus;
#[cfg(target_os = "linux")]
pub mod host;
pub mod instance;
pub mod waybar;
#[cfg(target_os = "linux")]
pub mod window;

pub use alpymist_menu::config::{Appearance, Colour};
use denise::Frame;
use denise::geom::{Point, Size};

/// Read the appearance the menu is configured with, so widgets and the menu
/// look alike.
#[must_use]
pub fn appearance() -> Appearance {
    alpymist_menu::config::load().config.appearance
}

/// What the host should do after a widget has handled something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Close the popup.
    Close,
}

impl Outcome {
    /// The more demanding of two outcomes: closing beats painting beats
    /// nothing.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Close, _) | (_, Self::Close) => Self::Close,
            (Self::Redraw, _) | (_, Self::Redraw) => Self::Redraw,
            _ => Self::Unchanged,
        }
    }

    /// `Redraw` when `changed`, otherwise `Unchanged`.
    #[must_use]
    pub fn redraw_if(changed: bool) -> Self {
        if changed {
            Self::Redraw
        } else {
            Self::Unchanged
        }
    }
}

/// A key, as far as a widget cares. The host maps keysyms to these, with
/// Emacs-style Ctrl chords folded in, and closes the popup at Ctrl+C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Up arrow, or Ctrl+K / Ctrl+P.
    Up,
    /// Down arrow, or Ctrl+J / Ctrl+N.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
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
    /// Space, without Ctrl.
    Space,
    /// Escape.
    Escape,
    /// Backspace.
    Backspace,
    /// Ctrl+U: clear a field.
    Clear,
}

/// A popup the [`host`] can run.
///
/// Coordinates are physical pixels relative to the panel's top left corner.
/// The host lays the widget out before every paint and before it asks what
/// is under the pointer, so a widget can keep its last layout and answer
/// from it.
pub trait Widget: 'static {
    /// What the widget's own threads send it: fresh readings, finished work.
    type Event: Send + 'static;

    /// Lay out at an output scale, and say how big the panel is in physical
    /// pixels.
    fn layout(&mut self, scale: u32) -> Size;

    /// Paint the panel into `frame`, which is the size [`Widget::layout`]
    /// returned.
    fn paint(&mut self, frame: &mut Frame<'_>);

    /// A key was pressed.
    fn key(&mut self, key: Key) -> Outcome;

    /// Text was typed, a character at a time.
    fn text(&mut self, ch: char) -> Outcome {
        let _ = ch;
        Outcome::Unchanged
    }

    /// The pointer is at `at` on the panel, or off it.
    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let _ = at;
        Outcome::Unchanged
    }

    /// The left button was pressed at `at` on the panel.
    fn press(&mut self, at: Point) -> Outcome {
        let _ = at;
        Outcome::Unchanged
    }

    /// The wheel turned by `rows` rows, downwards positive.
    fn scroll(&mut self, rows: i32) -> Outcome {
        let _ = rows;
        Outcome::Unchanged
    }

    /// Caps Lock went on or off: worth saying beside a password field.
    fn caps_lock(&mut self, on: bool) -> Outcome {
        let _ = on;
        Outcome::Unchanged
    }

    /// Whether the widget should be painted again every so often for now:
    /// a spinner turning.
    fn animating(&self) -> bool {
        false
    }

    /// How long between frames while animating.
    ///
    /// The default is the frame a spinner wants. Something that moves slowly
    /// should ask for longer: every frame is a wakeup, and a wakeup on a
    /// battery costs the same whether anything moved in it or not.
    fn frame_interval(&self) -> core::time::Duration {
        core::time::Duration::from_millis(40)
    }

    /// A frame passed while animating.
    fn tick(&mut self) -> Outcome {
        Outcome::Redraw
    }

    /// How far a touchpad must scroll for one row, in logical pixels.
    fn row_height(&self) -> f64 {
        36.0
    }

    /// Something arrived from the widget's own threads.
    fn event(&mut self, event: Self::Event) -> Outcome {
        let _ = event;
        Outcome::Redraw
    }
}

#[cfg(test)]
mod tests {
    use super::Outcome;

    #[test]
    fn closing_wins_and_nothing_loses() {
        assert_eq!(Outcome::Redraw.and(Outcome::Close), Outcome::Close);
        assert_eq!(Outcome::Unchanged.and(Outcome::Redraw), Outcome::Redraw);
        assert_eq!(
            Outcome::Unchanged.and(Outcome::Unchanged),
            Outcome::Unchanged
        );
    }
}

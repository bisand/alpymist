//! The overview: every workspace of a screen at once, to pick one from.
//!
//! Super+Tab shows the nine workspaces of the screen with the focus, three
//! by three, each a small copy of the screen with its windows where they
//! are. A window is drawn as an outline with the program's name in it, not
//! as a picture of itself: the places and sizes come from Hyprland's own
//! list of windows, which asks nothing of the compositor that any program
//! may not ask, and works on every processor. Hyprland's own overview is a
//! plugin that draws the windows themselves and loads only on `x86_64`
//! (ADR 0015); this is what the key shows everywhere else.
//!
//! As in the menu and the popups, three things are kept apart: [`model`] is
//! what there is to show and what a key or a click does, with no pixels;
//! [`view`] lays that out and paints it with Denise; and the program joins
//! the two to a surface over the screen.

#![forbid(unsafe_code)]

pub mod model;
pub mod view;

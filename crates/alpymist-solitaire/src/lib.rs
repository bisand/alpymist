//! Solitaire: Klondike, in a window of Alpymist's own.
//!
//! As in the menu, the popups and the overview, three things are kept
//! apart. [`cards`] and [`game`] are the deck and the rules, with no pixels:
//! what may be picked up, where it may be put down, and what a move leaves
//! behind. [`view`] lays a game out for a window and paints it with Denise,
//! and answers where a click landed. [`play`] is the hand on it: a drag, a
//! double click, the keyboard, a card's flight home and the clock, with no
//! window in it. [`kept`] is what stays from one opening to the next: the
//! back, the way the stock turns, and the best games. The program joins all
//! of it to a window.
//!
//! The table, the backs of the cards and the buttons take the theme's
//! colours. The faces are pictures, carried in the program: `cards/README.md`
//! says whose they are.

#![forbid(unsafe_code)]

pub mod cards;
pub mod faces;
pub mod game;
pub mod kept;
pub mod play;
pub mod view;

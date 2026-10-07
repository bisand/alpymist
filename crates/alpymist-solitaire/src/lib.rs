//! Solitaire: Klondike, in a window of Alpymist's own.
//!
//! As in the menu, the popups and the overview, three things are kept
//! apart. [`cards`] and [`game`] are the deck and the rules, with no pixels:
//! what may be picked up, where it may be put down, and what a move leaves
//! behind. [`view`] lays a game out for a window and paints it with Denise,
//! and answers where a click landed. The program joins the two to a window,
//! and is where a drag, a double click and a card's flight home live.
//!
//! The table, the backs of the cards and the buttons take the theme's
//! colours. The faces are pictures, carried in the program: `cards/README.md`
//! says whose they are.

#![forbid(unsafe_code)]

pub mod cards;
pub mod faces;
pub mod game;
pub mod view;

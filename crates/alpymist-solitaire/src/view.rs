//! Laying a game out for a window, and painting it.
//!
//! The cards are as large as the window lets seven of them stand side by
//! side with the two rows and a pile's fan under each other, so the table
//! fills whatever window it is given. The same rectangles that are painted
//! answer what is under the pointer and where a card let go of belongs.

use crate::cards::Card;
use crate::faces::{ASPECT, Faces};
use crate::game::{Game, PILES, Pile, Place};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise::{Color, Frame, PixelView};
use denise_render::Canvas;

/// A button on the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Deal again.
    New,
    /// Take the last move back.
    Undo,
    /// Turn one card from the stock, or three: deals again.
    Turn,
}

/// What is under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A button.
    Button(Button),
    /// The stock, or where it lay.
    Stock,
    /// The waste's top card.
    Waste,
    /// A foundation.
    Foundation(usize),
    /// A pile: `count` is how many cards lie from the one pointed at to the
    /// top, or 0 for a face-down card or an empty pile.
    Pile {
        /// Which pile.
        pile: usize,
        /// The card pointed at and those on it.
        count: usize,
    },
}

/// Cards off the table: in the hand, or on their way somewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifted {
    /// The pile whose top cards they are.
    pub place: Place,
    /// How many.
    pub count: usize,
    /// Where the first of them is drawn.
    pub at: Point,
}

/// What to paint beside the game itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct Scene {
    /// Cards drawn where they are held and not where they lie.
    pub lifted: Option<Lifted>,
    /// The button under the pointer.
    pub hover: Option<Button>,
    /// The pile the keyboard is at.
    pub focus: Option<Place>,
    /// The cards the keyboard has picked up, which stay where they lie.
    pub chosen: Option<(Place, usize)>,
}

/// Where everything is, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The sizes text and buttons are measured in.
    pub metrics: Metrics,
    /// The window.
    pub size: Size,
    /// The bar of buttons along the top.
    pub bar: Rect,
    /// The buttons on it.
    pub buttons: [(Button, Rect); 3],
    /// Where the bar says how the game stands.
    pub status: Rect,
    /// A card's width and height.
    pub card: (i32, i32),
    /// The space between piles.
    pub gap: i32,
    /// The first column's left edge.
    pub left: i32,
    /// The top of the row of stock, waste and foundations.
    pub top: i32,
    /// The top of the seven piles.
    pub piles: i32,
    /// How far a face-down card shows above the next, before a tall pile is
    /// squeezed.
    pub down: i32,
    /// How far a face-up one does.
    pub up: i32,
    /// A card's corner.
    pub radius: i32,
}

impl Layout {
    /// Lay the table out for a window of `size` at an output scale.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // m, u, w, h, x: the widgets' own
    pub fn new(appearance: &Appearance, size: Size, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, 0);
        let u = m.unit;
        let w = i32::try_from(size.width).unwrap_or(0);
        let h = i32::try_from(size.height).unwrap_or(0);
        let columns = i32::try_from(PILES).unwrap_or(7);

        let bar = Rect::new(0, 0, w, u * 3);
        let button_h = u * 2;
        let by = (bar.height - button_h) / 2;
        let mut x = u;
        let mut place = |width: i32| {
            let r = Rect::new(x, by, width, button_h);
            x += width + u / 2;
            r
        };
        let buttons = [
            (Button::New, place(u * 7)),
            (Button::Undo, place(u * 5)),
            (Button::Turn, place(u * 8)),
        ];
        let status = Rect::new(x + u / 2, by, (w - x - u * 3 / 2).max(0), button_h);

        // Seven across, and two rows with room for a fan under the second.
        let gap = (w / 70).max(u / 2);
        let by_width = (w - gap * (columns + 1)) / columns;
        let tallest = (h - bar.height - gap * 3) * 10 / 32;
        let by_height = tallest * ASPECT.0 / ASPECT.1;
        let card_w = by_width.min(by_height).max(24);
        let card_h = card_w * ASPECT.1 / ASPECT.0;
        let across = columns * card_w + (columns - 1) * gap;
        let top = bar.bottom() + gap;

        Self {
            metrics: m,
            size,
            bar,
            buttons,
            status,
            card: (card_w, card_h),
            gap,
            left: (w - across) / 2,
            top,
            piles: top + card_h + gap,
            down: (card_h * 8 / 100).max(2),
            up: (card_h * 20 / 100).max(4),
            radius: (card_w * 5 / 100).max(2),
        }
    }

    fn column(&self, index: usize) -> i32 {
        self.left + i32::try_from(index).unwrap_or(0) * (self.card.0 + self.gap)
    }

    /// Where a pile's first card lies.
    #[must_use]
    pub fn slot(&self, place: Place) -> Rect {
        let (x, y) = match place {
            Place::Stock => (self.column(0), self.top),
            Place::Waste => (self.column(1), self.top),
            Place::Foundation(i) => (self.column(3 + i.min(3)), self.top),
            Place::Tableau(i) => (self.column(i.min(PILES - 1)), self.piles),
        };
        Rect::new(x, y, self.card.0, self.card.1)
    }

    /// How far each face-down and each face-up card of `pile` shows above
    /// the next: less than usual when the pile would leave the window.
    #[must_use]
    pub fn steps(&self, pile: &Pile) -> (i32, i32) {
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let room =
            i32::try_from(self.size.height).unwrap_or(0) - self.gap - self.piles - self.card.1;
        let need =
            count(pile.down.len()) * self.down + count(pile.up.len().saturating_sub(1)) * self.up;
        if need <= room || need == 0 {
            return (self.down, self.up);
        }
        let room = room.max(0);
        (
            (self.down * room / need).max(1),
            (self.up * room / need).max(2),
        )
    }

    /// Where the card `index` cards from the bottom of pile `pile` lies.
    #[must_use]
    pub fn pile_card(&self, game: &Game, pile: usize, index: usize) -> Rect {
        let slot = self.slot(Place::Tableau(pile));
        let Some(cards) = game.tableau().get(pile) else {
            return slot;
        };
        let (down, up) = self.steps(cards);
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let hidden = index.min(cards.down.len());
        let shown = index - hidden;
        Rect::new(
            slot.x,
            slot.y + count(hidden) * down + count(shown) * up,
            slot.width,
            slot.height,
        )
    }

    /// How many of the waste's cards are fanned out: three when the stock
    /// turns three.
    fn fanned(game: &Game, waste: usize) -> usize {
        waste.min(if game.turn() == 3 { 3 } else { 1 })
    }

    /// Where the `k`th of `shown` fanned waste cards lies.
    fn waste_card(&self, k: usize) -> Rect {
        let slot = self.slot(Place::Waste);
        let step = self.card.0 * 22 / 100;
        Rect::new(
            slot.x + i32::try_from(k).unwrap_or(0) * step,
            slot.y,
            slot.width,
            slot.height,
        )
    }

    /// Where the first of the top `count` cards of `place` lies: where a
    /// hand takes them from, and where cards on their way there end up.
    #[must_use]
    pub fn top(&self, game: &Game, place: Place, count: usize) -> Rect {
        match place {
            Place::Stock | Place::Foundation(_) => self.slot(place),
            Place::Waste => {
                let shown = Self::fanned(game, game.waste().len());
                self.waste_card(shown.saturating_sub(1))
            }
            Place::Tableau(i) => {
                let Some(pile) = game.tableau().get(i) else {
                    return self.slot(place);
                };
                let all = pile.down.len() + pile.up.len();
                self.pile_card(game, i, all.saturating_sub(count.max(1)))
            }
        }
    }

    /// What is under `at`.
    #[must_use]
    pub fn hit(&self, game: &Game, at: Point) -> Option<Hit> {
        if let Some((button, _)) = self.buttons.iter().find(|(_, r)| r.contains(at)) {
            return Some(Hit::Button(*button));
        }
        if self.slot(Place::Stock).contains(at) {
            return Some(Hit::Stock);
        }
        let shown = Self::fanned(game, game.waste().len());
        if shown > 0 && self.waste_card(shown - 1).contains(at) {
            return Some(Hit::Waste);
        }
        if let Some(i) = (0..4).find(|&i| self.slot(Place::Foundation(i)).contains(at)) {
            return Some(Hit::Foundation(i));
        }
        for (i, pile) in game.tableau().iter().enumerate() {
            let all = pile.down.len() + pile.up.len();
            if all == 0 {
                if self.slot(Place::Tableau(i)).contains(at) {
                    return Some(Hit::Pile { pile: i, count: 0 });
                }
                continue;
            }
            // From the top card down: the one seen is the one pointed at.
            if let Some(index) = (0..all)
                .rev()
                .find(|&n| self.pile_card(game, i, n).contains(at))
            {
                let count = if index < pile.down.len() {
                    0
                } else {
                    all - index
                };
                return Some(Hit::Pile { pile: i, count });
            }
        }
        None
    }

    /// The pile a card let go of at `at` is meant for: the column it is
    /// over, in the row it is in.
    #[must_use]
    pub fn target(&self, at: Point) -> Option<Place> {
        let reach = self.gap / 2;
        let over = |index: usize| {
            let x = self.column(index);
            at.x >= x - reach && at.x < x + self.card.0 + reach
        };
        if at.y < self.piles - reach {
            return (0..4)
                .find(|&i| over(3 + i) && at.y >= self.top - reach)
                .map(Place::Foundation);
        }
        (0..PILES).find(|&i| over(i)).map(Place::Tableau)
    }
}

/// What the bar says of a game.
#[must_use]
pub fn status(game: &Game) -> String {
    if game.won() {
        return format!("Out in {} moves", game.moves());
    }
    match game.moves() {
        0 => String::new(),
        1 => "1 move".into(),
        n => format!("{n} moves"),
    }
}

/// What a button says.
#[must_use]
pub fn label(button: Button, game: &Game) -> &'static str {
    match button {
        Button::New => "New game",
        Button::Undo => "Undo",
        Button::Turn if game.turn() == 3 => "Turn three",
        Button::Turn => "Turn one",
    }
}

/// The edge of a card: what keeps one white card apart from the next.
const EDGE: Color = Color::rgba(0x10, 0x18, 0x20, 0x90);
/// A card's paper.
const PAPER: Color = Color::rgb(0xFF, 0xFF, 0xFF);
/// Under a card in the hand.
const SHADOW: Color = Color::rgba(0, 0, 0, 0x50);

struct Brush<'a> {
    layout: &'a Layout,
    faces: &'a mut Faces,
    back: Color,
    trim: Color,
    line: i32,
}

impl Brush<'_> {
    fn face(&mut self, pen: &mut Pen<'_>, card: Card, r: Rect) {
        let (w, h) = (
            u32::try_from(r.width).unwrap_or(1),
            u32::try_from(r.height).unwrap_or(1),
        );
        let drawn = self.faces.get(card, w, h).is_some_and(|face| {
            PixelView::new(&face.pixels, Size::new(face.width, face.height), face.width)
                .is_some_and(|view| {
                    pen.blit_rounded(&view, r, r, self.layout.radius);
                    true
                })
        });
        if !drawn {
            pen.fill_rounded_rect(r, self.layout.radius, PAPER);
        }
        pen.stroke_rounded_rect(r, self.layout.radius, self.line, EDGE);
    }

    fn back(&self, pen: &mut Pen<'_>, r: Rect) {
        let radius = self.layout.radius;
        pen.fill_rounded_rect(r, radius, PAPER);
        let rim = (r.width * 5 / 100).max(2);
        let inner = r.inflate(-rim);
        pen.fill_rounded_rect(inner, radius / 2, self.back);
        pen.stroke_rounded_rect(inner.inflate(-rim), radius / 2, self.line, self.trim);
        pen.stroke_rounded_rect(r, radius, self.line, EDGE);
    }
}

/// Paint the table into `frame`.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)] // one table, top to bottom
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    faces: &mut Faces,
    game: &Game,
    scene: &Scene,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let m = &layout.metrics;
    let ink = Ink::new(appearance);
    let st = fonts.styles(m);
    let engine = &mut fonts.engine;
    pen.clear(ink.background);

    // The bar.
    pen.fill_rect(layout.bar, ink.card);
    for (button, rect) in layout.buttons {
        let hovered = scene.hover == Some(button);
        let dead = button == Button::Undo && !game.can_undo();
        if dead {
            draw::button(
                &mut pen,
                engine,
                st.text,
                rect,
                label(button, game),
                (None, ink.dim),
                Some((m.px(1), ink.border)),
            );
        } else {
            draw::outline_button(
                &mut pen,
                engine,
                st.text,
                rect,
                label(button, game),
                hovered,
                m,
                &ink,
            );
        }
    }
    let said = status(game);
    let colour = if game.won() { ink.accent } else { ink.dim };
    draw::right_label(&mut pen, engine, st.text, layout.status, &said, colour);

    let mut brush = Brush {
        layout,
        faces,
        back: draw::mix(appearance.accent, appearance.background, 45),
        trim: ink.accent,
        line: m.px(1),
    };
    let radius = layout.radius;
    let hollow = |pen: &mut Pen<'_>, r: Rect| {
        pen.stroke_rounded_rect(r, radius, m.px(2), ink.border);
    };
    // How many of a pile's top cards are in the hand and not on it.
    let gone = |place: Place| match scene.lifted {
        Some(l) if l.place == place => l.count,
        _ => 0,
    };

    // The stock: a back while there is one to turn, a ring to turn it over
    // again, and nothing when it is done.
    let stock = layout.slot(Place::Stock);
    hollow(&mut pen, stock);
    if !game.stock().is_empty() {
        brush.back(&mut pen, stock);
    } else if !game.waste().is_empty() {
        let c = Point::new(stock.x + stock.width / 2, stock.y + stock.height / 2);
        pen.stroke_circle(c, stock.width / 5, m.px(3), ink.dim);
    }

    // The waste, its last cards fanned.
    hollow(&mut pen, layout.slot(Place::Waste));
    let waste = game.waste();
    let lying = waste.len().saturating_sub(gone(Place::Waste));
    let shown = Layout::fanned(game, lying);
    for (k, card) in waste[lying - shown..lying].iter().enumerate() {
        brush.face(&mut pen, *card, layout.waste_card(k));
    }

    // The foundations.
    for i in 0..4 {
        let place = Place::Foundation(i);
        let slot = layout.slot(place);
        let cards = game.foundation(i);
        let lying = cards.len().saturating_sub(gone(place));
        if let Some(card) = lying.checked_sub(1).and_then(|n| cards.get(n)) {
            brush.face(&mut pen, *card, slot);
        } else {
            hollow(&mut pen, slot);
            draw::centred(&mut pen, engine, st.large, slot, "A", ink.border);
        }
    }

    // The seven piles.
    for (i, pile) in game.tableau().iter().enumerate() {
        let place = Place::Tableau(i);
        let all = pile.down.len() + pile.up.len();
        let lying = all.saturating_sub(gone(place));
        if lying == 0 {
            hollow(&mut pen, layout.slot(place));
        }
        for n in 0..lying {
            let r = layout.pile_card(game, i, n);
            match n.checked_sub(pile.down.len()).and_then(|k| pile.up.get(k)) {
                Some(card) => brush.face(&mut pen, *card, r),
                None => brush.back(&mut pen, r),
            }
        }
    }

    // Where the keyboard is, and what it has picked up.
    let ring = |pen: &mut Pen<'_>, place: Place, count: usize, width: i32| {
        let first = layout.top(game, place, count);
        let last = layout.top(game, place, 1);
        let r = first.union(&last).inflate(m.px(3));
        pen.stroke_rounded_rect(r, radius + m.px(3), width, ink.accent);
    };
    if let Some((place, count)) = scene.chosen {
        ring(&mut pen, place, count, m.px(4));
    }
    if let Some(place) = scene.focus
        && scene.chosen.map(|(p, _)| p) != Some(place)
    {
        ring(&mut pen, place, 1, m.px(2));
    }

    // The cards in the hand, over everything.
    if let Some(lifted) = scene.lifted
        && let Some(cards) = game.held(lifted.place, lifted.count)
    {
        let (w, h) = layout.card;
        for (k, card) in cards.iter().enumerate() {
            let y = lifted.at.y + i32::try_from(k).unwrap_or(0) * layout.up;
            let r = Rect::new(lifted.at.x, y, w, h);
            if k == 0 {
                let tail = i32::try_from(cards.len() - 1).unwrap_or(0) * layout.up;
                let shade = Rect::new(r.x + m.px(4), r.y + m.px(6), w, h + tail);
                pen.fill_rounded_rect(shade, radius, SHADOW);
            }
            brush.face(&mut pen, *card, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Button, Hit, Layout, status};
    use crate::game::{Game, PILES, Place};
    use alpymist_widget::Appearance;
    use denise::geom::{Point, Size};

    fn layout(w: u32, h: u32) -> Layout {
        Layout::new(&Appearance::default(), Size::new(w, h), 1)
    }

    fn centre(r: denise::geom::Rect) -> Point {
        Point::new(r.x + r.width / 2, r.y + r.height / 2)
    }

    #[test]
    fn seven_cards_fit_across_and_two_rows_down_in_any_window() {
        for (w, h) in [
            (640, 480),
            (1000, 720),
            (1920, 1080),
            (900, 1600),
            (2560, 700),
        ] {
            let l = layout(w, h);
            let (cw, ch) = l.card;
            let last = l.slot(Place::Tableau(PILES - 1));
            assert!(
                l.left >= 0 && last.right() <= i32::try_from(w).unwrap(),
                "{w}x{h}"
            );
            assert!(l.piles + ch < i32::try_from(h).unwrap(), "{w}x{h}");
            assert_eq!(ch, cw * 436 / 300);
            // The foundations stand over the last four piles.
            assert_eq!(l.slot(Place::Foundation(3)).x, last.x);
        }
    }

    #[test]
    fn a_tall_pile_is_squeezed_to_stay_in_the_window() {
        let l = layout(1000, 500);
        let mut game = Game::new(2, 1);
        let h = 500;
        for (i, pile) in game.tableau().iter().enumerate() {
            let all = pile.down.len() + pile.up.len();
            assert!(l.pile_card(&game, i, all - 1).bottom() <= h, "pile {i}");
        }
        // Turning the stock does not move a pile.
        let before = l.pile_card(&game, 6, 6);
        game.deal();
        assert_eq!(l.pile_card(&game, 6, 6), before);
    }

    #[test]
    fn what_is_painted_is_what_is_hit() {
        let l = layout(1200, 800);
        let mut game = Game::new(4, 3);
        for (button, rect) in l.buttons {
            assert_eq!(l.hit(&game, centre(rect)), Some(Hit::Button(button)));
        }
        assert_eq!(l.buttons[0].0, Button::New);
        assert_eq!(l.hit(&game, centre(l.slot(Place::Stock))), Some(Hit::Stock));
        assert_eq!(l.hit(&game, centre(l.slot(Place::Waste))), None, "empty");
        game.deal();
        // Three are fanned, and only the last is the top card.
        let top = l.top(&game, Place::Waste, 1);
        assert!(top.x > l.slot(Place::Waste).x);
        assert_eq!(
            l.hit(&game, Point::new(top.right() - 2, top.y + 5)),
            Some(Hit::Waste)
        );
        assert_eq!(
            l.hit(&game, centre(l.slot(Place::Foundation(2)))),
            Some(Hit::Foundation(2))
        );
        // The last pile: six face down, one up.
        let down = l.pile_card(&game, 6, 0);
        assert_eq!(
            l.hit(&game, Point::new(down.x + 5, down.y + 1)),
            Some(Hit::Pile { pile: 6, count: 0 })
        );
        assert_eq!(
            l.hit(&game, centre(l.pile_card(&game, 6, 6))),
            Some(Hit::Pile { pile: 6, count: 1 })
        );
    }

    #[test]
    fn a_card_let_go_belongs_to_the_column_it_is_over() {
        let l = layout(1200, 800);
        for i in 0..PILES {
            let slot = l.slot(Place::Tableau(i));
            assert_eq!(l.target(centre(slot)), Some(Place::Tableau(i)));
            // Far down the window is still that pile.
            assert_eq!(
                l.target(Point::new(slot.x + 3, 790)),
                Some(Place::Tableau(i))
            );
        }
        for i in 0..4 {
            let slot = l.slot(Place::Foundation(i));
            assert_eq!(l.target(centre(slot)), Some(Place::Foundation(i)));
        }
        assert_eq!(l.target(centre(l.slot(Place::Stock))), None);
        assert_eq!(l.target(centre(l.slot(Place::Waste))), None);
        assert_eq!(l.target(Point::new(-50, 600)), None);
    }

    #[test]
    fn the_bar_counts_moves() {
        let mut game = Game::new(1, 1);
        assert_eq!(status(&game), "");
        game.deal();
        assert_eq!(status(&game), "1 move");
        game.deal();
        assert_eq!(status(&game), "2 moves");
    }
}

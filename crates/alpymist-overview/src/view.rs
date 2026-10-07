//! Laying the overview out, and painting it.
//!
//! Each workspace is the screen made small, with the same shape, so a window
//! is where it is on the screen and as large beside its neighbours. The
//! same rectangles answer what is under the pointer.

use crate::model::{Overview, SIDE, SPACES, Space, Window};
use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use alpymist_widget::{Appearance, Colour};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;
use denise_text::TextStyle;

/// How much of what is behind shows through the overview: little.
const SHADE: u8 = 0xE6;

/// What the foot of the screen says.
pub const HINT: &str = "Tab or the arrows move  ·  Enter goes there  ·  1 to 9  ·  Esc closes";

/// Where everything is, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The sizes text and corners are measured in.
    pub metrics: Metrics,
    /// The nine workspaces.
    pub cells: Vec<Rect>,
    /// The line of hints under them.
    pub hint: Rect,
    /// The screen's logical size, which the windows' places are in.
    screen: (i32, i32),
}

impl Layout {
    /// Lay `overview` out on a surface of `size` at an output scale.
    #[must_use]
    pub fn new(appearance: &Appearance, overview: &Overview, size: Size, scale: u32) -> Self {
        let metrics = Metrics::new(appearance, scale, 0);
        let (w, h) = (
            i32::try_from(size.width).unwrap_or(0),
            i32::try_from(size.height).unwrap_or(0),
        );
        let (sw, sh) = (overview.size.0.max(1), overview.size.1.max(1));
        let across = i32::try_from(SIDE).unwrap_or(3);
        let margin = (h / 14).max(metrics.unit * 2);
        let gap = metrics.unit * 5 / 4;
        let hint_h = metrics.unit * 2;
        let (free_w, free_h) = (w - 2 * margin, h - 2 * margin - hint_h);
        let gaps = (across - 1) * gap;
        // As wide as fits, unless that is too tall: then as tall as fits.
        let mut cw = ((free_w - gaps) / across).max(1);
        let mut ch = (cw * sh / sw).max(1);
        if across * ch + gaps > free_h {
            ch = ((free_h - gaps) / across).max(1);
            cw = (ch * sw / sh).max(1);
        }
        let x0 = (w - (across * cw + gaps)) / 2;
        let y0 = margin + (free_h - (across * ch + gaps)) / 2;
        let cells = (0..SPACES)
            .map(|i| {
                let (col, row) = (
                    i32::try_from(i % SIDE).unwrap_or(0),
                    i32::try_from(i / SIDE).unwrap_or(0),
                );
                Rect::new(x0 + col * (cw + gap), y0 + row * (ch + gap), cw, ch)
            })
            .collect();
        Self {
            metrics,
            cells,
            hint: Rect::new(0, y0 + across * ch + gaps + metrics.unit / 2, w, hint_h),
            screen: (sw, sh),
        }
    }

    /// The workspace at `at`, if one is.
    #[must_use]
    pub fn cell_at(&self, at: Point) -> Option<usize> {
        self.cells.iter().position(|c| {
            at.x >= c.x && at.x < c.x + c.width && at.y >= c.y && at.y < c.y + c.height
        })
    }

    /// Where `window` is drawn in the workspace at `cell`.
    #[must_use]
    pub fn window(&self, cell: usize, window: &Window) -> Rect {
        let c = self.cells[cell];
        let (sw, sh) = self.screen;
        let x = c.x + window.x * c.width / sw;
        let y = c.y + window.y * c.height / sh;
        // The far edge worked out and not the size, so two windows that
        // touch on the screen touch here, with no pixel lost between them.
        let right = c.x + (window.x + window.width) * c.width / sw;
        let bottom = c.y + (window.y + window.height) * c.height / sh;
        Rect::new(x, y, (right - x).max(1), (bottom - y).max(1))
    }
}

/// Whether two rectangles share any pixel.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// The place between `from` and `to` after `permille` of the way, fast at
/// first and settling at the end: where the mark round the chosen workspace
/// is while it moves.
#[must_use]
pub fn between(from: Rect, to: Rect, permille: i32) -> Rect {
    let p = i64::from(permille.clamp(0, 1000));
    // 1 - (1 - t)^3, in thousandths.
    let left = 1000 - p;
    let eased = 1000 - left * left * left / 1_000_000;
    let mix = |a: i32, b: i32| {
        let v = i64::from(a) + (i64::from(b) - i64::from(a)) * eased / 1000;
        i32::try_from(v).unwrap_or(b)
    };
    Rect::new(
        mix(from.x, to.x),
        mix(from.y, to.y),
        mix(from.width, to.width),
        mix(from.height, to.height),
    )
}

/// Paint the windows of `space`, which is the workspace at `index`, into
/// its cell: an outline each, with the program's name where there is room
/// for it and the title under it where there is room for both.
fn windows(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    layout: &Layout,
    ink: &Ink,
    (index, space): (usize, &Space),
    on_screen: bool,
) {
    let metrics = layout.metrics;
    let styles = fonts.styles(&metrics);
    let engine = &mut fonts.engine;
    let line = engine.line_height(styles.small);
    let mut inside = pen.with_clip(layout.cells[index]);
    let places: Vec<Rect> = space
        .windows
        .iter()
        .map(|w| layout.window(index, w).inflate(-metrics.px(1)))
        .collect();
    for (n, window) in space.windows.iter().enumerate() {
        let at = places[n];
        if at.width < metrics.px(3) || at.height < metrics.px(3) {
            continue;
        }
        let lit = window.focused && on_screen;
        let edge = if lit { ink.accent } else { ink.dim };
        inside.fill_rounded_rect(at, metrics.px(4), ink.background);
        inside.stroke_rounded_rect(at, metrics.px(4), metrics.px(1), edge);
        let room = at.inflate(-metrics.px(4));
        if room.height < line || room.width < metrics.unit * 2 {
            continue;
        }
        let both = room.height >= 2 * line && !window.title.is_empty();
        let tall = if both { 2 * line } else { line };
        let middle = room.y + (room.height - tall) / 2;
        // In the middle, unless a window over this one covers the middle:
        // then at the top, where a title bar would be.
        let covered = places[n + 1..]
            .iter()
            .any(|over| overlap(*over, Rect::new(room.x, middle, room.width, tall)));
        let top = if covered { room.y } else { middle };
        let mut clipped = inside.with_clip(room);
        let name = TextStyle {
            font: if lit {
                styles.strong.font
            } else {
                styles.text.font
            },
            size_px: styles.small.size_px,
        };
        let mut rows = vec![(name, window.name.as_str(), ink.text, top)];
        if both {
            rows.push((styles.small, window.title.as_str(), ink.dim, top + line));
        }
        for (style, text, colour, row_top) in rows {
            let row = Rect::new(room.x, row_top, room.width, line);
            // Centred when it fits; from the left, and cut off, when not.
            if engine.measure_line(style, text) <= room.width {
                draw::centred(&mut clipped, engine, style, row, text, colour);
            } else {
                draw::label(&mut clipped, engine, style, row, text, colour);
            }
        }
    }
}

/// Paint `overview` over the whole of `frame`, with the mark round the
/// chosen workspace at `mark`, or round the chosen one itself. Returns the
/// layout it was painted with, for the pointer.
pub fn paint(
    frame: &mut Frame<'_>,
    appearance: &Appearance,
    fonts: &mut Fonts,
    overview: &Overview,
    scale: u32,
    mark: Option<Rect>,
) -> Layout {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let size = pen.size();
    let layout = Layout::new(appearance, overview, size, scale);
    let metrics = layout.metrics;
    let ink = Ink::new(appearance);
    let styles = fonts.styles(&metrics);

    let Colour([red, green, blue, _]) = appearance.background;
    pen.clear(denise::Color::rgba(0, 0, 0, 0));
    pen.fill_rect(
        Rect::from_size(size),
        draw::colour(Colour([red, green, blue, SHADE])),
    );

    let radius = metrics.px(8);
    for (i, (space, &cell)) in overview.spaces.iter().zip(&layout.cells).enumerate() {
        let current = overview.current == Some(i);
        let hovered = overview.hovered == Some(i);
        let fill = if hovered { ink.selection } else { ink.card };
        pen.fill_rounded_rect(cell, radius, fill);
        windows(&mut pen, fonts, &layout, &ink, (i, space), current);

        // Its number, in the corner over whatever is there: the one on
        // screen now is the one lit.
        let badge = Rect::new(
            cell.x + metrics.px(6),
            cell.y + metrics.px(6),
            metrics.unit * 3 / 2,
            metrics.unit * 3 / 2,
        );
        let (back, text) = if current {
            (ink.accent, ink.on_accent)
        } else if space.windows.is_empty() {
            (ink.background, ink.dim)
        } else {
            (ink.background, ink.text)
        };
        pen.fill_rounded_rect(badge, badge.height / 2, back);
        let number = (i + 1).to_string();
        draw::centred(
            &mut pen,
            &mut fonts.engine,
            styles.strong,
            badge,
            &number,
            text,
        );
        if current {
            pen.stroke_rounded_rect(cell, radius, metrics.px(2), ink.accent);
        }
    }

    // The mark round the chosen one, last, so it is over its neighbours
    // while it moves between them.
    let mark = mark.or_else(|| layout.cells.get(overview.chosen).copied());
    if let Some(mark) = mark {
        pen.stroke_rounded_rect(
            mark.inflate(metrics.px(4)),
            radius + metrics.px(4),
            metrics.px(3),
            ink.text,
        );
    }

    let engine = &mut fonts.engine;
    draw::centred(&mut pen, engine, styles.small, layout.hint, HINT, ink.dim);
    layout
}

#[cfg(test)]
mod tests {
    use super::{Layout, between};
    use crate::model::{Overview, SPACES};
    use alpymist_widget::Appearance;
    use denise::geom::{Point, Rect, Size};

    fn layout(w: u32, h: u32) -> Layout {
        Layout::new(
            &Appearance::default(),
            &Overview::sample(),
            Size::new(w, h),
            1,
        )
    }

    #[test]
    fn nine_cells_shaped_as_the_screen_inside_the_surface_and_apart() {
        for (w, h) in [(1280, 800), (1920, 1080), (800, 1280), (3440, 1440)] {
            let l = layout(w, h);
            assert_eq!(l.cells.len(), SPACES);
            let (w, h) = (i32::try_from(w).unwrap(), i32::try_from(h).unwrap());
            for (i, c) in l.cells.iter().enumerate() {
                assert!(c.x >= 0 && c.y >= 0, "{w}x{h} cell {i}");
                assert!(
                    c.x + c.width <= w && c.y + c.height <= h,
                    "{w}x{h} cell {i}"
                );
                // The sample's screen is 1280 by 800: 16 to 10, give or take
                // the pixel a division drops.
                assert!(
                    (c.width * 10 - c.height * 16).abs() <= 16,
                    "{w}x{h} cell {i}"
                );
                for d in &l.cells[i + 1..] {
                    let apart = c.x + c.width <= d.x
                        || d.x + d.width <= c.x
                        || c.y + c.height <= d.y
                        || d.y + d.height <= c.y;
                    assert!(apart, "{w}x{h}: cells overlap");
                }
            }
            assert!(l.hint.y >= l.cells[8].y + l.cells[8].height);
            assert!(l.hint.y + l.hint.height <= h);
        }
    }

    #[test]
    fn the_pointer_finds_its_cell_and_the_gaps_are_no_cell() {
        let l = layout(1280, 800);
        for (i, c) in l.cells.iter().enumerate() {
            let middle = Point::new(c.x + c.width / 2, c.y + c.height / 2);
            assert_eq!(l.cell_at(middle), Some(i));
        }
        let (a, b) = (l.cells[0], l.cells[1]);
        let between = Point::new((a.x + a.width + b.x) / 2, a.y + 5);
        assert_eq!(l.cell_at(between), None);
        assert_eq!(l.cell_at(Point::new(0, 0)), None);
    }

    #[test]
    fn windows_that_touch_on_the_screen_touch_in_the_cell_and_stay_in_it() {
        let o = Overview::sample();
        let l = layout(1280, 800);
        let cell = l.cells[0];
        let rects: Vec<Rect> = o.spaces[0].windows.iter().map(|w| l.window(0, w)).collect();
        for r in &rects {
            assert!(r.x >= cell.x && r.y >= cell.y);
            assert!(r.x + r.width <= cell.x + cell.width);
            assert!(r.y + r.height <= cell.y + cell.height);
        }
        // Left of the two on the right, by the screen's own gap and no more.
        assert!(rects[0].x + rects[0].width <= rects[1].x);
        assert!(rects[1].x - (rects[0].x + rects[0].width) <= 4);
        assert_eq!(rects[1].x, rects[2].x);
    }

    #[test]
    fn the_mark_leaves_fast_arrives_gently_and_ends_where_it_was_going() {
        let (a, b) = (Rect::new(0, 0, 100, 60), Rect::new(300, 200, 100, 60));
        assert_eq!(between(a, b, 0), a);
        assert_eq!(between(a, b, 1000), b);
        assert_eq!(between(a, b, 5000), b, "late is there");
        let half = between(a, b, 500);
        assert!(half.x > 150, "more than half way at half time: {}", half.x);
        let (early, late) = (between(a, b, 250).x, between(a, b, 750).x);
        assert!(early < half.x && half.x < late && late < b.x);
        assert!(
            early > late - half.x,
            "the first quarter covers more than the third"
        );
    }
}

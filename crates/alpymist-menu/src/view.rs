//! Painting the menu.
//!
//! One panel: a search line with the breadcrumbs on its right, a rule, the
//! rows, and a footer that says how to get around — or, when the user's
//! configuration did not parse, says that instead, because a menu that
//! silently ignores an edit is a menu people stop trusting.
//!
//! Everything is laid out from the font size, so `font_size = 20` scales the
//! whole panel rather than crowding bigger text into the same rows, and every
//! length is multiplied by the output scale so a high-density panel is drawn sharp at
//! its own resolution rather than stretched by the compositor.

use crate::config::{Appearance, Colour};
use crate::font::LazyFont;
use crate::menu::Menu;
use crate::tree::Action;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise::{Color, Frame};
use denise_render::Canvas;
use denise_text::{FontId, TextEngine, TextStyle};

/// The fonts, loaded once.
pub struct Fonts {
    engine: TextEngine,
    text: FontId,
    icons: FontId,
    /// Paths that could not be loaded, for the log.
    pub problems: Vec<String>,
}

impl Fonts {
    /// Load the configured fonts. Never fails: a missing face falls back to
    /// Denise's built-in bitmap, which still draws every name legibly.
    #[must_use]
    pub fn load(appearance: &Appearance) -> Self {
        let mut engine = TextEngine::new();
        let mut problems = Vec::new();
        let built_in = TextStyle::built_in(0).font;
        let mut add = |engine: &mut TextEngine, path: &str| match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| LazyFont::from_vec(path, bytes))
        {
            Ok(source) => engine.add_font(Box::new(source)),
            Err(e) => {
                problems.push(format!("{path}: {e}"));
                built_in
            }
        };
        let text = add(&mut engine, &appearance.font);
        let icons = add(&mut engine, &appearance.icon_font);
        engine.set_default_font(text);
        Self {
            engine,
            text,
            icons,
            problems,
        }
    }
}

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Output scale.
    pub scale: u32,
    /// The whole surface.
    pub size: Size,
    /// Padding inside the border.
    pub pad: i32,
    /// The search line.
    pub search: Rect,
    /// The first row; later rows follow at `row.height` intervals.
    pub row: Rect,
    /// How many rows there are room for.
    pub rows: u32,
    /// The footer line.
    pub footer: Rect,
    /// Text heights.
    pub text_px: u16,
    /// Footer and breadcrumb text height.
    pub small_px: u16,
    /// Corner radius.
    pub radius: i32,
    /// Border thickness.
    pub border: i32,
}

impl Layout {
    /// The layout for an appearance at an output scale.
    #[must_use]
    pub fn new(appearance: &Appearance, scale: u32) -> Self {
        let scale = scale.max(1);
        let s = i32::try_from(scale).unwrap_or(1);
        let font = i32::from(appearance.font_size.clamp(8, 64));
        let px = |logical: i32| logical * s;
        let text_px = u16::try_from(px(font)).unwrap_or(u16::MAX);
        let small_px = u16::try_from(px(font * 13 / 16)).unwrap_or(u16::MAX);

        let border = px(2);
        let pad = px(font * 3 / 4);
        let width = px(i32::try_from(appearance.width.clamp(240, 4000)).unwrap_or(560));
        let rows = appearance.rows.clamp(1, 40);
        let search_h = px(font * 5 / 2);
        let row_h = px(font * 9 / 4);
        let footer_h = px(font * 2);
        let inner = width - 2 * (border + pad);

        let search = Rect::new(border + pad, border + pad / 2, inner, search_h);
        let row = Rect::new(
            border + pad / 2,
            search.bottom() + px(6),
            width - 2 * border - pad,
            row_h,
        );
        let list_h = row_h * i32::try_from(rows).unwrap_or(1);
        let footer = Rect::new(border + pad, row.y + list_h + px(4), inner, footer_h);
        let height = footer.bottom() + border + pad / 4;

        Self {
            scale,
            size: Size::new(
                u32::try_from(width).unwrap_or(0),
                u32::try_from(height).unwrap_or(0),
            ),
            pad,
            search,
            row,
            rows,
            footer,
            text_px,
            small_px,
            radius: px(10),
            border,
        }
    }

    /// The surface size in logical pixels, which is what the compositor is
    /// asked for.
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        (self.size.width / self.scale, self.size.height / self.scale)
    }

    /// Which visible row, counted from the top, is under `point`.
    #[must_use]
    pub fn row_at(&self, point: Point) -> Option<usize> {
        if point.x < self.row.x || point.x >= self.row.right() || point.y < self.row.y {
            return None;
        }
        let offset = (point.y - self.row.y) / self.row.height;
        let offset = u32::try_from(offset).ok()?;
        (offset < self.rows).then_some(offset as usize)
    }

    fn row_rect(&self, offset: usize) -> Rect {
        let offset = i32::try_from(offset).unwrap_or(0);
        Rect::new(
            self.row.x,
            self.row.y + offset * self.row.height,
            self.row.width,
            self.row.height,
        )
    }

    fn px(&self, logical: i32) -> i32 {
        logical * i32::try_from(self.scale).unwrap_or(1)
    }
}

fn colour(Colour([red, green, blue, alpha]): Colour) -> Color {
    Color::rgba(red, green, blue, alpha)
}

/// Where text of `size` sits to be centred vertically in a box at `y` of `h`.
fn text_top(engine: &TextEngine, style: TextStyle, y: i32, h: i32) -> i32 {
    y + (h - engine.line_height(style)) / 2
}

/// Draw `text` with the characters at `positions` in `highlight`, returning
/// the width drawn. Runs of one colour are drawn whole, so a highlighted name
/// costs a handful of calls, not one per letter.
fn draw_highlighted(
    engine: &mut TextEngine,
    pen: &mut Pen<'_>,
    style: TextStyle,
    at: Point,
    text: &str,
    positions: &[usize],
    (normal, highlight): (Color, Color),
) -> i32 {
    if positions.is_empty() {
        return i32::try_from(engine.draw(pen, style, at, text, normal).width).unwrap_or(0);
    }
    let mut x = at.x;
    let mut run_start = 0;
    let mut run_lit = positions.first() == Some(&0);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for i in 0..=chars.len() {
        let lit = positions.binary_search(&i).is_ok();
        if i == chars.len() || lit != run_lit {
            let from = chars.get(run_start).map_or(text.len(), |c| c.0);
            let to = chars.get(i).map_or(text.len(), |c| c.0);
            let run = &text[from..to];
            if !run.is_empty() {
                let ink = if run_lit { highlight } else { normal };
                engine.draw(pen, style, Point::new(x, at.y), run, ink);
                x += engine.measure_line(style, run);
            }
            run_start = i;
            run_lit = lit;
        }
    }
    x - at.x
}

/// Paint the whole menu.
#[allow(clippy::too_many_lines)]
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    menu: &Menu,
    notice: Option<&str>,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = canvas.pen();
    let full = Rect::from_size(layout.size);
    let (text, dim, accent) = (
        colour(appearance.text),
        colour(appearance.dim),
        colour(appearance.accent),
    );

    // The panel. Cleared to transparent first, so the rounded corners show
    // what is behind them rather than a square of black.
    pen.clear(Color::rgba(0, 0, 0, 0));
    pen.fill_rounded_rect(full, layout.radius, colour(appearance.background));
    pen.stroke_rounded_rect(
        full,
        layout.radius,
        layout.border,
        colour(appearance.border),
    );

    let Fonts {
        engine,
        text: text_font,
        icons,
        ..
    } = fonts;
    let style = TextStyle {
        font: *text_font,
        size_px: layout.text_px,
    };
    let small = TextStyle {
        font: *text_font,
        size_px: layout.small_px,
    };
    let icon_style = TextStyle {
        font: *icons,
        size_px: layout.text_px,
    };
    let icon_w = layout.px(i32::from(appearance.font_size.clamp(8, 64)) * 2);

    // Search line: a magnifier, the query or a placeholder, and a caret.
    let field = layout.search;
    let top = text_top(engine, icon_style, field.y, field.height);
    engine.draw(
        &mut pen,
        icon_style,
        Point::new(field.x, top),
        "\u{f002}",
        accent,
    );
    let qx = field.x + icon_w * 3 / 4;
    let top = text_top(engine, style, field.y, field.height);

    let crumbs = menu.breadcrumbs().join(" › ");
    let crumb_w = engine.measure_line(small, &crumbs);
    let crumb_x = field.right() - crumb_w;
    let query_room = Rect::new(
        qx,
        field.y,
        (crumb_x - layout.pad - qx).max(0),
        field.height,
    );
    {
        let mut clip = pen.with_clip(query_room);
        if menu.query().is_empty() {
            let caret = Rect::new(
                qx,
                top + layout.px(2),
                layout.px(2),
                engine.line_height(style) - layout.px(4),
            );
            clip.fill_rect(caret, accent);
            engine.draw(
                &mut clip,
                style,
                Point::new(qx + layout.px(6), top),
                "Search",
                dim,
            );
        } else {
            // Long queries scroll left so the end, where typing happens, stays in view.
            let w = engine.measure_line(style, menu.query());
            let x = qx.min(query_room.right() - w - layout.px(4));
            engine.draw(&mut clip, style, Point::new(x, top), menu.query(), text);
            let caret = Rect::new(
                x + w + layout.px(1),
                top + layout.px(2),
                layout.px(2),
                engine.line_height(style) - layout.px(4),
            );
            clip.fill_rect(caret, accent);
        }
    }
    let ctop = text_top(engine, small, field.y, field.height);
    engine.draw(&mut pen, small, Point::new(crumb_x, ctop), &crumbs, dim);

    let rule_y = field.bottom() + layout.px(2);
    pen.fill_rect(
        Rect::new(field.x, rule_y, field.width, layout.px(1).max(1)),
        colour(appearance.selection),
    );

    // Rows.
    let rows = menu.rows();
    let end = (menu.scroll() + menu.visible()).min(rows.len());
    for (offset, index) in (menu.scroll()..end).enumerate() {
        let row = &rows[index];
        let entry = &menu.tree.entries[row.entry];
        let cell = layout.row_rect(offset);
        let selected = index == menu.selected();
        if selected {
            pen.fill_rounded_rect(cell, layout.px(6), colour(appearance.selection));
        }
        let mut clip = pen.with_clip(cell);
        let x = cell.x + layout.pad / 2;
        if !entry.icon.is_empty() {
            let top = text_top(engine, icon_style, cell.y, cell.height);
            let w = engine.measure_line(icon_style, &entry.icon);
            let ix = x + (icon_w - w) / 2 - layout.px(4);
            engine.draw(
                &mut clip,
                icon_style,
                Point::new(ix, top),
                &entry.icon,
                accent,
            );
        }
        let name_x = x + icon_w;
        let top = text_top(engine, style, cell.y, cell.height);

        // The right edge: a chevron for a submenu, the trail for a search hit
        // from deeper down. Measured first, so the name knows where to stop.
        let mut right = cell.right() - layout.pad / 2;
        if matches!(entry.action, Action::Open(_)) {
            let w = engine.measure_line(icon_style, "\u{f054}");
            right -= w;
            let t = text_top(engine, icon_style, cell.y, cell.height);
            let chevron = TextStyle {
                size_px: layout.small_px,
                ..icon_style
            };
            engine.draw(
                &mut clip,
                chevron,
                Point::new(right, t + layout.px(1)),
                "\u{f054}",
                dim,
            );
            right -= layout.pad / 2;
        }
        if !row.trail.is_empty() {
            let w = engine.measure_line(small, &row.trail);
            right -= w;
            let t = text_top(engine, small, cell.y, cell.height);
            engine.draw(&mut clip, small, Point::new(right, t), &row.trail, dim);
            right -= layout.pad;
        }

        let mut name_clip = clip.with_clip(Rect::new(
            name_x,
            cell.y,
            (right - name_x).max(0),
            cell.height,
        ));
        let w = draw_highlighted(
            engine,
            &mut name_clip,
            style,
            Point::new(name_x, top),
            &entry.name,
            &row.positions,
            (text, accent),
        );
        if let Some(detail) = &entry.detail {
            let t = text_top(engine, small, cell.y, cell.height);
            engine.draw(
                &mut name_clip,
                small,
                Point::new(name_x + w + layout.pad * 2 / 3, t + layout.px(1)),
                detail,
                dim,
            );
        }
    }

    if rows.is_empty() {
        let message = "Nothing matches";
        let w = engine.measure_line(style, message);
        let cell = layout.row_rect(0);
        let top = text_top(engine, style, cell.y, cell.height);
        engine.draw(
            &mut pen,
            style,
            Point::new(cell.x + (cell.width - w) / 2, top),
            message,
            dim,
        );
    }

    // A scrollbar, only when there is something to scroll.
    let total = rows.len();
    let visible = menu.visible();
    if total > visible {
        let track_h = layout.row.height * i32::try_from(visible).unwrap_or(1);
        let track_x = layout.row.right() + (layout.pad / 2 - layout.px(3)) / 2;
        let len = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let thumb_h = (track_h * len(visible) / len(total)).max(layout.px(12));
        let thumb_y =
            layout.row.y + (track_h - thumb_h) * len(menu.scroll()) / len(total - visible);
        pen.fill_rounded_rect(
            Rect::new(track_x, thumb_y, layout.px(3), thumb_h),
            layout.px(2),
            colour(appearance.selection),
        );
    }

    // Footer.
    let foot = layout.footer;
    let top = text_top(engine, small, foot.y, foot.height);
    let mut clip = pen.with_clip(foot);
    if let Some(notice) = notice {
        engine.draw(
            &mut clip,
            small,
            Point::new(foot.x, top),
            notice,
            Color::rgb(0xE8, 0xB0, 0x6A),
        );
    } else {
        let back = if menu.breadcrumbs().len() > 1 {
            "back"
        } else {
            "close"
        };
        let hints = format!("↑↓ move    enter open    esc {back}");
        engine.draw(&mut clip, small, Point::new(foot.x, top), &hints, dim);
        if !menu.query().is_empty() {
            let count = format!("{} of {}", total.min(9999), menu_size(menu));
            let w = engine.measure_line(small, &count);
            engine.draw(
                &mut clip,
                small,
                Point::new(foot.right() - w, top),
                &count,
                dim,
            );
        }
    }
}

/// How many entries a search at this level looks through.
fn menu_size(menu: &Menu) -> usize {
    menu.searchable()
}

#[cfg(test)]
mod tests {
    use super::Layout;
    use crate::config::Appearance;
    use denise::geom::Point;

    #[test]
    fn the_layout_scales_with_the_output() {
        let a = Appearance::default();
        let one = Layout::new(&a, 1);
        let two = Layout::new(&a, 2);
        assert_eq!(two.size.width, one.size.width * 2);
        assert_eq!(two.logical_size(), one.logical_size());
    }

    #[test]
    fn rows_are_hit_where_they_are_drawn() {
        let a = Appearance::default();
        let l = Layout::new(&a, 1);
        let mid = |offset: i32| {
            Point::new(
                l.row.x + 10,
                l.row.y + offset * l.row.height + l.row.height / 2,
            )
        };
        assert_eq!(l.row_at(mid(0)), Some(0));
        assert_eq!(l.row_at(mid(3)), Some(3));
        assert_eq!(l.row_at(mid(i32::try_from(l.rows).unwrap())), None);
        assert_eq!(l.row_at(Point::new(l.row.x + 10, l.search.y)), None);
        assert_eq!(l.row_at(Point::new(-5, l.row.y + 1)), None);
    }

    #[test]
    fn everything_fits_inside_the_surface() {
        let mut a = Appearance::default();
        for (size, rows) in [(8, 1), (16, 9), (40, 20)] {
            a.font_size = size;
            a.rows = rows;
            let l = Layout::new(&a, 1);
            assert!(l.footer.bottom() <= i32::try_from(l.size.height).unwrap());
            assert!(l.row.right() <= i32::try_from(l.size.width).unwrap());
        }
    }
}

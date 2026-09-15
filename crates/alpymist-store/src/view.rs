//! Laying out and painting the store.
//!
//! ```text
//! ┌──────────────┬──────────────────────────────────────────────────┐
//! │ 󰏗 Store      │ 󰍉 Search Flathub and Alpine                       │
//! ├──────────────┼──────────────────────────────────────────────────┤
//! │ 󰀻 Discover   │ New & updated        [All 3,012] [Flathub] [Alpine]│
//! │ 󰗠 Installed  │ ┌──┐ GIMP  3.0.4                  Flathub [Install]│
//! │ 󰚰 Updates  2 │ └──┘ Create images and edit photographs            │
//! │ CATEGORIES   │ ┌──┐ …                                             │
//! │ 󰿎 Audio …    │                                                    │
//! │  ⟳ Refresh   │                                                    │
//! ├──────────────┴──────────────────────────────────────────────────┤
//! │ ◌ Installing GIMP — Installing runtime/org.gnome.Platform   Esc … │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Sizes come from the font size and colours from the menu's appearance,
//! as for every Alpymist widget, and the rectangles painted are the ones
//! [`Layout::hit`] answers clicks from.

// Geometry reads best in the letters it is written in: x, y, w, h, and u for
// the unit everything is measured in.
#![allow(clippy::many_single_char_names)]

use crate::catalog::{At, Entry, State, human_size};
use crate::icons::Icons;
use crate::pictures::{Picture, Pictures};
use crate::store::{Action, Focus, Loading, Store, Target, View};
use alpymist_widget::draw::{self, Fonts, Ink, Metrics, Styles};
use alpymist_widget::{Appearance, Colour};
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise::{Color, Frame};
use denise_render::Canvas;

/// The store's own glyph.
pub const STORE: &str = "\u{f10c1}";
const SEARCH: &str = "\u{f0349}";
const DISCOVER: &str = "\u{f018c}";
const INSTALLED: &str = "\u{f05e1}";
const UPDATES: &str = "\u{f06b0}";
const REFRESH: &str = "\u{f0450}";
const BACK: &str = "\u{f0141}";
const LINK: &str = "\u{f03cc}";
const LOCK: &str = "\u{f033e}";
const WARN: &str = "\u{f05d6}";
const PACKAGE: &str = "\u{f03d7}";

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Output scale.
    pub scale: u32,
    /// The window.
    pub size: Size,
    /// Sizes everything is measured in.
    pub metrics: Metrics,
    /// Whether the sidebar shows icons only.
    pub compact: bool,
    /// The store's name, top left.
    pub title: Rect,
    /// The search field.
    pub search: Rect,
    /// The sidebar.
    pub sidebar: Rect,
    /// Views in the sidebar.
    pub nav: Vec<(View, Rect)>,
    /// Where "Categories" is written.
    pub categories_label: Option<Rect>,
    /// The Refresh button.
    pub refresh: Rect,
    /// Right of the sidebar, between header and footer.
    pub content: Rect,
    /// The view's title.
    pub heading: Rect,
    /// Source filters.
    pub chips: Vec<(Option<u16>, Rect)>,
    /// Update all, in the Updates view.
    pub update_all: Option<Rect>,
    /// Where result rows go.
    pub list: Rect,
    /// A row's height.
    pub row_h: i32,
    /// How many rows fit.
    pub rows: usize,
    /// The status line.
    pub footer: Rect,
    /// An entry's page, when one is open.
    pub detail: Option<DetailLayout>,
}

/// An entry's page.
#[derive(Debug, Clone)]
pub struct DetailLayout {
    /// Back to the list.
    pub back: Rect,
    /// The large icon.
    pub icon: Rect,
    /// Name, developer and badge, beside the icon.
    pub head: Rect,
    /// Buttons, right-aligned under the head.
    pub buttons: Vec<(Action, Rect)>,
    /// The website link.
    pub link: Option<Rect>,
    /// Everything below, scrolled.
    pub body: Rect,
    /// A body line's height.
    pub line_h: i32,
    /// The screenshot, at the top of the body, where there are any.
    pub shot: Option<ShotLayout>,
}

/// The screenshot on an entry's page.
#[derive(Debug, Clone, Copy)]
pub struct ShotLayout {
    /// The picture's frame.
    pub frame: Rect,
    /// The caption and count under it.
    pub caption: Rect,
    /// Back and on, where there is more than one.
    pub prev: Option<Rect>,
    /// On.
    pub next: Option<Rect>,
    /// How far the body's text starts below the body's top.
    pub height: i32,
}

impl Layout {
    /// The layout of `store` in a window `size` physical pixels big.
    #[must_use]
    #[allow(clippy::too_many_lines)] // the window, top to bottom
    pub fn new(
        appearance: &Appearance,
        fonts: &mut Fonts,
        store: &Store,
        size: Size,
        scale: u32,
    ) -> Self {
        let scale = scale.max(1);
        let s = i32::try_from(scale).unwrap_or(1);
        let (w, h) = (
            i32::try_from(size.width).unwrap_or(0),
            i32::try_from(size.height).unwrap_or(0),
        );
        let metrics = Metrics::new(
            appearance,
            scale,
            w / s * 16 / i32::from(appearance.font_size.clamp(8, 64)),
        );
        let styles = fonts.styles(&metrics);
        let u = metrics.unit;
        let pad = metrics.pad;

        let header_h = u * 7 / 2;
        let footer_h = u * 9 / 4;
        let compact = w < u * 50;
        let sidebar_w = if compact {
            u * 7 / 2
        } else {
            (u * 13).min(w / 3)
        };

        let title = Rect::new(pad, 0, sidebar_w - pad, header_h);
        let search = Rect::new(
            sidebar_w + pad,
            (header_h - u * 5 / 2) / 2,
            (w - sidebar_w - 2 * pad).max(u * 8),
            u * 5 / 2,
        );
        let footer = Rect::new(0, h - footer_h, w, footer_h);
        let sidebar = Rect::new(0, header_h, sidebar_w, (h - header_h - footer_h).max(0));

        // The sidebar: three views, then the categories, then Refresh at the
        // foot.
        let nav_h = u * 9 / 4;
        let cat_h = u * 2;
        let inset = pad / 2;
        let item_w = sidebar_w - 2 * inset;
        let mut y = sidebar.y + inset;
        let mut nav = Vec::new();
        for view in [View::Discover, View::Installed, View::Updates] {
            nav.push((view, Rect::new(inset, y, item_w, nav_h)));
            y += nav_h;
        }
        let refresh = Rect::new(inset, sidebar.bottom() - inset - nav_h, item_w, nav_h);
        let categories_label =
            (!compact).then(|| Rect::new(inset + pad / 2, y + u / 2, item_w, u * 3 / 2));
        y += if compact { u / 2 } else { u * 2 };
        for view in View::all().into_iter().skip(3) {
            if y + cat_h > refresh.y - inset {
                break;
            }
            nav.push((view, Rect::new(inset, y, item_w, cat_h)));
            y += cat_h;
        }

        let content = Rect::new(sidebar_w, header_h, w - sidebar_w, sidebar.height);
        let bar_h = u * 3;
        let inner_x = content.x + pad;
        let inner_w = content.width - 2 * pad;
        let heading = Rect::new(inner_x, content.y + u / 4, inner_w, bar_h);

        // Chips from the right: All, then each source.
        let chip_h = u * 7 / 4;
        let chip_y = heading.y + (bar_h - chip_h) / 2;
        let gap = u / 3;
        let mut chips = Vec::new();
        let mut x = heading.right();
        let mut labels: Vec<(Option<u16>, String)> =
            vec![(None, format!("All  {}", count(store.counts.iter().sum())))];
        for (i, src) in store.sources.iter().enumerate() {
            let n = store.counts.get(i).copied().unwrap_or(0);
            labels.push((
                u16::try_from(i).ok(),
                format!("{}  {}", src.label, count(n)),
            ));
        }
        // A source with nothing to show has no chip, unless it is the one
        // chosen: Discover lists applications, which Alpine has none of.
        let nothing = store.counts.iter().all(|&n| n == 0) && store.filter.is_none();
        labels.retain(|(filter, _)| {
            if nothing {
                return false;
            }
            filter.is_none_or(|f| {
                store.filter == Some(f) || store.counts.get(usize::from(f)).is_some_and(|&n| n > 0)
            })
        });
        for (filter, label) in labels.iter().rev() {
            let icon_w = if filter.is_some() {
                fonts.engine.measure_line(styles.icon_small, "\u{f0000}") + u / 3
            } else {
                0
            };
            let cw = fonts.engine.measure_line(styles.small, label) + icon_w + u * 3 / 2;
            x -= cw;
            chips.push((*filter, Rect::new(x, chip_y, cw, chip_h)));
            x -= gap;
        }
        chips.reverse();
        let update_all =
            (store.view == View::Updates && store.detail.is_none() && store.update_count() > 0)
                .then(|| {
                    let bw = fonts.engine.measure_line(styles.small, "Update all") + u * 2;
                    let r = Rect::new(x - bw - gap, chip_y, bw, chip_h);
                    x = r.x;
                    r
                });
        let _ = x;

        let list = Rect::new(
            content.x + pad / 2,
            heading.bottom() + u / 4,
            content.width - pad,
            (content.bottom() - heading.bottom() - u / 2).max(0),
        );
        let row_h = u * 4;
        let rows = usize::try_from((list.height / row_h.max(1)).max(1)).unwrap_or(1);

        let detail = store
            .detail
            .map(|at| detail_layout(fonts, &styles, &metrics, store, at, content));

        Self {
            scale,
            size,
            metrics,
            compact,
            title,
            search,
            sidebar,
            nav,
            categories_label,
            refresh,
            content,
            heading,
            chips,
            update_all,
            list,
            row_h,
            rows,
            footer,
            detail,
        }
    }

    /// A visible row's rectangle, by its offset from the top of the list.
    #[must_use]
    pub fn row(&self, offset: usize) -> Rect {
        let o = i32::try_from(offset).unwrap_or(0);
        Rect::new(
            self.list.x,
            self.list.y + o * self.row_h,
            self.list.width - self.metrics.px(8),
            self.row_h,
        )
    }

    /// A row's button.
    #[must_use]
    pub fn row_button(&self, row: Rect) -> Rect {
        let u = self.metrics.unit;
        let bw = u * 6;
        let bh = u * 7 / 4;
        Rect::new(
            row.right() - bw - u / 2,
            row.y + (row.height - bh) / 2,
            bw,
            bh,
        )
    }

    /// What is at `p`.
    #[must_use]
    pub fn hit(&self, store: &Store, p: Point) -> Option<Target> {
        if self.search.contains(p) {
            return Some(Target::Search);
        }
        if self.refresh.contains(p) {
            return Some(Target::Refresh);
        }
        if let Some((view, _)) = self.nav.iter().find(|(_, r)| r.contains(p)) {
            return Some(Target::Nav(*view));
        }
        if let Some(d) = &self.detail {
            if d.back.contains(p) {
                return Some(Target::Back);
            }
            if let Some((action, _)) = d.buttons.iter().find(|(_, r)| r.contains(p)) {
                return Some(Target::Button(*action));
            }
            if d.link.is_some_and(|r| r.contains(p)) {
                return Some(Target::Link);
            }
            if let Some(shot) = d.shot
                && d.body.contains(p)
            {
                if shot.prev.is_some_and(|r| r.contains(p)) {
                    return Some(Target::Shot(-1));
                }
                if shot.next.is_some_and(|r| r.contains(p)) {
                    return Some(Target::Shot(1));
                }
            }
            return None;
        }
        if let Some((filter, _)) = self.chips.iter().find(|(_, r)| r.contains(p)) {
            return Some(Target::Chip(*filter));
        }
        if self.update_all.is_some_and(|r| r.contains(p)) {
            return Some(Target::UpdateAll);
        }
        if !self.list.contains(p) {
            return None;
        }
        let offset = usize::try_from((p.y - self.list.y) / self.row_h.max(1)).ok()?;
        if offset >= self.rows {
            return None;
        }
        let index = store.scroll + offset;
        let at = *store.results.get(index)?;
        let row = self.row(offset);
        if !row.contains(p) {
            return None;
        }
        if let Some(action) = store.row_action(at)
            && store.job_for(at).is_none()
            && self.row_button(row).contains(p)
        {
            return Some(Target::RowAction(index, action));
        }
        Some(Target::Row(index))
    }

    /// Whether `p` is over something that takes a click.
    #[must_use]
    pub fn clickable(&self, store: &Store, p: Point) -> bool {
        !matches!(self.hit(store, p), None | Some(Target::Search))
    }
}

/// `a` moved towards `b` by `percent`.
fn blend(a: Color, b: Color, percent: u16) -> Color {
    let p = percent.min(100);
    let m = |x: u8, y: u8| {
        u8::try_from((u16::from(x) * (100 - p) + u16::from(y) * p) / 100).unwrap_or(u8::MAX)
    };
    Color::rgba(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b), m(a.a, b.a))
}

/// `3012` as `3,012`.
fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn action_label(action: Action, confirming: bool) -> &'static str {
    match action {
        Action::Install => "Install",
        Action::Open => "Open",
        Action::Update => "Update",
        Action::Remove if confirming => "Remove?",
        Action::Remove => "Remove",
    }
}

fn detail_layout(
    fonts: &mut Fonts,
    styles: &Styles,
    metrics: &Metrics,
    store: &Store,
    at: At,
    content: Rect,
) -> DetailLayout {
    let u = metrics.unit;
    let pad = metrics.pad;
    let x = content.x + pad;
    let w = content.width - 2 * pad;
    let back_w = fonts.engine.measure_line(styles.small, "Back") + u * 2;
    let back = Rect::new(x, content.y + u / 2, back_w, u * 7 / 4);
    let icon_side = u * 5;
    let icon = Rect::new(x + u / 2, back.bottom() + u, icon_side, icon_side);
    let head = Rect::new(
        icon.right() + u,
        icon.y,
        (w - icon_side - u * 2).max(0),
        icon_side,
    );

    let mut buttons = Vec::new();
    let mut bx = content.right() - pad;
    let bh = u * 2;
    let by = icon.bottom() - bh;
    for action in store.actions(at).iter().rev() {
        let label = action_label(*action, true);
        let bw = fonts.engine.measure_line(styles.strong, label) + u * 5 / 2;
        let bw = bw.max(u * 6);
        bx -= bw;
        buttons.push((*action, Rect::new(bx, by, bw, bh)));
        bx -= u / 2;
    }
    buttons.reverse();

    let entry = store.catalog.get(at);
    let link = entry.filter(|e| !e.homepage.is_empty()).map(|e| {
        let lw = fonts.engine.measure_line(styles.small, &e.homepage)
            + fonts.engine.measure_line(styles.icon_small, LINK)
            + u / 2;
        Rect::new(
            head.x,
            icon.bottom() - u * 3 / 2,
            lw.min((bx - head.x - u).max(0)),
            u * 3 / 2,
        )
    });
    let body_y = icon.bottom() + u;
    let body = Rect::new(x, body_y, w, (content.bottom() - body_y - u / 2).max(0));
    let line_h = fonts.engine.line_height(styles.text) + u / 4;
    let shots = entry.map_or(0, |e| e.screenshots.len());
    let shot = (shots > 0).then(|| {
        // Sixteen by ten, as most screenshots are, no wider than the page
        // and no taller than a comfortable glance.
        let mut fw = body.width.min(u * 46);
        let mut fh = fw * 10 / 16;
        if fh > u * 22 {
            fh = u * 22;
            fw = fh * 16 / 10;
        }
        let scroll = i32::try_from(store.detail_scroll).unwrap_or(0) * line_h;
        let frame = Rect::new(body.x, body.y + u / 2 - scroll, fw, fh);
        let caption = Rect::new(frame.x, frame.bottom() + u / 4, fw, u * 3 / 2);
        let side = u * 9 / 4;
        let arrow = |left: bool| {
            let ax = if left {
                frame.x + u / 2
            } else {
                frame.right() - u / 2 - side
            };
            Rect::new(ax, frame.y + (fh - side) / 2, side, side)
        };
        ShotLayout {
            frame,
            caption,
            prev: (shots > 1).then(|| arrow(true)),
            next: (shots > 1).then(|| arrow(false)),
            height: fh + u * 9 / 4,
        }
    });
    DetailLayout {
        back,
        icon,
        head,
        buttons,
        link,
        body,
        line_h,
        shot,
    }
}

/// Paint the store.
#[allow(clippy::too_many_lines)] // one pass over the window, top to bottom
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    icons: &mut Icons,
    pictures: &mut Pictures,
    store: &Store,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let m = &layout.metrics;
    let styles = fonts.styles(m);
    let ink = Ink::new(appearance);
    let u = m.unit;
    let whole = Rect::from_size(layout.size);

    // The window is opaque: a translucent window over a busy desktop makes
    // small text hard to read, which a menu that is gone in a second gets
    // away with and a store does not.
    let Colour([r, g, b, _]) = appearance.background;
    pen.fill_rect(whole, Color::rgb(r, g, b));
    let side_bg = draw::mix(Colour([r, g, b, 0xFF]), appearance.selection, 22);
    pen.fill_rect(Rect::new(0, 0, layout.sidebar.width, whole.height), side_bg);
    let rule = draw::mix(Colour([r, g, b, 0xFF]), appearance.selection, 60);
    pen.fill_rect(
        Rect::new(0, layout.sidebar.y - m.px(1), whole.width, m.px(1)),
        rule,
    );
    pen.fill_rect(Rect::new(0, layout.footer.y, whole.width, m.px(1)), rule);

    paint_title(&mut pen, fonts, &styles, layout, &ink);
    paint_search(&mut pen, fonts, &styles, layout, store, &ink);
    paint_sidebar(&mut pen, fonts, &styles, layout, store, &ink);

    if let (Some(d), Some(at)) = (&layout.detail, store.detail) {
        paint_detail(
            &mut pen, fonts, &styles, icons, pictures, layout, d, store, at, &ink,
        );
    } else {
        paint_list(&mut pen, fonts, &styles, icons, layout, store, &ink);
    }
    paint_footer(&mut pen, fonts, &styles, layout, store, &ink);
    let _ = u;
}

fn paint_title(pen: &mut Pen<'_>, fonts: &mut Fonts, styles: &Styles, layout: &Layout, ink: &Ink) {
    let engine = &mut fonts.engine;
    let r = layout.title;
    let icon_w = engine.measure_line(styles.icon_large, STORE);
    if layout.compact {
        let centred = Rect::new(0, r.y, layout.sidebar.width, r.height);
        draw::centred(pen, engine, styles.icon_large, centred, STORE, ink.accent);
        return;
    }
    draw::label(pen, engine, styles.icon_large, r, STORE, ink.accent);
    let text = Rect::new(
        r.x + icon_w + layout.metrics.unit / 2,
        r.y,
        r.width,
        r.height,
    );
    draw::label(pen, engine, styles.large, text, "Store", ink.text);
}

fn paint_search(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    layout: &Layout,
    store: &Store,
    ink: &Ink,
) {
    let m = &layout.metrics;
    let u = m.unit;
    let r = layout.search;
    pen.fill_rounded_rect(r, r.height / 2, ink.card);
    let engine = &mut fonts.engine;
    let icon_x = r.x + u * 3 / 4;
    let icon_w = draw::label(
        pen,
        engine,
        styles.icon,
        Rect::new(icon_x, r.y, u * 2, r.height),
        SEARCH,
        ink.dim,
    );
    let text_r = Rect::new(
        icon_x + icon_w + u / 2,
        r.y,
        r.right() - icon_x - icon_w - u * 2,
        r.height,
    );
    let caret_x = if store.query.is_empty() {
        let names: Vec<&str> = store.sources.iter().map(|s| s.label.as_str()).collect();
        let hint = match names.as_slice() {
            [] => "Search".to_owned(),
            [one] => format!("Search {one}"),
            [init @ .., last] => format!("Search {} and {last}", init.join(", ")),
        };
        let after_caret = Rect::new(text_r.x + m.px(6), text_r.y, text_r.width, text_r.height);
        draw::label(pen, engine, styles.text, after_caret, &hint, ink.dim);
        text_r.x
    } else {
        // A long search shows its end, where the typing is.
        let width = engine.measure_line(styles.text, &store.query);
        let x = text_r.x.min(text_r.right() - width);
        let mut clip = pen.with_clip(text_r);
        let top = draw::text_top(engine, styles.text, r.y, r.height);
        engine.draw(
            &mut clip,
            styles.text,
            Point::new(x, top),
            &store.query,
            ink.text,
        );
        x + width + m.px(1)
    };
    let caret_h = u * 5 / 4;
    pen.fill_rect(
        Rect::new(caret_x, r.y + (r.height - caret_h) / 2, m.px(2), caret_h),
        ink.accent,
    );
}

#[allow(clippy::too_many_lines)] // three views, the categories, Refresh
fn paint_sidebar(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    layout: &Layout,
    store: &Store,
    ink: &Ink,
) {
    let m = &layout.metrics;
    let u = m.unit;
    let installed = store
        .catalog
        .iter()
        .filter(|(_, e)| matches!(e.state, State::Installed { explicit: true, .. }))
        .count();
    let updates = store.update_count();
    for (view, r) in &layout.nav {
        let chosen = *view == store.view && store.detail.is_none();
        let hovered = store.hover == Some(Target::Nav(*view));
        if chosen {
            pen.fill_rounded_rect(*r, m.px(8), ink.selection);
        } else if hovered {
            pen.fill_rounded_rect(*r, m.px(8), ink.card);
        }
        let (icon, label, big) = match view {
            View::Discover => (DISCOVER, "Discover", true),
            View::Installed => (INSTALLED, "Installed", true),
            View::Updates => (UPDATES, "Updates", true),
            View::Category(c) => (c.icon(), c.label(), false),
        };
        let engine = &mut fonts.engine;
        let (istyle, tstyle) = if big {
            (
                styles.icon,
                if chosen { styles.strong } else { styles.text },
            )
        } else {
            (styles.icon_small, styles.small)
        };
        let colour = if chosen { ink.accent } else { ink.dim };
        if layout.compact {
            draw::centred(pen, engine, istyle, *r, icon, colour);
            continue;
        }
        let ix = r.x + m.pad / 2;
        draw::label(
            pen,
            engine,
            istyle,
            Rect::new(ix, r.y, u * 2, r.height),
            icon,
            colour,
        );
        let tx = ix + u * 3 / 2 + u / 4;
        let text_colour = if chosen {
            ink.text
        } else {
            blend(ink.text, ink.dim, 35)
        };
        draw::label(
            pen,
            engine,
            tstyle,
            Rect::new(tx, r.y, r.right() - tx, r.height),
            label,
            text_colour,
        );
        let badge = match view {
            View::Updates if updates > 0 => Some((count(updates), true)),
            View::Installed if installed > 0 => Some((count(installed), false)),
            _ => None,
        };
        if let Some((text, lit)) = badge {
            let bw = engine.measure_line(styles.small, &text) + u * 3 / 4;
            let bh = u * 5 / 4;
            let b = Rect::new(
                r.right() - bw - m.pad / 2,
                r.y + (r.height - bh) / 2,
                bw,
                bh,
            );
            if lit {
                pen.fill_rounded_rect(b, bh / 2, ink.accent);
                draw::centred(pen, engine, styles.small, b, &text, ink.on_accent);
            } else {
                draw::centred(pen, engine, styles.small, b, &text, ink.dim);
            }
        }
    }
    if let Some(r) = layout.categories_label {
        draw::label(
            pen,
            &mut fonts.engine,
            styles.small,
            r,
            "CATEGORIES",
            ink.dim,
        );
    }
    let r = layout.refresh;
    let hovered = store.hover == Some(Target::Refresh);
    let busy = store
        .jobs
        .iter()
        .any(|j| j.op == crate::source::Op::Refresh);
    if hovered {
        pen.fill_rounded_rect(r, m.px(8), ink.selection);
    }
    let engine = &mut fonts.engine;
    if layout.compact {
        draw::centred(pen, engine, styles.icon, r, REFRESH, ink.dim);
    } else {
        let ix = r.x + m.pad / 2;
        if busy {
            spinner(
                pen,
                Point::new(ix + u * 3 / 4, r.y + r.height / 2),
                u / 2,
                m,
                store.frame,
                ink.accent,
            );
        } else {
            draw::label(
                pen,
                engine,
                styles.icon,
                Rect::new(ix, r.y, u * 2, r.height),
                REFRESH,
                ink.dim,
            );
        }
        let tx = ix + u * 3 / 2 + u / 4;
        let label = if busy { "Refreshing…" } else { "Refresh" };
        draw::label(
            pen,
            engine,
            styles.small,
            Rect::new(tx, r.y, r.right() - tx, r.height),
            label,
            ink.dim,
        );
    }
}

/// A turning arc: something is under way.
fn spinner(pen: &mut Pen<'_>, centre: Point, radius: i32, m: &Metrics, frame: u32, colour: Color) {
    let turn = denise::TURN;
    let start = i32::try_from(frame % 24).unwrap_or(0) * turn / 24;
    pen.stroke_circle(centre, radius, m.px(2), colour.with_alpha(50));
    pen.stroke_arc(centre, radius, m.px(2), start, turn / 3, colour);
}

/// A source's badge: its name on a tint of its colour. Returns its width.
#[allow(clippy::too_many_arguments)]
fn badge(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    right: i32,
    mid_y: i32,
    store: &Store,
    source: u16,
    m: &Metrics,
) -> i32 {
    let Some(src) = store.sources.get(usize::from(source)) else {
        return 0;
    };
    let u = m.unit;
    let colour = draw::colour(src.colour);
    let engine = &mut fonts.engine;
    let tw = engine.measure_line(styles.small, &src.label);
    let iw = engine.measure_line(styles.icon_small, &src.icon);
    let bw = tw + iw + u * 5 / 4;
    let bh = u * 11 / 8;
    let r = Rect::new(right - bw, mid_y - bh / 2, bw, bh);
    pen.fill_rounded_rect(r, bh / 2, colour.with_alpha(40));
    let ix = r.x + u / 2;
    draw::label(
        pen,
        engine,
        styles.icon_small,
        Rect::new(ix, r.y, iw, r.height),
        &src.icon,
        colour,
    );
    draw::label(
        pen,
        engine,
        styles.small,
        Rect::new(ix + iw + u / 4, r.y, tw + m.px(2), r.height),
        &src.label,
        colour,
    );
    bw
}

/// An entry's icon in `r`: its picture, or its source's glyph on a tile.
#[allow(clippy::too_many_arguments)]
fn entry_icon(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    icons: &mut Icons,
    store: &Store,
    at: At,
    entry: &Entry,
    r: Rect,
    large: bool,
    ink: &Ink,
) {
    let side = u32::try_from(r.width).unwrap_or(0);
    let path = if large || side > 64 {
        entry.icon_large.as_ref().or(entry.icon.as_ref())
    } else {
        entry.icon.as_ref().or(entry.icon_large.as_ref())
    };
    if let Some(icon) = path.and_then(|p| icons.get(p, side))
        && let Some(view) =
            denise::PixelView::new(&icon.pixels, Size::new(icon.size, icon.size), icon.size)
    {
        pen.blit(&view, Point::new(r.x, r.y));
        return;
    }
    let colour = store
        .sources
        .get(usize::from(at.source))
        .map_or(ink.accent, |s| draw::colour(s.colour));
    pen.fill_rounded_rect(r, r.width / 4, colour.with_alpha(36));
    let glyph = if entry.app { STORE } else { PACKAGE };
    let style = if large {
        styles.icon_large
    } else {
        styles.icon
    };
    draw::centred(pen, &mut fonts.engine, style, r, glyph, colour);
}

#[allow(clippy::too_many_lines)] // the heading, the chips, then each row
fn paint_list(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    icons: &mut Icons,
    layout: &Layout,
    store: &Store,
    ink: &Ink,
) {
    let m = &layout.metrics;
    let u = m.unit;

    // The heading, and what it is searching.
    let engine = &mut fonts.engine;
    let title = if store.query.is_empty() {
        store.view.title().to_owned()
    } else if store.view == View::Discover {
        "Results".to_owned()
    } else {
        format!("{} · results", store.view.title())
    };
    let heading_end = layout
        .update_all
        .or_else(|| layout.chips.first().map(|(_, r)| *r))
        .map_or(layout.heading.right(), |r| r.x - u);
    draw::label(
        pen,
        engine,
        styles.large,
        Rect::new(
            layout.heading.x,
            layout.heading.y,
            (heading_end - layout.heading.x).max(0),
            layout.heading.height,
        ),
        &title,
        ink.text,
    );

    for (filter, r) in &layout.chips {
        let chosen = *filter == store.filter;
        let hovered = store.hover == Some(Target::Chip(*filter));
        let label = match filter {
            None => format!("All  {}", count(store.counts.iter().sum())),
            Some(s) => {
                let src = &store.sources[usize::from(*s)];
                format!(
                    "{}  {}",
                    src.label,
                    count(store.counts.get(usize::from(*s)).copied().unwrap_or(0))
                )
            }
        };
        let tint = filter
            .and_then(|s| store.sources.get(usize::from(s)))
            .map_or(ink.accent, |s| draw::colour(s.colour));
        let (fill, text) = if chosen {
            (Some(tint), ink.on_accent)
        } else if hovered {
            (Some(ink.selection), ink.text)
        } else {
            (Some(ink.card), ink.text)
        };
        if let Some(fill) = fill {
            pen.fill_rounded_rect(*r, r.height / 2, fill);
        }
        let engine = &mut fonts.engine;
        let mut x = r.x + u * 3 / 4;
        if let Some(src) = filter.and_then(|s| store.sources.get(usize::from(s))) {
            let icon_colour = if chosen { ink.on_accent } else { tint };
            let iw = draw::label(
                pen,
                engine,
                styles.icon_small,
                Rect::new(x, r.y, u * 2, r.height),
                &src.icon,
                icon_colour,
            );
            x += iw + u / 3;
        }
        draw::label(
            pen,
            engine,
            styles.small,
            Rect::new(x, r.y, r.right() - x, r.height),
            &label,
            text,
        );
    }
    if let Some(r) = layout.update_all {
        let hovered = store.hover == Some(Target::UpdateAll);
        let fill = if hovered {
            blend(ink.accent, ink.text, 25)
        } else {
            ink.accent
        };
        draw::button(
            pen,
            &mut fonts.engine,
            styles.small,
            r,
            "Update all",
            (Some(fill), ink.on_accent),
            None,
        );
    }

    if store.results.is_empty() {
        paint_empty(pen, fonts, styles, layout, store, ink);
        return;
    }

    let visible = store
        .results
        .iter()
        .enumerate()
        .skip(store.scroll)
        .take(layout.rows);
    for (offset, (index, &at)) in visible.enumerate() {
        let Some(entry) = store.catalog.get(at) else {
            continue;
        };
        let row = layout.row(offset);
        let selected = index == store.selected;
        let hovered =
            matches!(store.hover, Some(Target::Row(i) | Target::RowAction(i, _)) if i == index);
        if selected {
            pen.fill_rounded_rect(row, m.px(10), ink.selection);
        } else if hovered {
            pen.fill_rounded_rect(row, m.px(10), ink.card);
        }

        let icon_side = u * 11 / 4;
        let icon = Rect::new(
            row.x + u * 3 / 4,
            row.y + (row.height - icon_side) / 2,
            icon_side,
            icon_side,
        );
        entry_icon(
            pen, fonts, styles, icons, store, at, entry, icon, false, ink,
        );

        // The button, then the badge left of it, then the text in what is
        // left.
        let button = layout.row_button(row);
        let job = store.job_for(at);
        let mut right = button.x - u / 2;
        if let Some(job) = job {
            let running = store.is_running(job);
            let label = if running { "Working…" } else { "Queued" };
            let centre = Point::new(button.x + u, button.y + button.height / 2);
            if running {
                spinner(pen, centre, u / 2, m, store.frame, ink.accent);
            }
            draw::label(
                pen,
                &mut fonts.engine,
                styles.small,
                Rect::new(button.x + u * 2, button.y, button.width, button.height),
                label,
                ink.dim,
            );
        } else if let Some(action) = store.row_action(at) {
            let hot = store.hover == Some(Target::RowAction(index, action));
            paint_button(
                pen, fonts, styles, button, action, hot, false, false, m, ink,
            );
        } else if entry.state.installed() {
            let r = Rect::new(button.x, button.y, button.width, button.height);
            let engine = &mut fonts.engine;
            let label = if entry.protected {
                "System"
            } else {
                "Installed"
            };
            let glyph = if entry.protected { LOCK } else { INSTALLED };
            let tw = engine.measure_line(styles.small, label);
            let iw = engine.measure_line(styles.icon_small, glyph);
            let x = r.x + (r.width - tw - iw - u / 3) / 2;
            draw::label(
                pen,
                engine,
                styles.icon_small,
                Rect::new(x, r.y, iw, r.height),
                glyph,
                ink.dim,
            );
            draw::label(
                pen,
                engine,
                styles.small,
                Rect::new(x + iw + u / 3, r.y, tw + m.px(2), r.height),
                label,
                ink.dim,
            );
        }
        let mid = row.y + row.height / 2;
        let bw = badge(pen, fonts, styles, right, mid, store, at.source, m);
        right -= bw + u / 2;

        let tx = icon.right() + u * 3 / 4;
        let text_w = (right - tx).max(0);
        let engine = &mut fonts.engine;
        let line = engine.line_height(styles.strong);
        let small_line = engine.line_height(styles.small);
        let block = line + small_line + m.px(2);
        let top = row.y + (row.height - block) / 2;
        let name_r = Rect::new(tx, top, text_w, line);
        let mut clip = pen.with_clip(name_r);
        let size = engine.draw(
            &mut clip,
            styles.strong,
            Point::new(tx, top),
            &entry.name,
            ink.text,
        );
        let name_w = i32::try_from(size.width).unwrap_or(0);
        let version = match &entry.state {
            State::Installed {
                update: Some(new),
                version,
                ..
            } => format!("{version} → {new}"),
            State::Installed { version, .. } if !version.is_empty() => version.clone(),
            _ => entry.version.clone(),
        };
        if !version.is_empty() {
            let vx = tx + name_w + u / 2;
            let vtop = top + (line - small_line);
            engine.draw(
                &mut clip,
                styles.small,
                Point::new(vx, vtop),
                &version,
                ink.dim,
            );
        }
        drop(clip);
        let summary_r = Rect::new(tx, top + line + m.px(2), text_w, small_line);
        let mut clip = pen.with_clip(summary_r);
        engine.draw(
            &mut clip,
            styles.small,
            Point::new(tx, summary_r.y),
            &entry.summary,
            ink.dim,
        );
    }

    // Where in the list the window is.
    let total = store.results.len();
    if total > layout.rows {
        let track = Rect::new(
            layout.list.right() - m.px(4),
            layout.list.y,
            m.px(3),
            layout.list.height,
        );
        let rows = i32::try_from(layout.rows).unwrap_or(1);
        let total_i = i32::try_from(total).unwrap_or(i32::MAX);
        let thumb_h = (track.height * rows / total_i).max(u);
        let scroll = i32::try_from(store.scroll).unwrap_or(0);
        let max_scroll = (total_i - rows).max(1);
        let thumb_y = track.y + (track.height - thumb_h) * scroll / max_scroll;
        pen.fill_rounded_rect(
            Rect::new(track.x, thumb_y, track.width, thumb_h),
            track.width / 2,
            ink.selection,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_button(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    r: Rect,
    action: Action,
    hovered: bool,
    focused: bool,
    confirming: bool,
    m: &Metrics,
    ink: &Ink,
) {
    let label = action_label(action, confirming);
    let engine = &mut fonts.engine;
    match action {
        Action::Install | Action::Update => {
            let fill = if hovered {
                blend(ink.accent, ink.text, 25)
            } else {
                ink.accent
            };
            draw::button(
                pen,
                engine,
                styles.strong,
                r,
                label,
                (Some(fill), ink.on_accent),
                None,
            );
        }
        Action::Open => {
            draw::outline_button(pen, engine, styles.strong, r, label, hovered, m, ink);
        }
        Action::Remove if confirming => {
            draw::button(
                pen,
                engine,
                styles.strong,
                r,
                label,
                (Some(ink.warn), ink.on_accent),
                None,
            );
        }
        Action::Remove => {
            draw::button(
                pen,
                engine,
                styles.strong,
                r,
                label,
                (hovered.then_some(ink.selection), ink.warn),
                Some((m.px(1), ink.warn.with_alpha(160))),
            );
        }
    }
    if focused {
        draw::focus_ring(pen, r, r.height / 2, m, ink);
    }
}

fn paint_empty(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    layout: &Layout,
    store: &Store,
    ink: &Ink,
) {
    let u = layout.metrics.unit;
    let busy = store.loading.iter().find_map(|l| match l {
        Loading::Busy(text) => Some(text.clone()),
        _ => None,
    });
    let (glyph, line, detail) =
        if let Some(text) = busy.filter(|_| store.catalog.iter().next().is_none()) {
            (None, "Getting the catalogues ready".to_owned(), text)
        } else if !store.query.is_empty() {
            (
                Some(SEARCH),
                format!("Nothing found for “{}”", store.query.trim()),
                match store.filter {
                    Some(_) => "Try every source: Ctrl+0".to_owned(),
                    None => "Try fewer or other words".to_owned(),
                },
            )
        } else {
            match store.view {
                View::Updates => (
                    Some(INSTALLED),
                    "Everything is up to date".to_owned(),
                    String::new(),
                ),
                View::Installed => (
                    Some(PACKAGE),
                    "Nothing installed from the store yet".to_owned(),
                    String::new(),
                ),
                _ => (Some(STORE), "Nothing here".to_owned(), String::new()),
            }
        };
    let c = layout.list;
    let mid = c.y + c.height / 3;
    let engine = &mut fonts.engine;
    match glyph {
        Some(g) => draw::centred(
            pen,
            engine,
            styles.icon_large,
            Rect::new(c.x, mid - u * 3, c.width, u * 2),
            g,
            ink.dim,
        ),
        None => spinner(
            pen,
            Point::new(c.x + c.width / 2, mid - u * 2),
            u,
            &layout.metrics,
            store.frame,
            ink.accent,
        ),
    }
    draw::centred(
        pen,
        engine,
        styles.strong,
        Rect::new(c.x, mid, c.width, u * 2),
        &line,
        ink.text,
    );
    if !detail.is_empty() {
        draw::centred(
            pen,
            engine,
            styles.small,
            Rect::new(c.x, mid + u * 2, c.width, u * 3 / 2),
            &detail,
            ink.dim,
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn paint_detail(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    icons: &mut Icons,
    pictures: &mut Pictures,
    layout: &Layout,
    d: &DetailLayout,
    store: &Store,
    at: At,
    ink: &Ink,
) {
    let m = &layout.metrics;
    let u = m.unit;
    let Some(entry) = store.catalog.get(at) else {
        return;
    };

    // Back.
    let hovered = store.hover == Some(Target::Back);
    if hovered {
        pen.fill_rounded_rect(d.back, d.back.height / 2, ink.selection);
    }
    let engine = &mut fonts.engine;
    let iw = draw::label(
        pen,
        engine,
        styles.icon_small,
        Rect::new(d.back.x + u / 2, d.back.y, u, d.back.height),
        BACK,
        ink.dim,
    );
    draw::label(
        pen,
        engine,
        styles.small,
        Rect::new(
            d.back.x + u / 2 + iw + u / 4,
            d.back.y,
            d.back.width,
            d.back.height,
        ),
        "Back",
        ink.dim,
    );

    entry_icon(
        pen, fonts, styles, icons, store, at, entry, d.icon, true, ink,
    );

    // Name, developer, badge.
    let engine = &mut fonts.engine;
    let name_h = engine.line_height(styles.large);
    let mut clip = pen.with_clip(Rect::new(d.head.x, d.head.y, d.head.width, name_h));
    engine.draw(
        &mut clip,
        styles.large,
        Point::new(d.head.x, d.head.y),
        &entry.name,
        ink.text,
    );
    drop(clip);
    let mut y = d.head.y + name_h + m.px(2);
    let small_h = engine.line_height(styles.small);
    let src_label = store
        .sources
        .get(usize::from(at.source))
        .map_or("", |s| s.label.as_str());
    let by = if entry.developer.is_empty() {
        src_label.to_owned()
    } else {
        entry.developer.clone()
    };
    let bw = engine.measure_line(styles.small, &by);
    draw::label(
        pen,
        engine,
        styles.small,
        Rect::new(d.head.x, y, d.head.width, small_h),
        &by,
        ink.dim,
    );
    let right = d.head.x + bw + u / 2 + badge_width(fonts, styles, store, at.source, m);
    badge(
        pen,
        fonts,
        styles,
        right,
        y + small_h / 2,
        store,
        at.source,
        m,
    );
    y += small_h + u / 4;
    let _ = y;

    if let Some(link) = d.link {
        let hovered = store.hover == Some(Target::Link);
        let colour = if hovered { ink.text } else { ink.accent };
        let engine = &mut fonts.engine;
        let iw = draw::label(
            pen,
            engine,
            styles.icon_small,
            Rect::new(link.x, link.y, u, link.height),
            LINK,
            colour,
        );
        draw::label(
            pen,
            engine,
            styles.small,
            Rect::new(
                link.x + iw + u / 3,
                link.y,
                link.width - iw - u / 3,
                link.height,
            ),
            &entry.homepage,
            colour,
        );
    }

    // Buttons, or the operation under way.
    if let Some(job) = store.job_for(at) {
        let running = store.is_running(job);
        let right = layout.content.right() - m.pad;
        let engine = &mut fonts.engine;
        let text = if running {
            job.progress.clone()
        } else {
            "Waiting for the operation before it".to_owned()
        };
        let tw = engine.measure_line(styles.small, &text).min(u * 22);
        let by = d.icon.bottom() - u;
        let r = Rect::new(right - tw, by - u, tw, u * 2);
        let mut clip = pen.with_clip(r);
        let top = draw::text_top(engine, styles.small, r.y, r.height);
        engine.draw(
            &mut clip,
            styles.small,
            Point::new(r.x, top),
            &text,
            ink.dim,
        );
        drop(clip);
        if running {
            spinner(
                pen,
                Point::new(r.x - u, by),
                u * 2 / 3,
                m,
                store.frame,
                ink.accent,
            );
        }
    } else {
        let confirming = store.confirming(at);
        for (i, (action, r)) in d.buttons.iter().enumerate() {
            let hovered = store.hover == Some(Target::Button(*action));
            let focused = store.focus_visible && store.focus == Focus::Button(i);
            paint_button(
                pen,
                fonts,
                styles,
                *r,
                *action,
                hovered,
                focused,
                confirming && *action == Action::Remove,
                m,
                ink,
            );
        }
        if entry.protected && entry.state.installed() {
            let engine = &mut fonts.engine;
            let text = "Part of the system: cannot be removed here";
            let tw = engine.measure_line(styles.small, text);
            let right = d
                .buttons
                .first()
                .map_or(layout.content.right() - m.pad, |(_, r)| r.x - u);
            let r = Rect::new(right - tw, d.icon.bottom() - u * 2, tw, u * 2);
            draw::label(pen, engine, styles.small, r, text, ink.dim);
        }
    }

    // The body: summary, facts, description; scrolled by lines.
    let body = d.body;
    pen.fill_rect(
        Rect::new(body.x, body.y - u / 2, body.width, m.px(1)),
        ink.selection,
    );
    let mut clip = pen.with_clip(body);
    let scroll = i32::try_from(store.detail_scroll).unwrap_or(0) * d.line_h;
    let mut y = body.y + u / 2 - scroll;
    if let Some(shot) = d.shot {
        paint_shot(
            &mut clip, fonts, styles, pictures, &shot, store, entry, m, ink,
        );
        y += shot.height;
    }
    let engine = &mut fonts.engine;

    for line in engine.wrap(styles.strong, &entry.summary, body.width) {
        engine.draw(
            &mut clip,
            styles.strong,
            Point::new(body.x, y),
            line,
            ink.text,
        );
        y += d.line_h;
    }
    y += u / 2;

    let mut facts: Vec<(&str, String)> = Vec::new();
    match &entry.state {
        State::Installed {
            version, update, ..
        } => {
            facts.push((
                "Installed",
                if version.is_empty() {
                    "yes".into()
                } else {
                    version.clone()
                },
            ));
            if let Some(new) = update {
                facts.push(("Update", new.clone()));
            }
        }
        State::Available => {}
    }
    if !entry.version.is_empty() {
        facts.push(("Version", entry.version.clone()));
    }
    let origin = if entry.origin.is_empty() {
        src_label.to_owned()
    } else {
        format!("{src_label} · {}", entry.origin)
    };
    facts.push(("Source", origin));
    if !entry.license.is_empty() {
        facts.push(("Licence", entry.license.clone()));
    }
    if let Some(bytes) = entry.download_size {
        facts.push(("Download", human_size(bytes)));
    }
    if let Some(bytes) = entry.installed_size {
        facts.push(("Size", human_size(bytes)));
    }
    facts.push(("ID", entry.id.clone()));
    let row_h = engine.line_height(styles.small) + u / 3;
    let used = draw::details(
        &mut clip,
        engine,
        styles.small,
        ink,
        m,
        (body.x, y, body.width),
        row_h,
        &facts,
    );
    y += used + u;

    if !entry.description.is_empty() {
        for line in engine.wrap(styles.text, &entry.description, body.width.min(u * 48)) {
            if y + d.line_h >= body.y && y < body.bottom() {
                engine.draw(
                    &mut clip,
                    styles.text,
                    Point::new(body.x, y),
                    line,
                    blend(ink.text, ink.dim, 25),
                );
            }
            y += d.line_h;
        }
    }
    drop(clip);
}

/// The screenshot showing, in its frame: the picture, or a spinner while it
/// comes, or why it cannot; arrows over its sides and a caption beneath.
#[allow(clippy::too_many_arguments)]
fn paint_shot(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    pictures: &mut Pictures,
    shot: &ShotLayout,
    store: &Store,
    entry: &Entry,
    m: &Metrics,
    ink: &Ink,
) {
    let u = m.unit;
    let f = shot.frame;
    let radius = m.px(10);
    pen.fill_rounded_rect(f, radius, ink.card);
    let Some(current) = entry.screenshots.get(store.shot) else {
        return;
    };
    let size = |v: i32| u32::try_from(v.max(1)).unwrap_or(1);
    match pictures.get(&current.url).cloned() {
        Some(Picture::Ready(_)) => {
            if let Some((pixels, w, h)) =
                pictures.fitted(&current.url, size(f.width), size(f.height))
                && let Some(view) = denise::PixelView::new(&pixels, Size::new(w, h), w)
            {
                let (w, h) = (i32::try_from(w).unwrap_or(0), i32::try_from(h).unwrap_or(0));
                let dest = Rect::new(f.x + (f.width - w) / 2, f.y + (f.height - h) / 2, w, h);
                pen.blit_rounded(&view, dest, dest, radius);
            }
        }
        Some(Picture::Failed(e)) => {
            let engine = &mut fonts.engine;
            let mid = f.y + f.height / 2;
            draw::centred(
                pen,
                engine,
                styles.icon_large,
                Rect::new(f.x, mid - u * 2, f.width, u * 2),
                WARN,
                ink.dim,
            );
            draw::centred(
                pen,
                engine,
                styles.small,
                Rect::new(f.x, mid + u / 2, f.width, u * 3 / 2),
                &e,
                ink.dim,
            );
        }
        Some(Picture::Loading) | None => {
            spinner(
                pen,
                Point::new(f.x + f.width / 2, f.y + f.height / 2),
                u,
                m,
                store.frame,
                ink.accent,
            );
        }
    }
    for (target, rect, glyph) in [
        (Target::Shot(-1), shot.prev, draw::CHEVRON_LEFT),
        (Target::Shot(1), shot.next, draw::CHEVRON_RIGHT),
    ] {
        let Some(r) = rect else { continue };
        let hovered = store.hover == Some(target);
        let fill = if hovered {
            ink.selection
        } else {
            ink.background.with_alpha(200)
        };
        pen.fill_circle(
            Point::new(r.x + r.width / 2, r.y + r.height / 2),
            r.width / 2,
            fill,
        );
        draw::centred(pen, &mut fonts.engine, styles.icon, r, glyph, ink.text);
    }
    let engine = &mut fonts.engine;
    let n = entry.screenshots.len();
    let count = if n > 1 {
        format!("{} / {n}", store.shot + 1)
    } else {
        String::new()
    };
    let count_x = if count.is_empty() {
        shot.caption.right()
    } else {
        draw::right_label(pen, engine, styles.small, shot.caption, &count, ink.dim)
    };
    let caption = Rect::new(
        shot.caption.x,
        shot.caption.y,
        (count_x - u - shot.caption.x).max(0),
        shot.caption.height,
    );
    let mut clip = pen.with_clip(caption);
    let top = draw::text_top(engine, styles.small, caption.y, caption.height);
    engine.draw(
        &mut clip,
        styles.small,
        Point::new(caption.x, top),
        &current.caption,
        ink.dim,
    );
}

fn badge_width(fonts: &mut Fonts, styles: &Styles, store: &Store, source: u16, m: &Metrics) -> i32 {
    let Some(src) = store.sources.get(usize::from(source)) else {
        return 0;
    };
    let engine = &mut fonts.engine;
    engine.measure_line(styles.small, &src.label)
        + engine.measure_line(styles.icon_small, &src.icon)
        + m.unit * 5 / 4
}

#[allow(clippy::too_many_lines)] // a job, a notice, a problem, or the sources
fn paint_footer(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    layout: &Layout,
    store: &Store,
    ink: &Ink,
) {
    let m = &layout.metrics;
    let u = m.unit;
    let r = Rect::new(
        layout.footer.x + m.pad,
        layout.footer.y,
        layout.footer.width - 2 * m.pad,
        layout.footer.height,
    );
    let engine = &mut fonts.engine;

    let hints = if store.detail.is_some() {
        "Tab button   Enter press   Esc back"
    } else {
        "↑↓ choose   Enter open   Tab view   ←→ source   Ctrl+R refresh"
    };
    let hints_w = engine.measure_line(styles.small, hints);
    let hints_x = r.right() - hints_w;
    let narrow = hints_w > r.width / 2;
    if !narrow {
        draw::label(
            pen,
            engine,
            styles.small,
            Rect::new(hints_x, r.y, hints_w, r.height),
            hints,
            ink.dim,
        );
    }
    let text_right = if narrow { r.right() } else { hints_x - u };

    let mut x = r.x;
    let mid = r.y + r.height / 2;
    let (text, colour) = if let Some(job) = store.jobs.first() {
        spinner(
            pen,
            Point::new(x + u / 2, mid),
            u / 2,
            m,
            store.frame,
            ink.accent,
        );
        x += u * 3 / 2;
        let queued = store.jobs.len() - 1;
        let label = store
            .sources
            .get(usize::from(job.source))
            .map_or("", |s| s.label.as_str());
        let mut text = job.op.doing(&job.name, label);
        if !job.progress.is_empty() && !job.progress.starts_with(&text) {
            text = format!("{text} — {}", job.progress);
        }
        if queued > 0 {
            text = format!("{text}   (+{queued} waiting)");
        }
        (text, ink.text)
    } else if let Some(notice) = &store.notice {
        if notice.warn {
            let iw = draw::label(
                pen,
                engine,
                styles.icon_small,
                Rect::new(x, r.y, u, r.height),
                WARN,
                ink.warn,
            );
            x += iw + u / 3;
        }
        (
            notice.text.clone(),
            if notice.warn { ink.warn } else { ink.text },
        )
    } else if let Some(problem) = store.problems.first() {
        let iw = draw::label(
            pen,
            engine,
            styles.icon_small,
            Rect::new(x, r.y, u, r.height),
            WARN,
            ink.warn,
        );
        x += iw + u / 3;
        (problem.clone(), ink.warn)
    } else {
        let parts: Vec<String> = store
            .sources
            .iter()
            .zip(&store.loading)
            .enumerate()
            .map(|(i, (src, loading))| match loading {
                Loading::Busy(text) => format!("{}: {text}…", src.label),
                Loading::Failed(e) => format!("{}: {e}", src.label),
                Loading::Ready => {
                    let n = store.catalog.list(u16::try_from(i).unwrap_or(0)).len();
                    let noun = if src.apps { "applications" } else { "packages" };
                    format!("{} {} {noun}", src.label, count(n))
                }
            })
            .collect();
        let failed = store
            .loading
            .iter()
            .any(|l| matches!(l, Loading::Failed(_)));
        (
            parts.join("   ·   "),
            if failed { ink.warn } else { ink.dim },
        )
    };
    let engine = &mut fonts.engine;
    let area = Rect::new(x, r.y, (text_right - x).max(0), r.height);
    let mut clip = pen.with_clip(area);
    let top = draw::text_top(engine, styles.small, r.y, r.height);
    engine.draw(&mut clip, styles.small, Point::new(x, top), &text, colour);
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn counts_are_grouped_in_thousands() {
        assert_eq!(count(7), "7");
        assert_eq!(count(3012), "3,012");
        assert_eq!(count(1_234_567), "1,234,567");
    }
}

//! Laying out and painting the popup.
//!
//! One panel, top to bottom: a header with the switch; a card for the joined
//! network, with its address and link details; the other networks, with a
//! passphrase field opening under the one being joined; a line for what just
//! happened; and a footer of keys. What is there depends on the state, so the
//! layout is worked out from the popup each time, and the same [`Layout`]
//! answers where a click landed — the painted rectangle and the clickable one
//! can never drift apart.
//!
//! Sizes come from the font size and colours from the menu's appearance, so a
//! theme set for the menu dresses this too.

use crate::bar;
use crate::model::{Radio, Security, Station, band};
use crate::popup::{Busy, Focus, Popup, Target};
use alpymist_menu::config::{Appearance, Colour};
use alpymist_menu::font::LazyFont;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise::{Color, Frame};
use denise_render::Canvas;
use denise_text::{FontId, TextEngine, TextStyle};

/// Popup width in logical pixels, at a 16 px font.
const WIDTH: i32 = 380;
/// How many networks show at once.
pub const ROWS: usize = 6;

const LOCK: &str = "\u{f023}";
const RESCAN: &str = "\u{f0450}";
const EYE: &str = "\u{f0208}";
const EYE_OFF: &str = "\u{f0209}";

/// The fonts, loaded once.
pub struct Fonts {
    engine: TextEngine,
    text: FontId,
    strong: FontId,
    icons: FontId,
    /// Paths that could not be loaded, for the log.
    pub problems: Vec<String>,
}

impl Fonts {
    /// Load the appearance's fonts, and the semibold face beside the text
    /// face when there is one. Never fails: a missing face falls back to
    /// Denise's built-in bitmap.
    #[must_use]
    pub fn load(appearance: &Appearance) -> Self {
        let mut engine = TextEngine::new();
        let mut problems = Vec::new();
        let built_in = TextStyle::built_in(0).font;
        let mut add =
            |engine: &mut TextEngine, path: &str, required: bool| match std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| LazyFont::from_vec(path, bytes))
            {
                Ok(source) => Some(engine.add_font(Box::new(source))),
                Err(e) => {
                    if required {
                        problems.push(format!("{path}: {e}"));
                    }
                    None
                }
            };
        let text = add(&mut engine, &appearance.font, true).unwrap_or(built_in);
        let strong = appearance
            .font
            .contains("Regular")
            .then(|| appearance.font.replace("Regular", "SemiBold"))
            .and_then(|path| add(&mut engine, &path, false))
            .unwrap_or(text);
        let icons = add(&mut engine, &appearance.icon_font, true).unwrap_or(built_in);
        engine.set_default_font(text);
        Self {
            engine,
            text,
            strong,
            icons,
            problems,
        }
    }
}

/// The joined network's card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    /// The whole card.
    pub rect: Rect,
    /// The Disconnect button, when joined.
    pub disconnect: Option<Rect>,
    /// The Forget button, when the network is saved.
    pub forget: Option<Rect>,
}

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Output scale.
    pub scale: u32,
    /// The whole surface.
    pub size: Size,
    /// One unit: the font size, scaled.
    pub unit: i32,
    /// Padding inside the border.
    pub pad: i32,
    /// Border thickness.
    pub border: i32,
    /// Corner radius.
    pub radius: i32,
    /// Text heights.
    pub text_px: u16,
    /// Small text height.
    pub small_px: u16,
    /// The title line.
    pub header: Rect,
    /// The on/off switch.
    pub switch: Rect,
    /// A line said instead of the list, when there is no list to show.
    pub notice: Option<Rect>,
    /// The joined network.
    pub card: Option<Card>,
    /// The "Networks" heading.
    pub section: Option<Rect>,
    /// The rescan button on the heading.
    pub rescan: Option<Rect>,
    /// Visible network rows, with their place among the others.
    pub rows: Vec<(usize, Rect)>,
    /// The passphrase field and its buttons, under the row being joined.
    pub ask: Option<AskRects>,
    /// Where the rows would be, for scrolling.
    pub list: Rect,
    /// The message line.
    pub message: Option<Rect>,
    /// The footer.
    pub footer: Rect,
}

/// The passphrase field's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskRects {
    /// Behind all of it.
    pub block: Rect,
    /// The text field.
    pub field: Rect,
    /// Show or hide, inside the field's right end.
    pub reveal: Rect,
    /// The Join button.
    pub join: Rect,
}

impl Layout {
    /// Lay out `popup` at an output scale.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new(appearance: &Appearance, popup: &Popup, scale: u32) -> Self {
        let scale = scale.max(1);
        let s = i32::try_from(scale).unwrap_or(1);
        let font = i32::from(appearance.font_size.clamp(8, 64));
        let u = font * s;
        let px = |logical: i32| logical * s;
        let text_px = u16::try_from(u).unwrap_or(u16::MAX);
        let small_px = u16::try_from(u * 13 / 16).unwrap_or(u16::MAX);
        let border = px(2);
        let pad = u * 3 / 4;
        let width = px(WIDTH * font / 16);
        let inner_x = border + pad;
        let inner_w = width - 2 * inner_x;
        let state = popup.state();

        let header = Rect::new(inner_x, border + pad / 2, inner_w, u * 5 / 2);
        let switch_h = u * 3 / 2;
        let switch_w = u * 11 / 4;
        let switch = Rect::new(
            header.right() - switch_w,
            header.y + (header.height - switch_h) / 2,
            switch_w,
            switch_h,
        );
        let mut y = header.bottom() + px(4);

        let mut notice = None;
        let mut card = None;
        let mut section = None;
        let mut rescan = None;
        let mut rows = Vec::new();
        let mut ask = None;
        let row_h = u * 9 / 4;
        let list_x = border + pad / 2;
        let list_w = width - 2 * border - pad;
        let mut list = Rect::new(list_x, y, list_w, 0);

        if !popup.loaded() || state.radio != Radio::On {
            let r = Rect::new(inner_x, y, inner_w, u * 3);
            notice = Some(r);
            y = r.bottom();
        } else {
            let joining = matches!(state.station, Station::Connecting)
                || matches!(popup.busy(), Some(Busy::Joining(_)));
            if state.station == Station::Connected || state.station == Station::Roaming {
                // Name, address, and two lines of details.
                let h = pad + u * 3 / 2 + u * 5 / 4 + px(8) + 2 * (u * 5 / 4) + pad;
                let rect = Rect::new(inner_x, y, inner_w, h);
                let button_h = u * 3 / 2;
                let by = rect.y + pad / 2 + px(2);
                let button =
                    |label_w: i32, right: i32| Rect::new(right - label_w, by, label_w, button_h);
                let disconnect = button(u * 6, rect.right() - pad / 2);
                let known = state
                    .current
                    .as_deref()
                    .and_then(|c| state.network(c))
                    .is_some_and(|n| n.known_path.is_some());
                let forget = known.then(|| button(u * 4, disconnect.x - px(6)));
                card = Some(Card {
                    rect,
                    disconnect: Some(disconnect),
                    forget,
                });
                y = rect.bottom() + px(8);
            } else if joining {
                let rect = Rect::new(inner_x, y, inner_w, u * 3);
                card = Some(Card {
                    rect,
                    disconnect: None,
                    forget: None,
                });
                y = rect.bottom() + px(8);
            }

            let heading = Rect::new(inner_x, y, inner_w, u * 2);
            let r = heading.height;
            rescan = Some(Rect::new(heading.right() - r, heading.y, r, r));
            section = Some(heading);
            y = heading.bottom();

            list.y = y;
            let count = state.others().count();
            let ask_row = popup.ask_row();
            let end = (popup.scroll() + popup.rows()).min(count);
            if count == 0 {
                list.height = row_h;
                y += row_h;
            } else {
                for index in popup.scroll()..end {
                    let rect = Rect::new(list_x, y, list_w, row_h);
                    rows.push((index, rect));
                    y += row_h;
                    if ask_row == Some(index) {
                        let block = Rect::new(list_x, y, list_w, u * 3);
                        let field_h = u * 2;
                        let join_w = u * 4;
                        let fy = block.y + (block.height - field_h) / 2;
                        let join = Rect::new(block.right() - pad / 2 - join_w, fy, join_w, field_h);
                        let field = Rect::new(
                            block.x + pad / 2,
                            fy,
                            join.x - px(8) - (block.x + pad / 2),
                            field_h,
                        );
                        let reveal = Rect::new(field.right() - field_h, field.y, field_h, field_h);
                        ask = Some(AskRects {
                            block,
                            field,
                            reveal,
                            join,
                        });
                        y = block.bottom();
                    }
                }
                list.height = y - list.y;
            }
        }

        let message = popup.message().map(|_| {
            let r = Rect::new(inner_x, y + px(4), inner_w, u * 7 / 4);
            y = r.bottom();
            r
        });
        let footer = Rect::new(inner_x, y + px(4), inner_w, u * 7 / 4);
        let height = footer.bottom() + border + pad / 4;

        Self {
            scale,
            size: Size::new(
                u32::try_from(width).unwrap_or(0),
                u32::try_from(height).unwrap_or(0),
            ),
            unit: u,
            pad,
            border,
            radius: px(10),
            text_px,
            small_px,
            header,
            switch,
            notice,
            card,
            section,
            rescan,
            rows,
            ask,
            list,
            message,
            footer,
        }
    }

    /// The surface size in logical pixels, which is what the compositor is
    /// asked for.
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        (self.size.width / self.scale, self.size.height / self.scale)
    }

    /// What is under `point`.
    #[must_use]
    pub fn hit(&self, point: Point) -> Option<Target> {
        let inside = |r: &Rect| r.contains(point);
        if inside(&self.switch.inflate(self.px(4))) {
            return Some(Target::Switch);
        }
        if let Some(card) = &self.card {
            if card.disconnect.as_ref().is_some_and(inside) {
                return Some(Target::Disconnect);
            }
            if card.forget.as_ref().is_some_and(inside) {
                return Some(Target::Forget);
            }
        }
        if self.rescan.as_ref().is_some_and(inside) {
            return Some(Target::Rescan);
        }
        if let Some(ask) = &self.ask {
            if inside(&ask.join) {
                return Some(Target::Join);
            }
            if inside(&ask.reveal) {
                return Some(Target::Reveal);
            }
            if inside(&ask.block) {
                return Some(Target::Field);
            }
        }
        self.rows
            .iter()
            .find(|(_, r)| inside(r))
            .map(|(i, _)| Target::Row(*i))
    }

    fn px(&self, logical: i32) -> i32 {
        logical * i32::try_from(self.scale).unwrap_or(1)
    }
}

fn colour(Colour([red, green, blue, alpha]): Colour) -> Color {
    Color::rgba(red, green, blue, alpha)
}

/// `a` moved towards `b` by `percent`, alpha included.
fn mix(a: Colour, b: Colour, percent: u16) -> Color {
    let p = percent.min(100);
    let m = |x: u8, y: u8| {
        u8::try_from((u16::from(x) * (100 - p) + u16::from(y) * p) / 100).unwrap_or(u8::MAX)
    };
    let (Colour(a), Colour(b)) = (a, b);
    Color::rgba(m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3]))
}

/// The warning colour the menu uses for its notices.
const WARN: Color = Color::rgb(0xE8, 0xB0, 0x6A);

/// Where text sits to be centred vertically in a box at `y` of `h`.
fn text_top(engine: &TextEngine, style: TextStyle, y: i32, h: i32) -> i32 {
    y + (h - engine.line_height(style)) / 2
}

struct Ink {
    text: Color,
    dim: Color,
    accent: Color,
    selection: Color,
    card: Color,
    on_accent: Color,
}

struct Styles {
    text: TextStyle,
    strong: TextStyle,
    small: TextStyle,
    icon: TextStyle,
    icon_small: TextStyle,
}

/// Paint the whole popup.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    popup: &Popup,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = canvas.pen();
    let full = Rect::from_size(layout.size);
    let ink = Ink {
        text: colour(appearance.text),
        dim: colour(appearance.dim),
        accent: colour(appearance.accent),
        selection: colour(appearance.selection),
        card: mix(appearance.background, appearance.selection, 45),
        on_accent: {
            let Colour([r, g, b, _]) = appearance.background;
            Color::rgb(r, g, b)
        },
    };
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
        text,
        strong,
        icons,
        ..
    } = fonts;
    let st = Styles {
        text: TextStyle {
            font: *text,
            size_px: layout.text_px,
        },
        strong: TextStyle {
            font: *strong,
            size_px: layout.text_px,
        },
        small: TextStyle {
            font: *text,
            size_px: layout.small_px,
        },
        icon: TextStyle {
            font: *icons,
            size_px: layout.text_px,
        },
        icon_small: TextStyle {
            font: *icons,
            size_px: layout.small_px,
        },
    };
    let state = popup.state();
    let u = layout.unit;
    let hover = popup.hover();

    // Header: an icon, the title, the interface, and the switch.
    let h = layout.header;
    let on = state.radio == Radio::On;
    let icon = if on { bar::BARS[4] } else { bar::OFF };
    let top = text_top(engine, st.icon, h.y, h.height);
    engine.draw(
        &mut pen,
        st.icon,
        Point::new(h.x, top),
        icon,
        if on { ink.accent } else { ink.dim },
    );
    let tx = h.x + u * 3 / 2;
    let top = text_top(engine, st.strong, h.y, h.height);
    let w = engine
        .draw(&mut pen, st.strong, Point::new(tx, top), "Wi-Fi", ink.text)
        .width;
    if let Some(device) = &state.device {
        let top = text_top(engine, st.small, h.y, h.height);
        engine.draw(
            &mut pen,
            st.small,
            Point::new(
                tx + i32::try_from(w).unwrap_or(0) + u / 2,
                top + layout.px(1),
            ),
            device,
            ink.dim,
        );
    }
    if matches!(state.radio, Radio::On | Radio::Off) {
        let switching = match popup.busy() {
            Some(Busy::Switching(to)) => Some(*to),
            _ => None,
        };
        let lit = switching.unwrap_or(on);
        let r = layout.switch;
        let track = if lit { ink.accent } else { ink.selection };
        pen.fill_rounded_rect(r, r.height / 2, track);
        if hover == Some(Target::Switch) {
            pen.stroke_rounded_rect(r, r.height / 2, layout.px(1), ink.text);
        }
        let knob_r = r.height / 2 - layout.px(3);
        let cx = if lit {
            r.right() - r.height / 2
        } else {
            r.x + r.height / 2
        };
        let knob = if lit { ink.on_accent } else { ink.text };
        pen.fill_circle(Point::new(cx, r.y + r.height / 2), knob_r, knob);
    }

    // No list to show: say why.
    if let Some(r) = layout.notice {
        let (line, colour) = if popup.loaded() {
            match (state.radio, popup.busy()) {
                (_, Some(Busy::Switching(true))) => ("Switching Wi-Fi on…", ink.dim),
                (Radio::NoDaemon, _) => ("iwd is not running.", WARN),
                (Radio::NoAdapter, _) => ("There is no Wi-Fi adapter.", WARN),
                _ => ("Wi-Fi is off.", ink.dim),
            }
        } else {
            ("Asking iwd…", ink.dim)
        };
        let top = text_top(engine, st.text, r.y, r.height);
        engine.draw(&mut pen, st.text, Point::new(r.x, top), line, colour);
    }

    if let Some(card) = &layout.card {
        paint_card(&mut pen, engine, &st, &ink, layout, popup, card);
    }

    if let Some(r) = layout.section {
        let top = text_top(engine, st.small, r.y, r.height);
        engine.draw(
            &mut pen,
            st.small,
            Point::new(r.x, top),
            if state.scanning {
                "Looking for networks…"
            } else {
                "Networks"
            },
            ink.dim,
        );
        if let Some(b) = layout.rescan
            && !state.scanning
        {
            if hover == Some(Target::Rescan) {
                pen.fill_rounded_rect(b, layout.px(6), ink.selection);
            }
            let w = engine.measure_line(st.icon_small, RESCAN);
            let top = text_top(engine, st.icon_small, b.y, b.height);
            engine.draw(
                &mut pen,
                st.icon_small,
                Point::new(b.x + (b.width - w) / 2, top),
                RESCAN,
                ink.dim,
            );
        }
    }

    // Rows.
    if layout.section.is_some() && layout.rows.is_empty() {
        let r = layout.list;
        let line = if state.scanning {
            "Looking…"
        } else {
            "No other networks in range."
        };
        let top = text_top(engine, st.small, r.y, r.height);
        engine.draw(
            &mut pen,
            st.small,
            Point::new(r.x + layout.pad / 2, top),
            line,
            ink.dim,
        );
    }
    let networks: Vec<_> = state.others().collect();
    for &(index, cell) in &layout.rows {
        let Some(network) = networks.get(index) else {
            continue;
        };
        let asking = popup.ask_row() == Some(index);
        if popup.selected() == Some(index) || asking {
            pen.fill_rounded_rect(cell, layout.px(6), ink.selection);
        }
        let mut clip = pen.with_clip(cell);
        let x = cell.x + layout.pad / 2;
        let bars = bar::BARS[usize::from(network.bars().min(4))];
        let top = text_top(engine, st.icon, cell.y, cell.height);
        engine.draw(
            &mut clip,
            st.icon,
            Point::new(x, top),
            bars,
            if network.known { ink.accent } else { ink.text },
        );
        let name_x = x + u * 3 / 2;
        let mut right = cell.right() - layout.pad / 2;
        if network.security.secured() {
            let w = engine.measure_line(st.icon_small, LOCK);
            right -= w;
            let t = text_top(engine, st.icon_small, cell.y, cell.height);
            engine.draw(
                &mut clip,
                st.icon_small,
                Point::new(right, t),
                LOCK,
                ink.dim,
            );
            right -= layout.pad / 2;
        }
        let note = match (popup.busy(), network.security) {
            (Some(Busy::Joining(name)), _) if *name == network.name => Some("Joining…"),
            (_, Security::Enterprise) if !network.known => Some("Enterprise"),
            _ if network.known => Some("Saved"),
            _ => None,
        };
        if let Some(note) = note {
            let w = engine.measure_line(st.small, note);
            right -= w;
            let t = text_top(engine, st.small, cell.y, cell.height);
            engine.draw(&mut clip, st.small, Point::new(right, t), note, ink.dim);
            right -= layout.pad / 2;
        }
        let mut name_clip = clip.with_clip(Rect::new(
            name_x,
            cell.y,
            (right - name_x).max(0),
            cell.height,
        ));
        let top = text_top(engine, st.text, cell.y, cell.height);
        engine.draw(
            &mut name_clip,
            st.text,
            Point::new(name_x, top),
            &network.name,
            ink.text,
        );
    }

    if let (Some(rects), Some(ask)) = (layout.ask, popup.ask()) {
        paint_ask(&mut pen, engine, &st, &ink, layout, popup, rects, ask);
    }

    // A scrollbar, only when there is something to scroll.
    let total = networks.len();
    if total > popup.rows() && !layout.rows.is_empty() {
        let track_h = layout.list.height;
        let track_x = layout.list.right() + (layout.pad / 2 - layout.px(3)) / 2;
        let len = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let thumb_h = (track_h * len(popup.rows()) / len(total)).max(layout.px(12));
        let thumb_y =
            layout.list.y + (track_h - thumb_h) * len(popup.scroll()) / len(total - popup.rows());
        pen.fill_rounded_rect(
            Rect::new(track_x, thumb_y, layout.px(3), thumb_h),
            layout.px(2),
            ink.selection,
        );
    }

    if let (Some(r), Some(message)) = (layout.message, popup.message()) {
        let top = text_top(engine, st.small, r.y, r.height);
        let mut clip = pen.with_clip(r);
        engine.draw(
            &mut clip,
            st.small,
            Point::new(r.x, top),
            &message.text,
            if message.error { WARN } else { ink.accent },
        );
    }

    if popup.focus_visible()
        && let Some((rect, radius)) = focus_rect(layout, popup)
    {
        let ring = rect.inflate(layout.px(3));
        pen.stroke_rounded_rect(ring, radius + layout.px(3), layout.px(2), ink.text);
    }

    let foot = layout.footer;
    let hints = if popup.ask().is_some() {
        "tab next    enter join    esc cancel"
    } else if on && state.others().next().is_some() {
        "tab move    ↑↓ choose    enter join    esc close"
    } else if matches!(state.radio, Radio::On | Radio::Off) {
        "tab move    space switch    esc close"
    } else {
        "esc close"
    };
    let top = text_top(engine, st.small, foot.y, foot.height);
    let mut clip = pen.with_clip(foot);
    engine.draw(&mut clip, st.small, Point::new(foot.x, top), hints, ink.dim);
}

/// The rectangle the focus ring goes round, and its corner radius.
fn focus_rect(layout: &Layout, popup: &Popup) -> Option<(Rect, i32)> {
    let pill = |r: Rect| (r, r.height / 2);
    match popup.focus() {
        Focus::Switch => Some(pill(layout.switch)),
        Focus::Forget => layout.card.and_then(|c| c.forget).map(pill),
        Focus::Disconnect => layout.card.and_then(|c| c.disconnect).map(pill),
        Focus::Rescan => layout.rescan.map(|r| (r, layout.px(6))),
        Focus::List => {
            let selected = popup.selected()?;
            layout
                .rows
                .iter()
                .find(|(i, _)| *i == selected)
                .map(|(_, r)| (*r, layout.px(6)))
        }
        Focus::Field => layout.ask.map(|a| (a.field, layout.px(6))),
        Focus::Reveal => layout.ask.map(|a| (a.reveal, layout.px(6))),
        Focus::Join => layout.ask.map(|a| pill(a.join)),
    }
}

fn button(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    label: &str,
    (fill, text): (Option<Color>, Color),
    outline: Option<(i32, Color)>,
) {
    if let Some(fill) = fill {
        pen.fill_rounded_rect(r, r.height / 2, fill);
    }
    if let Some((width, colour)) = outline {
        pen.stroke_rounded_rect(r, r.height / 2, width, colour);
    }
    let w = engine.measure_line(style, label);
    let top = text_top(engine, style, r.y, r.height);
    engine.draw(
        pen,
        style,
        Point::new(r.x + (r.width - w) / 2, top),
        label,
        text,
    );
}

#[allow(clippy::too_many_lines)]
fn paint_card(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    st: &Styles,
    ink: &Ink,
    layout: &Layout,
    popup: &Popup,
    card: &Card,
) {
    let state = popup.state();
    let u = layout.unit;
    let r = card.rect;
    let hover = popup.hover();
    pen.fill_rounded_rect(r, layout.px(8), ink.card);
    let x = r.x + layout.pad;

    let Some(joined) = &state.link else {
        // Joining: a line and nothing else to show yet.
        let name = match popup.busy() {
            Some(Busy::Joining(name)) => Some(name.as_str()),
            _ => state.current.as_deref(),
        };
        let line = format!("Joining {}…", name.unwrap_or("a network"));
        let top = text_top(engine, st.icon, r.y, r.height);
        engine.draw(
            pen,
            st.icon,
            Point::new(x, top),
            bar::CONNECTING,
            ink.accent,
        );
        let top = text_top(engine, st.text, r.y, r.height);
        let mut clip = pen.with_clip(r);
        engine.draw(
            &mut clip,
            st.text,
            Point::new(x + u * 3 / 2, top),
            &line,
            ink.text,
        );
        return;
    };

    // The name line, with the buttons on its right.
    let line_y = r.y + layout.pad / 2;
    let line_h = u * 3 / 2 + layout.px(4);
    let bars = bar::BARS[usize::from(state.link_bars().unwrap_or(0).min(4))];
    let top = text_top(engine, st.icon, line_y, line_h);
    engine.draw(pen, st.icon, Point::new(x, top), bars, ink.accent);
    let name_x = x + u * 3 / 2;
    let name_right = card
        .forget
        .or(card.disconnect)
        .map_or(r.right() - layout.pad, |b| b.x - layout.pad / 2);
    {
        let mut clip = pen.with_clip(Rect::new(
            name_x,
            line_y,
            (name_right - name_x).max(0),
            line_h,
        ));
        let top = text_top(engine, st.strong, line_y, line_h);
        engine.draw(
            &mut clip,
            st.strong,
            Point::new(name_x, top),
            &joined.name,
            ink.text,
        );
    }
    let leaving = matches!(popup.busy(), Some(Busy::Disconnecting));
    if let Some(b) = card.disconnect {
        let lit = hover == Some(Target::Disconnect);
        button(
            pen,
            engine,
            st.small,
            b,
            if leaving { "Leaving…" } else { "Disconnect" },
            (lit.then_some(ink.selection), ink.text),
            Some((layout.px(1), ink.dim)),
        );
    }
    if let Some(b) = card.forget {
        let lit = hover == Some(Target::Forget);
        button(
            pen,
            engine,
            st.small,
            b,
            "Forget",
            (lit.then_some(ink.selection), ink.text),
            Some((layout.px(1), ink.dim)),
        );
    }

    // The address, under the name.
    let mut y = line_y + line_h;
    let address = joined.ipv4.as_deref().unwrap_or("No address yet");
    let small_h = u * 5 / 4;
    let top = text_top(engine, st.small, y, small_h);
    engine.draw(pen, st.small, Point::new(name_x, top), address, ink.dim);
    y += small_h + layout.px(8);

    // Details in two columns: label dim, value bright.
    let mut details: Vec<(&str, String)> = Vec::new();
    if let Some(dbm) = joined.rssi_dbm {
        details.push(("Signal", format!("{dbm} dBm")));
    }
    if let Some(f) = joined.frequency_mhz {
        let channel = joined
            .channel
            .map(|c| format!(" · ch {c}"))
            .unwrap_or_default();
        details.push(("Band", format!("{}{channel}", band(f))));
    }
    if let Some(sec) = &joined.security {
        details.push((
            "Security",
            sec.replace("-Personal", "").replace("-Enterprise", " Ent."),
        ));
    }
    if let Some(rate) = joined.rx_mbit.or(joined.tx_mbit) {
        details.push(("Speed", format!("{rate} Mbit/s")));
    }
    let col_w = (r.width - 2 * layout.pad) / 2;
    let label_w = (0..details.len())
        .step_by(2)
        .filter_map(|i| details.get(i))
        .map(|(l, _)| engine.measure_line(st.small, l))
        .max()
        .unwrap_or(0)
        .max(
            (1..details.len())
                .step_by(2)
                .filter_map(|i| details.get(i))
                .map(|(l, _)| engine.measure_line(st.small, l))
                .max()
                .unwrap_or(0),
        );
    for (i, (label, value)) in details.iter().enumerate() {
        let column = i32::try_from(i % 2).unwrap_or(0);
        let row = i32::try_from(i / 2).unwrap_or(0);
        let cx = x + column * col_w;
        let cy = y + row * small_h;
        let top = text_top(engine, st.small, cy, small_h);
        let mut clip = pen.with_clip(Rect::new(cx, cy, col_w - layout.px(4), small_h));
        engine.draw(&mut clip, st.small, Point::new(cx, top), label, ink.dim);
        engine.draw(
            &mut clip,
            st.small,
            Point::new(cx + label_w + u / 2, top),
            value,
            ink.text,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_ask(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    st: &Styles,
    ink: &Ink,
    layout: &Layout,
    popup: &Popup,
    rects: AskRects,
    ask: &crate::popup::Ask,
) {
    let f = rects.field;
    pen.fill_rounded_rect(rects.block, layout.px(6), ink.selection);
    pen.fill_rounded_rect(f, layout.px(6), ink.card);
    pen.stroke_rounded_rect(f, layout.px(6), layout.px(1), ink.accent);

    let text_x = f.x + layout.pad / 2;
    let room = Rect::new(text_x, f.y, (rects.reveal.x - text_x).max(0), f.height);
    let top = text_top(engine, st.text, f.y, f.height);
    {
        let mut clip = pen.with_clip(room);
        let shown: String = if ask.reveal {
            ask.passphrase.clone()
        } else {
            "•".repeat(ask.passphrase.chars().count())
        };
        let w = engine.measure_line(st.text, &shown);
        // Long passphrases scroll left, so the end, where typing happens, shows.
        let x = text_x.min(room.right() - w - layout.px(4));
        if shown.is_empty() {
            engine.draw(
                &mut clip,
                st.text,
                Point::new(text_x + layout.px(4), top),
                "Passphrase",
                ink.dim,
            );
        } else {
            engine.draw(&mut clip, st.text, Point::new(x, top), &shown, ink.text);
        }
        let caret_x = if shown.is_empty() {
            text_x
        } else {
            x + w + layout.px(1)
        };
        // The caret only where typing goes.
        if popup.focus() == Focus::Field {
            clip.fill_rect(
                Rect::new(
                    caret_x,
                    top + layout.px(2),
                    layout.px(2),
                    engine.line_height(st.text) - layout.px(4),
                ),
                ink.accent,
            );
        }
    }
    let eye = if ask.reveal { EYE_OFF } else { EYE };
    let w = engine.measure_line(st.icon_small, eye);
    let t = text_top(engine, st.icon_small, f.y, f.height);
    engine.draw(
        pen,
        st.icon_small,
        Point::new(rects.reveal.x + (rects.reveal.width - w) / 2, t),
        eye,
        if popup.hover() == Some(Target::Reveal) {
            ink.text
        } else {
            ink.dim
        },
    );

    let joining = matches!(popup.busy(), Some(Busy::Joining(_)));
    let lit = popup.hover() == Some(Target::Join);
    button(
        pen,
        engine,
        st.small,
        rects.join,
        if joining { "…" } else { "Join" },
        (Some(if lit { ink.text } else { ink.accent }), ink.on_accent),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::Layout;
    use crate::model::sample;
    use crate::popup::{Key, Popup, Target};
    use alpymist_menu::config::Appearance;
    use denise::geom::Point;

    fn centre(r: denise::geom::Rect) -> Point {
        Point::new(r.x + r.width / 2, r.y + r.height / 2)
    }

    fn popup() -> Popup {
        let mut p = Popup::new(super::ROWS);
        p.update(sample());
        p
    }

    #[test]
    fn everything_is_hit_where_it_is_drawn() {
        let a = Appearance::default();
        let p = popup();
        let l = Layout::new(&a, &p, 1);
        assert_eq!(l.hit(centre(l.switch)), Some(Target::Switch));
        let card = l.card.unwrap();
        assert_eq!(
            l.hit(centre(card.disconnect.unwrap())),
            Some(Target::Disconnect)
        );
        assert_eq!(l.hit(centre(card.forget.unwrap())), Some(Target::Forget));
        for (i, r) in &l.rows {
            assert_eq!(l.hit(centre(*r)), Some(Target::Row(*i)));
        }
    }

    #[test]
    fn asking_for_a_passphrase_makes_room_under_its_row() {
        let a = Appearance::default();
        let mut p = popup();
        let before = Layout::new(&a, &p, 1);
        p.key(Key::Down);
        p.key(Key::Down);
        p.key(Key::Enter);
        let after = Layout::new(&a, &p, 1);
        let ask = after.ask.expect("a field");
        assert!(after.size.height > before.size.height);
        assert_eq!(after.hit(centre(ask.join)), Some(Target::Join));
        let (_, row) = after.rows[1];
        assert_eq!(ask.block.y, row.bottom());
    }

    #[test]
    fn the_layout_scales_with_the_output_and_fits() {
        let a = Appearance::default();
        let p = popup();
        let one = Layout::new(&a, &p, 1);
        let two = Layout::new(&a, &p, 2);
        assert_eq!(two.logical_size(), one.logical_size());
        assert!(one.footer.bottom() <= i32::try_from(one.size.height).unwrap());
    }
}

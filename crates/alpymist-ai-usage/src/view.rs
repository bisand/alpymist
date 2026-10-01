//! Laying out and painting the popup.
//!
//! One panel, top to bottom: the icon, a line of status and the figure the
//! bar shows; each provider turned on — its name and plan, each meter as a
//! label, what it says and a bar where it has a limit, and what there is to
//! say about its age; two buttons; and a footer of keys. How many providers
//! and meters there are varies, so the layout is worked out from the popup
//! each time, and the same [`Layout`] answers where a click landed.

use crate::bar;
use crate::popup::{Popup, Target};
use crate::report::percent;
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Ink, Metrics};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise_render::Canvas;

/// Popup width in logical pixels, at a 16 px font.
const WIDTH: i32 = 400;

pub use alpymist_widget::draw::Fonts;

/// Where one meter goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// Its label and what it says.
    pub text: Rect,
    /// Its bar, where it has a limit.
    pub meter: Option<Rect>,
}

/// Where one provider goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The card behind it.
    pub card: Rect,
    /// Its name and plan.
    pub name: Rect,
    /// Its meters.
    pub rows: Vec<Row>,
    /// Its notes, a line each.
    pub notes: Vec<Rect>,
}

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The sizes everything is measured in.
    pub metrics: Metrics,
    /// The whole panel.
    pub size: Size,
    /// Icon, title, status and figure.
    pub header: Rect,
    /// Each provider, in the reading's order.
    pub blocks: Vec<Block>,
    /// What is said instead, with nothing turned on.
    pub empty: Option<Rect>,
    /// The buttons.
    pub buttons: Vec<(Target, Rect)>,
    /// The footer.
    pub footer: Rect,
}

impl Layout {
    /// Lay out `popup` at an output scale.
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn new(appearance: &Appearance, popup: &Popup, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let u = m.unit;
        let x = m.inner_x();
        let w = m.inner_w();
        let inset = m.pad / 2 + m.px(2);

        let header = Rect::new(x, m.border + m.pad / 2, w, u * 3);
        let mut y = header.bottom() + m.px(6);

        let mut blocks = Vec::new();
        for provider in &popup.reading().providers {
            let top = y;
            let (tx, tw) = (x + inset, w - 2 * inset);
            y += m.pad / 4;
            let name = Rect::new(tx, y, tw, u * 7 / 4);
            y = name.bottom();
            let rows = provider
                .lines
                .iter()
                .map(|line| {
                    let text = Rect::new(tx, y, tw, u * 3 / 2);
                    y = text.bottom();
                    let meter = line.used.map(|_| {
                        let r = Rect::new(tx, y, tw, m.px(4));
                        y = r.bottom() + m.px(6);
                        r
                    });
                    Row { text, meter }
                })
                .collect();
            let notes = provider
                .notes
                .iter()
                .map(|_| {
                    let r = Rect::new(tx, y, tw, u * 5 / 4);
                    y = r.bottom();
                    r
                })
                .collect();
            y += m.pad / 2;
            blocks.push(Block {
                card: Rect::new(x, top, w, y - top),
                name,
                rows,
                notes,
            });
            y += m.px(8);
        }

        let empty = blocks.is_empty().then(|| {
            let r = Rect::new(x, y, w, u * 3);
            y = r.bottom() + m.px(8);
            r
        });

        let shown = popup.buttons();
        let gap = m.px(8);
        let count = i32::try_from(shown.len()).unwrap_or(1).max(1);
        let each = (w - gap * (count - 1)) / count;
        let mut bx = x;
        let buttons = shown
            .into_iter()
            .map(|target| {
                let r = Rect::new(bx, y + m.px(2), each, u * 2);
                bx = r.right() + gap;
                (target, r)
            })
            .collect::<Vec<_>>();
        y = buttons.first().map_or(y, |(_, r)| r.bottom());

        let footer = Rect::new(x, y + m.px(4), w, u * 7 / 4);
        Self {
            metrics: m,
            size: m.size(footer.bottom()),
            header,
            blocks,
            empty,
            buttons,
            footer,
        }
    }

    /// What is under `point`.
    #[must_use]
    pub fn hit(&self, point: Point) -> Option<Target> {
        self.buttons
            .iter()
            .find(|(_, r)| r.contains(point))
            .map(|(t, _)| *t)
    }
}

/// Paint the whole popup.
#[allow(clippy::too_many_lines)]
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    popup: &Popup,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = canvas.pen();
    let m = &layout.metrics;
    let u = m.unit;
    let ink = Ink::new(appearance);
    let st = fonts.styles(m);
    draw::panel(&mut pen, layout.size, m, &ink);
    let reading = popup.reading();

    // Header: the icon, what is closest to its limit, and how close.
    let h = layout.header;
    let worst = reading.worst();
    let alarm = worst.is_some_and(|(_, _, used)| reading.warns(used));
    let icon_box = Rect::new(h.x - m.px(4), h.y, u * 2, h.height);
    draw::centred(
        &mut pen,
        &mut fonts.engine,
        st.icon_large,
        icon_box,
        bar::ICON,
        if alarm { ink.warn } else { ink.accent },
    );
    let tx = icon_box.right() + u / 2;
    let figure = worst.map(|(_, _, used)| percent(used));
    let figure_x = figure.as_deref().map_or(h.right(), |f| {
        draw::right_label(&mut pen, &mut fonts.engine, st.large, h, f, ink.text)
    });
    let text_w = (figure_x - tx - u / 2).max(0);
    let line_h = h.height / 2;
    draw::label(
        &mut pen,
        &mut fonts.engine,
        st.strong,
        Rect::new(tx, h.y + m.px(2), text_w, line_h),
        "AI usage",
        ink.text,
    );
    draw::label(
        &mut pen,
        &mut fonts.engine,
        st.small,
        Rect::new(tx, h.y + line_h - m.px(2), text_w, line_h),
        &popup.status(),
        ink.dim,
    );

    for (provider, block) in reading.providers.iter().zip(&layout.blocks) {
        draw::card(&mut pen, block.card, m, &ink);
        let plan_x = provider.plan.as_deref().map_or(block.name.right(), |plan| {
            draw::right_label(
                &mut pen,
                &mut fonts.engine,
                st.small,
                block.name,
                plan,
                ink.dim,
            )
        });
        let name = Rect::new(
            block.name.x,
            block.name.y,
            (plan_x - block.name.x - u / 2).max(0),
            block.name.height,
        );
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.strong,
            name,
            &provider.name,
            ink.text,
        );
        for (line, row) in provider.lines.iter().zip(&block.rows) {
            let says_x = draw::right_label(
                &mut pen,
                &mut fonts.engine,
                st.small,
                row.text,
                &line.says,
                ink.text,
            );
            let label = Rect::new(
                row.text.x,
                row.text.y,
                (says_x - row.text.x - u / 2).max(0),
                row.text.height,
            );
            draw::label(
                &mut pen,
                &mut fonts.engine,
                st.small,
                label,
                &line.label,
                ink.dim,
            );
            if let (Some(r), Some(used)) = (row.meter, line.used) {
                let fill = if reading.warns(used) {
                    ink.warn
                } else {
                    ink.accent
                };
                draw::meter(&mut pen, r, used, fill, ink.selection);
            }
        }
        for (note, r) in provider.notes.iter().zip(&block.notes) {
            let colour = if provider.failed { ink.warn } else { ink.dim };
            draw::label(&mut pen, &mut fonts.engine, st.small, *r, note, colour);
        }
    }

    if let Some(r) = layout.empty {
        let line_h = r.height / 2;
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.text,
            Rect::new(r.x, r.y, r.width, line_h),
            "No AI provider is turned on.",
            ink.text,
        );
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.small,
            Rect::new(r.x, r.y + line_h, r.width, line_h),
            "Turn one on in Settings.",
            ink.dim,
        );
    }

    for (target, r) in &layout.buttons {
        let label = match target {
            Target::Refresh if popup.asking() => "Asking…",
            Target::Refresh => "Ask again",
            Target::Settings => "Settings…",
        };
        draw::outline_button(
            &mut pen,
            &mut fonts.engine,
            st.text,
            *r,
            label,
            popup.hover() == Some(*target),
            m,
            &ink,
        );
        if popup.focus_visible() && popup.focus() == *target {
            draw::focus_ring(&mut pen, *r, r.height / 2, m, &ink);
        }
    }

    let hints = if popup.focus_visible() {
        "enter press    tab move    esc close"
    } else {
        "tab move    esc close"
    };
    draw::hints(
        &mut pen,
        &mut fonts.engine,
        st.small,
        layout.footer,
        hints,
        &ink,
    );
}

#[cfg(test)]
mod tests {
    use super::Layout;
    use crate::popup::{Popup, Reading, Target, sample};
    use alpymist_widget::Appearance;
    use denise::geom::{Point, Rect};

    fn centre(r: Rect) -> Point {
        Point::new(r.x + r.width / 2, r.y + r.height / 2)
    }

    fn popup() -> Popup {
        let mut p = Popup::new();
        p.update(sample());
        p
    }

    #[test]
    fn everything_is_hit_where_it_is_drawn() {
        let l = Layout::new(&Appearance::default(), &popup(), 1);
        assert_eq!(l.buttons.len(), 2);
        for (t, r) in &l.buttons {
            assert_eq!(l.hit(centre(*r)), Some(*t));
        }
        assert_eq!(l.hit(centre(l.header)), None);
        assert_eq!(l.blocks.len(), 3);
        assert!(l.blocks[0].rows.iter().all(|r| r.meter.is_some()));
        assert!(l.blocks[2].rows[0].meter.is_none(), "spend has no limit");
        assert_eq!(l.blocks[2].notes.len(), 2);
        // Each provider below the last, and everything inside its card.
        for pair in l.blocks.windows(2) {
            assert!(pair[1].card.y >= pair[0].card.bottom());
        }
        for b in &l.blocks {
            let last = b.notes.last().map_or(b.name.bottom(), Rect::bottom);
            assert!(last <= b.card.bottom());
        }
    }

    #[test]
    fn nothing_turned_on_says_so_and_offers_settings() {
        let mut p = Popup::new();
        p.update(Reading::default());
        let l = Layout::new(&Appearance::default(), &p, 1);
        assert!(l.empty.is_some());
        assert_eq!(l.buttons.len(), 1);
        assert_eq!(l.hit(centre(l.buttons[0].1)), Some(Target::Settings));
    }

    #[test]
    fn the_layout_scales_and_fits() {
        let a = Appearance::default();
        let p = popup();
        let one = Layout::new(&a, &p, 1);
        let two = Layout::new(&a, &p, 2);
        assert_eq!(two.size.width, one.size.width * 2);
        assert_eq!(two.size.height, one.size.height * 2);
        assert!(one.footer.bottom() <= i32::try_from(one.size.height).unwrap());
    }
}

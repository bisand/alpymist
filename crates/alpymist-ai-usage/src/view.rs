//! Laying out and painting the popup.
//!
//! One panel, top to bottom: the icon, a line of status and the figure the
//! bar shows; every provider installed, each a card with its name and a
//! switch — and, turned on, each meter as a label, what it says and a bar
//! where it has a limit, and what there is to say about its age; under the
//! one being set up, a field for its key or a button to install its tool;
//! the two settings worth having at hand; a line for what just went wrong;
//! two buttons; and a footer of keys. What is there varies, so the layout is
//! worked out from the popup each time, and the same [`Layout`] answers
//! where a click landed.

use crate::bar;
use crate::popup::{Ask, Popup, Setup, Target};
use crate::report::percent;
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Ink, Metrics, Styles, text_top};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;

/// Popup width in logical pixels, at a 16 px font.
const WIDTH: i32 = 420;

/// An eye, and one struck through, from Nerd Font's Material Design set.
const EYE: &str = "\u{f0208}";
const EYE_OFF: &str = "\u{f0209}";

pub use alpymist_widget::draw::Fonts;

/// Where one meter goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// Its label and what it says.
    pub text: Rect,
    /// Its bar, where it has a limit.
    pub meter: Option<Rect>,
}

/// Where a provider's setup goes, under its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupRects {
    /// What is asked for.
    pub label: Rect,
    /// The key field, when a key is asked for.
    pub field: Option<Rect>,
    /// The eye in the field.
    pub reveal: Option<Rect>,
    /// Keep the key, or install the tool.
    pub action: Rect,
}

/// Where one provider goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The card behind it.
    pub card: Rect,
    /// Its name, and its plan beside it.
    pub name: Rect,
    /// Its switch.
    pub switch: Rect,
    /// The button for its keys, when it has one.
    pub keys: Option<Rect>,
    /// Its meters.
    pub rows: Vec<Row>,
    /// Its notes, a line each.
    pub notes: Vec<Rect>,
    /// What it is waiting for, when it is being set up.
    pub setup: Option<SetupRects>,
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
    /// What is said instead, with no provider installed.
    pub empty: Option<Rect>,
    /// The notification check box.
    pub notify: Option<Rect>,
    /// The warning's stepper.
    pub warn: Option<Rect>,
    /// The message, a line each.
    pub message: Vec<Rect>,
    /// The buttons.
    pub buttons: Vec<(Target, Rect)>,
    /// The footer.
    pub footer: Rect,
}

impl Layout {
    /// Lay out `popup` at an output scale.
    #[must_use]
    #[allow(clippy::many_single_char_names, clippy::too_many_lines)]
    pub fn new(appearance: &Appearance, popup: &Popup, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let u = m.unit;
        let x = m.inner_x();
        let w = m.inner_w();
        let inset = m.pad / 2 + m.px(2);
        let (tx, tw) = (x + inset, w - 2 * inset);
        let (switch_w, switch_h) = draw::switch_size(&m);
        let row_h = u * 2;

        let header = Rect::new(x, m.border + m.pad / 2, w, u * 3);
        let mut y = header.bottom() + m.px(6);

        let setup_at = popup.setup_at();
        let mut blocks = Vec::new();
        for (index, provider) in popup.reading().providers.iter().enumerate() {
            let top = y;
            y += m.px(2);
            let line = Rect::new(tx, y, tw, row_h);
            let switch = Rect::new(
                line.right() - switch_w,
                line.y + (row_h - switch_h) / 2,
                switch_w,
                switch_h,
            );
            let keys = popup.has_keys_button(index).then(|| {
                let (kw, kh) = (u * 7 / 2, u * 3 / 2);
                Rect::new(switch.x - u / 2 - kw, line.y + (row_h - kh) / 2, kw, kh)
            });
            let name_right = keys.map_or(switch.x, |k| k.x) - u / 2;
            let name = Rect::new(tx, y, (name_right - tx).max(0), row_h);
            y = line.bottom();

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
                .collect::<Vec<_>>();
            let notes = provider
                .notes
                .iter()
                .map(|_| {
                    let r = Rect::new(tx, y, tw, u * 5 / 4);
                    y = r.bottom();
                    r
                })
                .collect::<Vec<_>>();

            let setup = (setup_at == Some(index)).then(|| {
                let label = Rect::new(tx, y, tw, u * 3 / 2);
                y = label.bottom();
                let control = Rect::new(tx, y, tw, row_h);
                y = control.bottom();
                match popup.setup() {
                    Some(Setup::Ask(_)) => {
                        let save_w = u * 4;
                        let field = Rect::new(tx, control.y, tw - save_w - u / 2, row_h);
                        SetupRects {
                            label,
                            field: Some(field),
                            reveal: Some(Rect::new(field.right() - row_h, field.y, row_h, row_h)),
                            action: Rect::new(control.right() - save_w, control.y, save_w, row_h),
                        }
                    }
                    _ => SetupRects {
                        label,
                        field: None,
                        reveal: None,
                        action: control,
                    },
                }
            });
            if !rows.is_empty() || !notes.is_empty() || setup.is_some() {
                y += m.pad / 2;
            }
            y += m.px(2);
            blocks.push(Block {
                card: Rect::new(x, top, w, y - top),
                name,
                switch,
                keys,
                rows,
                notes,
                setup,
            });
            y += m.px(6);
        }

        let empty = blocks.is_empty().then(|| {
            let r = Rect::new(x, y, w, u * 3);
            y = r.bottom() + m.px(6);
            r
        });

        let list_x = m.border + m.pad / 2;
        let list_w = m.width - 2 * m.border - m.pad;
        let (mut notify, mut warn) = (None, None);
        if !blocks.is_empty() {
            let r = Rect::new(list_x, y, list_w, row_h);
            notify = Some(r);
            let r = Rect::new(list_x, r.bottom(), list_w, row_h);
            warn = Some(r);
            y = r.bottom();
        }

        let message = popup
            .message()
            .into_iter()
            .flat_map(|message| &message.lines)
            .map(|_| {
                let r = Rect::new(x, y, w, u * 5 / 4);
                y = r.bottom();
                r
            })
            .collect::<Vec<_>>();

        let shown: Vec<Target> = if blocks.is_empty() {
            vec![Target::Settings]
        } else {
            vec![Target::Refresh, Target::Settings]
        };
        let gap = m.px(8);
        let count = i32::try_from(shown.len()).unwrap_or(1).max(1);
        let each = (w - gap * (count - 1)) / count;
        let mut bx = x;
        let buttons = shown
            .into_iter()
            .map(|target| {
                let r = Rect::new(bx, y + m.px(6), each, row_h);
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
            notify,
            warn,
            message,
            buttons,
            footer,
        }
    }

    /// What is under `point`.
    #[must_use]
    pub fn hit(&self, point: Point) -> Option<Target> {
        let inside = |r: &Rect| r.contains(point);
        for (index, block) in self.blocks.iter().enumerate() {
            if inside(&block.switch) {
                return Some(Target::Switch(index));
            }
            if block.keys.as_ref().is_some_and(inside) {
                return Some(Target::Keys(index));
            }
            if let Some(setup) = &block.setup {
                if inside(&setup.action) {
                    return Some(if setup.field.is_some() {
                        Target::Save
                    } else {
                        Target::Install
                    });
                }
                if setup.reveal.as_ref().is_some_and(inside) {
                    return Some(Target::Reveal);
                }
                if setup.field.as_ref().is_some_and(inside) {
                    return Some(Target::Field);
                }
            }
        }
        if self.notify.as_ref().is_some_and(inside) {
            return Some(Target::Notify);
        }
        if self.warn.as_ref().is_some_and(inside) {
            return Some(Target::Warn);
        }
        self.buttons
            .iter()
            .find(|(_, r)| inside(r))
            .map(|(t, _)| *t)
    }

    /// Where `target` is drawn, and how round its corners are, for the
    /// focus ring.
    fn ring(&self, target: Target) -> Option<(Rect, i32)> {
        let m = &self.metrics;
        let pill = |r: Rect| (r, r.height / 2);
        let setup = self.blocks.iter().find_map(|b| b.setup);
        match target {
            Target::Switch(i) => self.blocks.get(i).map(|b| pill(b.switch)),
            Target::Keys(i) => self.blocks.get(i).and_then(|b| b.keys).map(pill),
            Target::Field => setup.and_then(|s| s.field).map(|r| (r, m.px(6))),
            Target::Reveal => setup.and_then(|s| s.reveal).map(|r| (r, m.px(6))),
            Target::Save | Target::Install => setup.map(|s| pill(s.action)),
            Target::Notify => self.notify.map(|r| (r, m.px(6))),
            Target::Warn => self.warn.map(|r| (r, m.px(6))),
            Target::Refresh | Target::Settings => self
                .buttons
                .iter()
                .find(|(t, _)| *t == target)
                .map(|(_, r)| pill(*r)),
        }
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
    let hover = popup.hover();

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

    for (index, (provider, block)) in reading.providers.iter().zip(&layout.blocks).enumerate() {
        let on = popup.shown_on(index);
        draw::card(&mut pen, block.card, m, &ink);
        draw::switch(
            &mut pen,
            block.switch,
            on,
            hover == Some(Target::Switch(index)),
            m,
            &ink,
        );
        if let Some(r) = block.keys {
            draw::outline_button(
                &mut pen,
                &mut fonts.engine,
                st.small,
                r,
                "Key…",
                hover == Some(Target::Keys(index)),
                m,
                &ink,
            );
        }
        // Beside the name: the plan, or that its vendor does not document it.
        let beside = provider
            .plan
            .as_deref()
            .or_else(|| provider.unofficial.then_some("undocumented"));
        let beside_x = beside.map_or(block.name.right(), |text| {
            draw::right_label(
                &mut pen,
                &mut fonts.engine,
                st.small,
                block.name,
                text,
                ink.dim,
            )
        });
        let name = Rect::new(
            block.name.x,
            block.name.y,
            (beside_x - block.name.x - u / 2).max(0),
            block.name.height,
        );
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.strong,
            name,
            &provider.name,
            if on { ink.text } else { ink.dim },
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

        if let (Some(rects), Some(setup)) = (block.setup, popup.setup()) {
            match setup {
                Setup::Ask(ask) => {
                    paint_ask(
                        &mut pen,
                        fonts,
                        &st,
                        &ink,
                        m,
                        popup,
                        rects,
                        ask,
                        &provider.name,
                    );
                }
                Setup::Tool(_) => {
                    let needs = provider.tool.as_deref().map_or_else(
                        || "Its vendor's tool is not installed.".to_owned(),
                        |tool| format!("Needs {tool}."),
                    );
                    draw::label(
                        &mut pen,
                        &mut fonts.engine,
                        st.small,
                        rects.label,
                        &needs,
                        ink.text,
                    );
                    let lit = hover == Some(Target::Install);
                    draw::button(
                        &mut pen,
                        &mut fonts.engine,
                        st.text,
                        rects.action,
                        "Install in a terminal…",
                        (Some(if lit { ink.text } else { ink.accent }), ink.on_accent),
                        None,
                    );
                }
            }
        }
    }

    if let Some(r) = layout.empty {
        let line_h = r.height / 2;
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.text,
            Rect::new(r.x, r.y, r.width, line_h),
            "No AI provider is installed.",
            ink.text,
        );
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.small,
            Rect::new(r.x, r.y + line_h, r.width, line_h),
            "They come with the alpymist-ai-usage package.",
            ink.dim,
        );
    }

    if let Some(r) = layout.notify {
        draw::check(
            &mut pen,
            fonts,
            &st,
            r,
            "Notify when a limit is near",
            reading.notify,
            hover == Some(Target::Notify),
            m,
            &ink,
        );
    }
    if let Some(r) = layout.warn {
        draw::stepper(
            &mut pen,
            fonts,
            &st,
            r,
            "Warn from",
            &format!("{}%", reading.warn_at),
            hover == Some(Target::Warn),
            m,
            &ink,
        );
    }

    if let Some(message) = popup.message() {
        let colour = if message.error { ink.warn } else { ink.accent };
        for (line, r) in message.lines.iter().zip(&layout.message) {
            draw::label(&mut pen, &mut fonts.engine, st.small, *r, line, colour);
        }
    }

    for (target, r) in &layout.buttons {
        let label = match target {
            Target::Refresh if popup.asking() => "Asking…",
            Target::Refresh => "Ask again",
            _ => "All settings…",
        };
        draw::outline_button(
            &mut pen,
            &mut fonts.engine,
            st.text,
            *r,
            label,
            hover == Some(*target),
            m,
            &ink,
        );
    }

    if popup.focus_visible()
        && let Some((r, radius)) = layout.ring(popup.focus())
    {
        draw::focus_ring(&mut pen, r, radius, m, &ink);
    }

    let hints = match (popup.setup(), popup.focus()) {
        (Some(Setup::Ask(_)), _) => "enter keep    tab move    esc cancel",
        (Some(Setup::Tool(_)), _) => "enter install    tab move    esc cancel",
        (None, Target::Warn) if popup.focus_visible() => "←→ change    tab move    esc close",
        (None, Target::Switch(_)) if popup.focus_visible() => {
            "space switch    tab move    esc close"
        }
        _ => "tab move    enter press    esc close",
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

/// The key being asked for: what it is, the field, the eye and the button.
#[allow(clippy::too_many_arguments)]
fn paint_ask(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    st: &Styles,
    ink: &Ink,
    m: &Metrics,
    popup: &Popup,
    rects: SetupRects,
    ask: &Ask,
    provider: &str,
) {
    let (Some(field), Some(reveal), Some(credential)) = (rects.field, rects.reveal, ask.current())
    else {
        return;
    };
    let engine = &mut fonts.engine;
    let asks = if ask.change {
        format!("New {} for {provider}", credential.title)
    } else if credential.optional {
        format!("{} for {provider}, optional", credential.title)
    } else {
        format!("{} for {provider}", credential.title)
    };
    let asks = asks
        .chars()
        .next()
        .map(|first| {
            first
                .to_uppercase()
                .chain(asks.chars().skip(1))
                .collect::<String>()
        })
        .unwrap_or_default();
    draw::label(pen, engine, st.small, rects.label, &asks, ink.text);

    pen.fill_rounded_rect(field, m.px(6), ink.background);
    pen.stroke_rounded_rect(field, m.px(6), m.px(1), ink.accent);
    let text_x = field.x + m.pad / 2;
    let room = Rect::new(text_x, field.y, (reveal.x - text_x).max(0), field.height);
    let top = text_top(engine, st.text, field.y, field.height);
    {
        let mut clip = pen.with_clip(room);
        let shown: String = if ask.reveal {
            ask.typed.clone()
        } else {
            "•".repeat(ask.typed.chars().count())
        };
        let w = engine.measure_line(st.text, &shown);
        // A long key scrolls left, so the end, where typing happens, shows.
        let x = text_x.min(room.right() - w - m.px(4));
        if shown.is_empty() {
            engine.draw(
                &mut clip,
                st.text,
                Point::new(text_x + m.px(4), top),
                if ask.skippable() {
                    "Leave empty to pass over it"
                } else {
                    "Type it, or paste with Ctrl+V"
                },
                ink.dim,
            );
        } else {
            engine.draw(&mut clip, st.text, Point::new(x, top), &shown, ink.text);
        }
        let caret_x = if shown.is_empty() {
            text_x
        } else {
            x + w + m.px(1)
        };
        // The caret only where typing goes.
        if popup.focus() == Target::Field {
            clip.fill_rect(
                Rect::new(
                    caret_x,
                    top + m.px(2),
                    m.px(2),
                    engine.line_height(st.text) - m.px(4),
                ),
                ink.accent,
            );
        }
    }
    let eye = if ask.reveal { EYE_OFF } else { EYE };
    draw::centred(
        pen,
        engine,
        st.icon_small,
        reveal,
        eye,
        if popup.hover() == Some(Target::Reveal) {
            ink.text
        } else {
            ink.dim
        },
    );
    let lit = popup.hover() == Some(Target::Save);
    draw::button(
        pen,
        engine,
        st.text,
        rects.action,
        "Keep",
        (Some(if lit { ink.text } else { ink.accent }), ink.on_accent),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::Layout;
    use crate::popup::{Command, Popup, Reading, Reply, Target, sample};
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
        assert_eq!(l.blocks.len(), 4);
        for (i, b) in l.blocks.iter().enumerate() {
            assert_eq!(l.hit(centre(b.switch)), Some(Target::Switch(i)));
            assert_eq!(l.hit(centre(b.name)), None);
        }
        // Only the provider that is on and takes a key has the button.
        let keys: Vec<bool> = l.blocks.iter().map(|b| b.keys.is_some()).collect();
        assert_eq!(keys, [false, false, true, false]);
        assert_eq!(
            l.hit(centre(l.blocks[2].keys.unwrap())),
            Some(Target::Keys(2))
        );
        assert_eq!(l.hit(centre(l.notify.unwrap())), Some(Target::Notify));
        assert_eq!(l.hit(centre(l.warn.unwrap())), Some(Target::Warn));
        for (t, r) in &l.buttons {
            assert_eq!(l.hit(centre(*r)), Some(*t));
        }
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
    fn a_provider_being_set_up_makes_room_under_its_row() {
        let a = Appearance::default();
        let mut p = popup();
        let before = Layout::new(&a, &p, 1);
        let on = Command::TurnOn("openrouter".into());
        p.click(Target::Switch(3));
        p.finished(&on, Reply::NeedsKeys(vec!["api-key".into()]));
        let asking = Layout::new(&a, &p, 1);
        assert!(asking.size.height > before.size.height);
        let setup = asking.blocks[3].setup.expect("a field");
        assert_eq!(asking.hit(centre(setup.action)), Some(Target::Save));
        assert_eq!(
            asking.hit(centre(setup.reveal.unwrap())),
            Some(Target::Reveal)
        );
        let field = setup.field.unwrap();
        assert_eq!(
            asking.hit(Point::new(field.x + 4, field.y + 4)),
            Some(Target::Field)
        );
        assert!(setup.action.bottom() <= asking.blocks[3].card.bottom());

        let mut p = popup();
        let on = Command::TurnOn("codex".into());
        p.click(Target::Switch(1));
        p.finished(&on, Reply::NeedsTool);
        let tool = Layout::new(&a, &p, 1);
        let setup = tool.blocks[1].setup.expect("a button");
        assert!(setup.field.is_none());
        assert_eq!(tool.hit(centre(setup.action)), Some(Target::Install));
    }

    #[test]
    fn nothing_installed_says_so_and_offers_settings() {
        let mut p = Popup::new();
        p.update(Reading::default());
        let l = Layout::new(&Appearance::default(), &p, 1);
        assert!(l.empty.is_some() && l.notify.is_none());
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

//! Laying out and painting the question with Denise, in the look of the
//! password prompt that follows it.

use crate::ask::{Dialog, Target};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise_render::Canvas;
use denise_text::{TextEngine, TextStyle};

/// The dialog's width, in logical pixels at a 16 px font.
pub const WIDTH: i32 = 480;

/// A Nerd Font plug, beside the title.
const PLUG: &str = "\u{f06a5}";

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The sizes it was laid out at.
    pub metrics: Metrics,
    /// The whole dialog.
    pub size: Size,
    /// The title.
    pub title: Rect,
    /// The line under it.
    pub subtitle: Rect,
    /// What letting it in means, a line each.
    pub risk: Vec<(Rect, String)>,
    /// What Always means, a line each, on a card.
    pub always: Vec<(Rect, String)>,
    /// The card behind it.
    pub card: Rect,
    /// That a password is asked next.
    pub hint: Rect,
    /// The buttons, left to right.
    pub buttons: [(Target, Rect); 3],
}

impl Layout {
    /// Lay `dialog` out at an output scale.
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn new(appearance: &Appearance, fonts: &mut Fonts, dialog: &Dialog, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let st = fonts.styles(&m);
        let engine = &mut fonts.engine;
        let u = m.unit;
        let (x, w) = (m.inner_x(), m.inner_w());
        let q = &dialog.question;
        let mut y = m.border + m.pad;

        let title = Rect::new(x, y, w, u * 2);
        y = title.bottom();
        let subtitle = Rect::new(x, y, w, u * 3 / 2);
        y = subtitle.bottom() + u * 3 / 4;

        let line = engine.line_height(st.text) + m.px(3);
        let mut risk = Vec::new();
        for text in wrap(engine, st.text, &q.risk, w) {
            risk.push((Rect::new(x, y, w, line), text));
            y += line;
        }
        y += u * 3 / 4;

        let small = engine.line_height(st.small) + m.px(3);
        let pad = u * 3 / 4;
        let card_top = y;
        y += pad * 2 / 3;
        let mut always = Vec::new();
        for text in wrap(engine, st.small, &q.always, w - pad * 2) {
            always.push((Rect::new(x + pad, y, w - pad * 2, small), text));
            y += small;
        }
        y += pad * 2 / 3;
        let card = Rect::new(x, card_top, w, y - card_top);
        y += u / 2;

        let hint = Rect::new(x, y, w, u * 3 / 2);
        y = hint.bottom() + u / 2;

        let button_h = u * 2 + m.px(4);
        let gap = u / 2;
        let widths = Target::ALL.map(|t| engine.measure_line(st.strong, t.label()) + u * 2);
        let mut right = x + w;
        let mut buttons = [(Target::Deny, Rect::new(0, 0, 0, 0)); 3];
        for (i, target) in Target::ALL.iter().enumerate().rev() {
            let bw = widths[i];
            buttons[i] = (*target, Rect::new(right - bw, y, bw, button_h));
            right -= bw + gap;
        }
        y += button_h;

        Self {
            size: m.size(y + m.pad),
            metrics: m,
            title,
            subtitle,
            risk,
            always,
            card,
            hint,
            buttons,
        }
    }

    /// The button under `at`.
    #[must_use]
    pub fn hit(&self, at: Point) -> Option<Target> {
        self.buttons
            .iter()
            .find(|(_, r)| r.contains(at))
            .map(|(t, _)| *t)
    }
}

/// `text` in lines no wider than `width`.
fn wrap(engine: &mut TextEngine, style: TextStyle, text: &str, width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && engine.measure_line(style, &candidate) > width {
            lines.push(std::mem::take(&mut line));
            word.clone_into(&mut line);
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Paint the dialog into `frame`.
#[allow(clippy::many_single_char_names)]
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    dialog: &Dialog,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = canvas.pen();
    let m = &layout.metrics;
    let u = m.unit;
    let st = fonts.styles(m);
    let ink = Ink::new(appearance);
    // Opaque, as the password prompt is: this is a question about the
    // machine's safety, not a popup.
    draw::panel(
        &mut pen,
        layout.size,
        m,
        &Ink {
            background: ink.background.with_alpha(255),
            ..ink
        },
    );
    let engine = &mut fonts.engine;
    let q = &dialog.question;

    let t = layout.title;
    draw::label(
        &mut pen,
        engine,
        st.icon_large,
        Rect::new(t.x, t.y, u * 2, t.height),
        PLUG,
        ink.accent,
    );
    let text_x = t.x + u * 2;
    draw::label(
        &mut pen,
        engine,
        st.large,
        Rect::new(text_x, t.y, t.right() - text_x, t.height),
        &q.title,
        ink.text,
    );
    let s = layout.subtitle;
    draw::label(
        &mut pen,
        engine,
        st.small,
        Rect::new(text_x, s.y, s.right() - text_x, s.height),
        &q.subtitle,
        ink.dim,
    );

    for (rect, text) in &layout.risk {
        draw::label(&mut pen, engine, st.text, *rect, text, ink.text);
    }

    draw::card(&mut pen, layout.card, m, &ink);
    let colour = if q.always_warns { ink.warn } else { ink.dim };
    for (rect, text) in &layout.always {
        draw::label(&mut pen, engine, st.small, *rect, text, colour);
    }

    draw::label(
        &mut pen,
        engine,
        st.small,
        layout.hint,
        "Allowing asks for an administrator's password next.",
        ink.dim,
    );

    for (target, rect) in layout.buttons {
        let hovered = dialog.hover() == Some(target);
        if target == Target::Deny {
            let fill = if hovered {
                draw::mix(appearance.accent, appearance.text, 20)
            } else {
                ink.accent
            };
            draw::button(
                &mut pen,
                engine,
                st.strong,
                rect,
                target.label(),
                (Some(fill), ink.on_accent),
                None,
            );
        } else {
            draw::outline_button(
                &mut pen,
                engine,
                st.text,
                rect,
                target.label(),
                hovered,
                m,
                &ink,
            );
        }
        if dialog.focus() == Some(target) {
            draw::focus_ring(&mut pen, rect, rect.height / 2, m, &ink);
        }
    }
}

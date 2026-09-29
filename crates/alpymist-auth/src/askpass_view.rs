//! The askpass dialog, laid out and painted: the polkit prompt's panel,
//! field and foot, around ssh's question.
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │  󰌆  Unlock an SSH key                     │
//! │     Enter passphrase for key              │
//! │  ┌──────────────────────────────────────┐ │
//! │  │ ~/.ssh/id_ed25519                    │ │
//! │  └──────────────────────────────────────┘ │
//! │  ┌──────────────────────────────────────┐ │
//! │  │ ● ● ● ● ●|                           │ │
//! │  └──────────────────────────────────────┘ │
//! │  Caps Lock is on                          │
//! │                          [ Cancel ] [Unlock]
//! └──────────────────────────────────────────┘
//! ```

// Geometry reads best in the letters it is written in.
#![allow(clippy::many_single_char_names)]

use crate::askpass::{Askpass, Focus, Target};
use crate::view::{self, Field};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics, Styles};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;
use denise_text::{TextEngine, TextStyle};

/// A key.
const KEY: &str = "\u{f0306}";
/// A warning.
const WARN: &str = "\u{f05d6}";

/// The dialog's width in logical pixels at a 16 px font.
const WIDTH: i32 = 460;

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Sizes everything is measured in.
    pub metrics: Metrics,
    /// The whole panel.
    pub size: Size,
    /// The key and title.
    pub title: Rect,
    /// The question, wrapped, its own line breaks kept.
    pub question: Vec<String>,
    /// Where the question starts.
    pub question_at: Rect,
    /// The key's file or the site, in a card.
    pub subject: Option<Rect>,
    /// The field, where something is to be typed.
    pub field: Option<Rect>,
    /// The line under it: Caps Lock.
    pub status: Option<Rect>,
    /// Cancel or Deny, where there is one.
    pub cancel: Option<Rect>,
    /// Unlock, Allow or OK.
    pub accept: Rect,
    /// How to check the prompt is genuine, or that it was, at the foot.
    pub check: Option<Rect>,
}

impl Layout {
    /// The dialog for `ask` at an output scale.
    #[must_use]
    pub fn new(appearance: &Appearance, fonts: &mut Fonts, ask: &Askpass, scale: u32) -> Self {
        let metrics = Metrics::new(appearance, scale, WIDTH);
        let styles = fonts.styles(&metrics);
        let u = metrics.unit;
        let x = metrics.inner_x();
        let w = metrics.inner_w();
        let engine = &mut fonts.engine;

        let title = Rect::new(x, metrics.border + metrics.pad, w, u * 5 / 2);
        let text_x = x + u * 5 / 2;
        let text_w = w - (text_x - x);
        let question: Vec<String> = ask
            .question
            .lines()
            .flat_map(|line| {
                let wrapped: Vec<String> = engine
                    .wrap(styles.text, line, text_w)
                    .into_iter()
                    .flat_map(|l| hard_wrap(engine, styles.text, l, text_w))
                    .collect();
                if wrapped.is_empty() {
                    vec![String::new()]
                } else {
                    wrapped
                }
            })
            .collect();
        let line = engine.line_height(styles.text);
        let lines = i32::try_from(question.len().max(1)).unwrap_or(1);
        let question_at = Rect::new(text_x, title.bottom(), text_w, line * lines);
        let mut bottom = question_at.bottom();
        let subject = ask.subject.as_ref().map(|_| {
            let r = Rect::new(x, bottom + u * 3 / 4, w, u * 9 / 4);
            bottom = r.bottom();
            r
        });
        let (field, status) = if ask.has_field() {
            let field = Rect::new(x, bottom + u * 3 / 4, w, u * 5 / 2);
            let status = Rect::new(x, field.bottom() + u / 4, w, u * 3 / 2);
            bottom = status.bottom();
            (Some(field), Some(status))
        } else {
            (None, None)
        };
        let bh = u * 2;
        let by = bottom + if ask.has_field() { u / 2 } else { u * 5 / 4 };
        let aw = engine.measure_line(styles.strong, ask.accept_label()) + u * 2;
        let accept = Rect::new(x + w - aw.max(u * 5), by, aw.max(u * 5), bh);
        let cancel = ask.cancel_label().map(|label| {
            let cw = engine.measure_line(styles.strong, label) + u * 2;
            let cw = cw.max(u * 5);
            Rect::new(accept.x - u / 2 - cw, by, cw, bh)
        });
        let check = ask
            .checkable
            .then(|| Rect::new(x, accept.bottom() + u * 3 / 4, w, u * 3 / 2));
        let bottom = check.map_or(accept.bottom(), |c| c.bottom());
        let size = metrics.size(bottom + metrics.pad);
        Self {
            metrics,
            size,
            title,
            question,
            question_at,
            subject,
            field,
            status,
            cancel,
            accept,
            check,
        }
    }

    /// What is at `p`.
    #[must_use]
    pub fn hit(&self, p: Point) -> Option<Target> {
        if self.cancel.is_some_and(|r| r.contains(p)) {
            Some(Focus::Cancel)
        } else if self.accept.contains(p) {
            Some(Focus::Accept)
        } else if self.field.is_some_and(|r| r.contains(p)) {
            Some(Focus::Field)
        } else {
            None
        }
    }
}

/// `line` in pieces no wider than `width`, broken between characters: a key's
/// fingerprint is one word longer than the dialog is wide.
fn hard_wrap(engine: &mut TextEngine, style: TextStyle, line: &str, width: i32) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for ch in line.chars() {
        piece.push(ch);
        if piece.chars().count() > 1 && engine.measure_line(style, &piece) > width {
            piece.pop();
            pieces.push(std::mem::take(&mut piece));
            piece.push(ch);
        }
    }
    pieces.push(piece);
    pieces
}

/// Paint the dialog.
#[allow(clippy::too_many_lines)] // the dialog, top to bottom
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    ask: &Askpass,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let m = &layout.metrics;
    let u = m.unit;
    let styles: Styles = fonts.styles(m);
    let ink = Ink::new(appearance);
    // Opaque, like the polkit prompt: nothing shows through behind a field
    // a passphrase is typed into.
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

    // Title, and the question.
    let t = layout.title;
    draw::label(
        &mut pen,
        engine,
        styles.icon_large,
        Rect::new(t.x, t.y, u * 2, t.height),
        KEY,
        ink.accent,
    );
    draw::label(
        &mut pen,
        engine,
        styles.large,
        Rect::new(layout.question_at.x, t.y, t.width, t.height),
        ask.title,
        ink.text,
    );
    let line = engine.line_height(styles.text);
    for (i, text) in layout.question.iter().enumerate() {
        let y = layout.question_at.y + i32::try_from(i).unwrap_or(0) * line;
        engine.draw(
            &mut pen,
            styles.text,
            Point::new(layout.question_at.x, y),
            text,
            ink.dim,
        );
    }

    // The key or the site.
    if let (Some(r), Some(subject)) = (layout.subject, &ask.subject) {
        draw::card(&mut pen, r, m, &ink);
        let inner = Rect::new(r.x + u * 3 / 4, r.y, r.width - u * 3 / 2, r.height);
        let mut clip = pen.with_clip(inner);
        let top = draw::text_top(engine, styles.strong, r.y, r.height);
        engine.draw(
            &mut clip,
            styles.strong,
            Point::new(inner.x, top),
            subject,
            ink.text,
        );
    }

    // The field, and Caps Lock under it.
    if let Some(rect) = layout.field {
        view::field(
            &mut pen,
            engine,
            &styles,
            m,
            &ink,
            &Field {
                rect,
                secret: &ask.secret,
                echo: ask.echo,
                hint: ask.hint,
                taking: true,
                focused: ask.focus == Focus::Field,
            },
        );
    }
    if let Some(s) = layout.status
        && ask.caps_lock
        && !ask.echo
    {
        view::note(
            &mut pen,
            engine,
            &styles,
            s,
            WARN,
            "Caps Lock is on",
            ink.warn,
        );
    }

    if let Some(c) = layout.check {
        view::check(&mut pen, engine, &styles, m, &ink, c, ask.verified);
    }

    // Buttons.
    if let (Some(rect), Some(label)) = (layout.cancel, ask.cancel_label()) {
        draw::outline_button(
            &mut pen,
            engine,
            styles.strong,
            rect,
            label,
            ask.hover == Some(Focus::Cancel),
            m,
            &ink,
        );
    }
    let (fill, text) = if ask.ready() {
        (ink.accent, ink.on_accent)
    } else {
        (ink.selection, ink.dim)
    };
    draw::button(
        &mut pen,
        engine,
        styles.strong,
        layout.accept,
        ask.accept_label(),
        (Some(fill), text),
        None,
    );
    for (focus, rect) in [
        (Focus::Cancel, layout.cancel),
        (Focus::Accept, Some(layout.accept)),
    ] {
        if let Some(rect) = rect
            && ask.focus == focus
        {
            draw::focus_ring(&mut pen, rect, rect.height / 2, m, &ink);
        }
    }
}

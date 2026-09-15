//! The dialog, laid out and painted.
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │  󰌾  Authentication required               │
//! │     Install Alpine packages               │
//! │  ┌──────────────────────────────────────┐ │
//! │  │ Install gimp                         │ │
//! │  └──────────────────────────────────────┘ │
//! │  (A) André Biseth (andre)            ‹ ›  │
//! │  ┌──────────────────────────────────────┐ │
//! │  │ ● ● ● ● ●|                           │ │
//! │  └──────────────────────────────────────┘ │
//! │  Caps Lock is on                          │
//! │                     [ Cancel ] [Authenticate]
//! └──────────────────────────────────────────┘
//! ```

// Geometry reads best in the letters it is written in.
#![allow(clippy::many_single_char_names)]

use crate::prompt::{Focus, Phase, Prompt, Target};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics, Styles};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_render::Canvas;

/// A padlock.
const LOCK: &str = "\u{f033e}";
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
    /// The padlock and title.
    pub title: Rect,
    /// polkit's message, wrapped.
    pub message: Vec<String>,
    /// Where the message starts.
    pub message_at: Rect,
    /// What is to be done, in a card.
    pub what: Rect,
    /// The account.
    pub account: Rect,
    /// The account arrows, where there is more than one account.
    pub identity: Option<(Rect, Rect)>,
    /// The field.
    pub field: Rect,
    /// The line under it: an error, Caps Lock, or checking.
    pub status: Rect,
    /// Cancel.
    pub cancel: Rect,
    /// Authenticate.
    pub authenticate: Rect,
}

impl Layout {
    /// The dialog for `prompt` at an output scale.
    #[must_use]
    pub fn new(appearance: &Appearance, fonts: &mut Fonts, prompt: &Prompt, scale: u32) -> Self {
        let metrics = Metrics::new(appearance, scale, WIDTH);
        let styles = fonts.styles(&metrics);
        let u = metrics.unit;
        let x = metrics.inner_x();
        let w = metrics.inner_w();
        let engine = &mut fonts.engine;

        let title = Rect::new(x, metrics.border + metrics.pad, w, u * 5 / 2);
        let text_x = x + u * 5 / 2;
        let message: Vec<String> = engine
            .wrap(styles.text, &prompt.request.message, w - (text_x - x))
            .into_iter()
            .map(str::to_owned)
            .collect();
        let line = engine.line_height(styles.text);
        let lines = i32::try_from(message.len().max(1)).unwrap_or(1);
        let message_at = Rect::new(text_x, title.bottom(), w - (text_x - x), line * lines);
        let what = Rect::new(x, message_at.bottom() + u * 3 / 4, w, u * 9 / 4);
        let account = Rect::new(x, what.bottom() + u * 3 / 4, w, u * 2);
        let identity = (prompt.request.identities.len() > 1).then(|| {
            let side = u * 3 / 2;
            let y = account.y + (account.height - side) / 2;
            let next = Rect::new(account.right() - side, y, side, side);
            let prev = Rect::new(next.x - side - u / 4, y, side, side);
            (prev, next)
        });
        let field = Rect::new(x, account.bottom() + u / 2, w, u * 5 / 2);
        let status = Rect::new(x, field.bottom() + u / 4, w, u * 3 / 2);
        let bh = u * 2;
        let aw = engine.measure_line(styles.strong, "Authenticate") + u * 2;
        let cw = engine.measure_line(styles.strong, "Cancel") + u * 2;
        let by = status.bottom() + u / 2;
        let authenticate = Rect::new(x + w - aw, by, aw, bh);
        let cancel = Rect::new(authenticate.x - u / 2 - cw, by, cw, bh);
        let size = metrics.size(authenticate.bottom() + metrics.pad);
        Self {
            metrics,
            size,
            title,
            message,
            message_at,
            what,
            account,
            identity,
            field,
            status,
            cancel,
            authenticate,
        }
    }

    /// What is at `p`.
    #[must_use]
    pub fn hit(&self, p: Point) -> Option<Target> {
        if self.cancel.contains(p) {
            Some(Target::Cancel)
        } else if self.authenticate.contains(p) {
            Some(Target::Authenticate)
        } else if self.field.contains(p) {
            Some(Target::Field)
        } else if let Some((prev, next)) = self.identity {
            if prev.contains(p) {
                Some(Target::Identity(-1))
            } else if next.contains(p) {
                Some(Target::Identity(1))
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Paint the dialog.
#[allow(clippy::too_many_lines)] // the dialog, top to bottom
pub fn paint(
    frame: &mut Frame<'_>,
    layout: &Layout,
    appearance: &Appearance,
    fonts: &mut Fonts,
    prompt: &Prompt,
) {
    let mut canvas = Canvas::new(frame);
    let mut pen = Pen::new(&mut canvas);
    let m = &layout.metrics;
    let u = m.unit;
    let styles: Styles = fonts.styles(m);
    let ink = Ink::new(appearance);
    // Opaque, whatever the menu's translucency: nothing behind a password
    // field should show through it.
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

    // Title.
    let t = layout.title;
    draw::label(
        &mut pen,
        engine,
        styles.icon_large,
        Rect::new(t.x, t.y, u * 2, t.height),
        LOCK,
        ink.accent,
    );
    draw::label(
        &mut pen,
        engine,
        styles.large,
        Rect::new(layout.message_at.x, t.y, t.width, t.height),
        "Authentication required",
        ink.text,
    );
    let line = engine.line_height(styles.text);
    for (i, text) in layout.message.iter().enumerate() {
        let y = layout.message_at.y + i32::try_from(i).unwrap_or(0) * line;
        engine.draw(
            &mut pen,
            styles.text,
            Point::new(layout.message_at.x, y),
            text,
            ink.dim,
        );
    }

    // What, as polkit says it.
    let what = layout.what;
    draw::card(&mut pen, what, m, &ink);
    let inner = Rect::new(
        what.x + u * 3 / 4,
        what.y,
        what.width - u * 3 / 2,
        what.height,
    );
    let mut clip = pen.with_clip(inner);
    let top = draw::text_top(engine, styles.strong, what.y, what.height);
    engine.draw(
        &mut clip,
        styles.strong,
        Point::new(inner.x, top),
        &prompt.request.what(),
        ink.text,
    );
    drop(clip);

    // The account.
    let a = layout.account;
    let identity = prompt.request.identities.get(prompt.identity);
    let name = identity.map_or_else(
        || "an administrator".to_owned(),
        crate::request::Identity::display,
    );
    let initial = identity
        .and_then(|i| i.full_name.chars().next().or_else(|| i.name.chars().next()))
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect());
    let r = a.height / 2;
    pen.fill_circle(Point::new(a.x + r, a.y + r), r, ink.selection);
    draw::centred(
        &mut pen,
        engine,
        styles.strong,
        Rect::new(a.x, a.y, a.height, a.height),
        &initial,
        ink.text,
    );
    let name_x = a.x + a.height + u / 2;
    let name_w = layout
        .identity
        .map_or(a.right(), |(prev, _)| prev.x - u / 2)
        - name_x;
    draw::label(
        &mut pen,
        engine,
        styles.text,
        Rect::new(name_x, a.y, name_w.max(0), a.height),
        &name,
        ink.text,
    );
    if let Some((prev, next)) = layout.identity {
        for (rect, glyph, target) in [
            (prev, draw::CHEVRON_LEFT, Target::Identity(-1)),
            (next, draw::CHEVRON_RIGHT, Target::Identity(1)),
        ] {
            if prompt.hover == Some(target) {
                pen.fill_rounded_rect(rect, rect.height / 2, ink.selection);
            }
            draw::centred(&mut pen, engine, styles.icon, rect, glyph, ink.dim);
        }
    }

    // The field.
    let f = layout.field;
    let focused = prompt.focus == Focus::Field;
    pen.fill_rounded_rect(f, m.px(8), ink.card);
    let edge = if focused && prompt.phase == Phase::Asking {
        ink.accent
    } else {
        ink.selection
    };
    pen.stroke_rounded_rect(f, m.px(8), m.px(2), edge);
    let inner = Rect::new(f.x + u * 3 / 4, f.y, f.width - u * 3 / 2, f.height);
    let mut caret_x = inner.x;
    if prompt.secret.is_empty() {
        let hint = match prompt.phase {
            Phase::Starting => "Getting ready…",
            Phase::Asking | Phase::Checking => prompt.label.as_str(),
        };
        let at = Rect::new(inner.x + m.px(6), inner.y, inner.width, inner.height);
        draw::label(&mut pen, engine, styles.text, at, hint, ink.dim);
    } else if prompt.echo {
        let text = String::from_utf8_lossy(prompt.secret.expose()).into_owned();
        caret_x += draw::label(&mut pen, engine, styles.text, inner, &text, ink.text) + m.px(2);
    } else {
        // One dot a character, until the field is full; how long the
        // password is shows no further than that.
        let dot = u * 5 / 16;
        let step = dot * 2 + u / 4;
        let most = usize::try_from((inner.width - step) / step.max(1)).unwrap_or(0);
        let cy = f.y + f.height / 2;
        for i in 0..prompt.secret.chars().min(most) {
            let cx = inner.x + dot + i32::try_from(i).unwrap_or(0) * step;
            pen.fill_circle(Point::new(cx, cy), dot, ink.text);
            caret_x = cx + dot + u / 4;
        }
    }
    if prompt.phase == Phase::Asking && focused {
        let h = u * 5 / 4;
        pen.fill_rect(
            Rect::new(caret_x, f.y + (f.height - h) / 2, m.px(2), h),
            ink.accent,
        );
    }

    // The line under it.
    let s = layout.status;
    if prompt.phase == Phase::Checking {
        let centre = Point::new(s.x + u / 2, s.y + s.height / 2);
        let start = i32::try_from(prompt.frame % 24).unwrap_or(0) * denise::TURN / 24;
        pen.stroke_circle(centre, u / 2, m.px(2), ink.accent.with_alpha(50));
        pen.stroke_arc(centre, u / 2, m.px(2), start, denise::TURN / 3, ink.accent);
        draw::label(
            &mut pen,
            engine,
            styles.small,
            Rect::new(s.x + u * 3 / 2, s.y, s.width, s.height),
            "Checking…",
            ink.dim,
        );
    } else if let Some(text) = prompt.error.as_ref().or(prompt.info.as_ref()) {
        let colour = if prompt.error.is_some() {
            ink.warn
        } else {
            ink.dim
        };
        let iw = draw::label(
            &mut pen,
            engine,
            styles.icon_small,
            Rect::new(s.x, s.y, u, s.height),
            WARN,
            colour,
        );
        draw::label(
            &mut pen,
            engine,
            styles.small,
            Rect::new(s.x + iw + u / 3, s.y, s.width, s.height),
            text,
            colour,
        );
    } else if prompt.caps_lock {
        let iw = draw::label(
            &mut pen,
            engine,
            styles.icon_small,
            Rect::new(s.x, s.y, u, s.height),
            WARN,
            ink.warn,
        );
        draw::label(
            &mut pen,
            engine,
            styles.small,
            Rect::new(s.x + iw + u / 3, s.y, s.width, s.height),
            "Caps Lock is on",
            ink.warn,
        );
    }

    // Buttons.
    let ready = prompt.phase == Phase::Asking && !prompt.secret.is_empty();
    draw::outline_button(
        &mut pen,
        engine,
        styles.strong,
        layout.cancel,
        "Cancel",
        prompt.hover == Some(Target::Cancel),
        m,
        &ink,
    );
    let (fill, text) = if ready {
        (ink.accent, ink.on_accent)
    } else {
        (ink.selection, ink.dim)
    };
    draw::button(
        &mut pen,
        engine,
        styles.strong,
        layout.authenticate,
        "Authenticate",
        (Some(fill), text),
        None,
    );
    for (focus, rect) in [
        (Focus::Cancel, layout.cancel),
        (Focus::Authenticate, layout.authenticate),
    ] {
        if prompt.focus == focus {
            draw::focus_ring(&mut pen, rect, rect.height / 2, m, &ink);
        }
    }
}

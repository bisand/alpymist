//! Laying out and painting the About box with Denise.
//!
//! The box is as tall as what it lists, so its size is known before it is
//! drawn and the window asks for exactly that. The same layout answers where
//! a click landed.

use crate::dialog::{Dialog, Target};
use alpymist_ui::{MARK, Silhouette};
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Fonts, Ink, Metrics};
use denise::geom::{Point, Rect, Size};
use denise::{Frame, Mask};
use denise_render::Canvas;

/// The box's width, in logical pixels at a 16 px font.
pub const WIDTH: i32 = 460;

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The sizes it was laid out at.
    pub metrics: Metrics,
    /// The whole box.
    pub size: Size,
    /// The mark.
    pub mark: Rect,
    /// The title and the line under it.
    pub title: Rect,
    /// The subtitle.
    pub subtitle: Rect,
    /// The card of system details.
    pub details: Rect,
    /// One detail row's height.
    pub row: i32,
    /// "Packages".
    pub packages_label: Rect,
    /// The card of packages.
    pub packages: Rect,
    /// One package row's height.
    pub package_row: i32,
    /// Copy, and what happened to the last copy beside it.
    pub copy: Rect,
    /// Close.
    pub close: Rect,
    /// Where the copy's result is said.
    pub status: Rect,
}

impl Layout {
    /// Lay the box out for `dialog` at an output scale.
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn new(appearance: &Appearance, dialog: &Dialog, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let u = m.unit;
        let (x, w) = (m.inner_x(), m.inner_w());
        let mut y = m.border + m.pad;

        let mark_w = u * 4;
        let mark_h = mark_w * i32::try_from(Silhouette::ASPECT).unwrap_or(62) / 100;
        let mark = Rect::new(x, y, mark_w, mark_h);
        let text_x = x + mark_w + u;
        let title = Rect::new(text_x, y, x + w - text_x, u * 2);
        let subtitle = Rect::new(text_x, y + u * 2, x + w - text_x, u * 3 / 2);
        y += mark_h.max(title.height + subtitle.height) + u;

        let row = u * 7 / 4;
        let rows = i32::try_from(dialog.about().rows().len()).unwrap_or(0);
        let details = Rect::new(x, y, w, rows * row + u / 2);
        y = details.bottom() + u * 3 / 4;

        let packages_label = Rect::new(x, y, w, u * 3 / 2);
        y = packages_label.bottom();
        let package_row = u * 3 / 2;
        let count = i32::try_from(dialog.about().shown_packages().len())
            .unwrap_or(0)
            .max(1);
        let packages = Rect::new(x, y, w, count * package_row + u / 2);
        y = packages.bottom() + u;

        let button_h = u * 2 + m.px(4);
        let close_w = u * 5;
        let copy_w = u * 7;
        let close = Rect::new(x + w - close_w, y, close_w, button_h);
        let copy = Rect::new(close.x - u / 2 - copy_w, y, copy_w, button_h);
        let status = Rect::new(x, y, copy.x - u / 2 - x, button_h);
        y = close.bottom() + m.pad;

        Self {
            metrics: m,
            size: Size::new(
                u32::try_from(m.width).unwrap_or(0),
                u32::try_from(y + m.border).unwrap_or(0),
            ),
            mark,
            title,
            subtitle,
            details,
            row,
            packages_label,
            packages,
            package_row,
            copy,
            close,
            status,
        }
    }

    /// What is under `at`.
    #[must_use]
    pub fn hit(&self, at: Point) -> Option<Target> {
        [(self.copy, Target::Copy), (self.close, Target::Close)]
            .into_iter()
            .find(|(r, _)| r.contains(at))
            .map(|(_, t)| t)
    }
}

/// Paint the box into `frame`.
#[allow(clippy::too_many_lines)] // one box, top to bottom
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
    let ink = Ink::new(appearance);
    let st = fonts.styles(m);
    let engine = &mut fonts.engine;
    let about = dialog.about();
    pen.clear(ink.background);

    // The mark, from the silhouette the bar and the splash draw.
    let mw = u32::try_from(layout.mark.width).unwrap_or(1);
    let coverage = MARK.mask(mw);
    let mh = i32::try_from(coverage.len() / mw as usize).unwrap_or(0);
    if let Some(mask) = Mask::new(&coverage, layout.mark.width, mh, mw as usize) {
        pen.blit_mask(Point::new(layout.mark.x, layout.mark.y), &mask, ink.accent);
    }

    draw::label(
        &mut pen,
        engine,
        st.large,
        layout.title,
        &about.title(),
        ink.text,
    );
    draw::label(
        &mut pen,
        engine,
        st.text,
        layout.subtitle,
        &about.subtitle(),
        ink.dim,
    );

    // The system.
    draw::card(&mut pen, layout.details, m, &ink);
    let rows = about.rows();
    let label_w = rows
        .iter()
        .map(|(l, _)| engine.measure_line(st.text, l))
        .max()
        .unwrap_or(0);
    let inner = layout.details.x + u * 3 / 4;
    let value_x = inner + label_w + u;
    let value_w = layout.details.right() - u * 3 / 4 - value_x;
    let mut y = layout.details.y + u / 4;
    for (label, value) in &rows {
        let line = Rect::new(inner, y, label_w, layout.row);
        draw::label(&mut pen, engine, st.text, line, label, ink.dim);
        let line = Rect::new(value_x, y, value_w, layout.row);
        draw::label(&mut pen, engine, st.text, line, value, ink.text);
        y += layout.row;
    }

    // Every Alpymist package, with its version.
    draw::label(
        &mut pen,
        engine,
        st.strong,
        layout.packages_label,
        "Packages",
        ink.text,
    );
    draw::card(&mut pen, layout.packages, m, &ink);
    let (left, right) = (
        layout.packages.x + u * 3 / 4,
        layout.packages.right() - u * 3 / 4,
    );
    let mut y = layout.packages.y + u / 4;
    if about.packages.is_empty() {
        let line = Rect::new(left, y, right - left, layout.package_row);
        draw::label(
            &mut pen,
            engine,
            st.small,
            line,
            "apk lists no Alpymist packages",
            ink.dim,
        );
    }
    for (name, version) in about.shown_packages() {
        let line = Rect::new(left, y, right - left, layout.package_row);
        let x = draw::right_label(&mut pen, engine, st.small, line, version, ink.text);
        let line = Rect::new(left, y, x - u / 2 - left, layout.package_row);
        draw::label(&mut pen, engine, st.small, line, name, ink.dim);
        y += layout.package_row;
    }

    // What happened to the last copy, and the buttons.
    match dialog.copied() {
        Some(Ok(())) => {
            draw::label(
                &mut pen,
                engine,
                st.small,
                layout.status,
                "Copied to the clipboard",
                ink.accent,
            );
        }
        Some(Err(e)) => {
            draw::label(&mut pen, engine, st.small, layout.status, e, ink.warn);
        }
        None => {}
    }
    for (target, rect, label) in [
        (Target::Copy, layout.copy, "Copy details"),
        (Target::Close, layout.close, "Close"),
    ] {
        let hovered = dialog.hover() == Some(target);
        if target == Target::Close {
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
                label,
                (Some(fill), ink.on_accent),
                None,
            );
        } else {
            draw::outline_button(&mut pen, engine, st.text, rect, label, hovered, m, &ink);
        }
        if dialog.focus() == Some(target) {
            draw::focus_ring(&mut pen, rect, rect.height / 2, m, &ink);
        }
    }
}

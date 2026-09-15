//! Drawing a widget: fonts, colours, sizes and controls.
//!
//! Sizes come from the font size, so a panel grows with the text, and colours
//! from the menu's [`Appearance`], so a theme set for the menu dresses every
//! widget. The controls are painted from rectangles a widget's layout worked
//! out, and the same layout answers where a click landed: the painted
//! rectangle and the clickable one never drift apart.

use crate::{Appearance, Colour};
use alpymist_menu::font::LazyFont;
use denise::Color;
use denise::geom::{Point, Rect, Size};
use denise::painter::Pen;
use denise_text::{FontId, TextEngine, TextStyle};

/// The warning colour the menu uses for its notices.
pub const WARN: Color = Color::rgb(0xE8, 0xB0, 0x6A);

/// The fonts, loaded once.
pub struct Fonts {
    /// Measures and draws text.
    pub engine: TextEngine,
    /// The text face.
    pub text: FontId,
    /// A semibold face beside it, or the text face.
    pub strong: FontId,
    /// The icon face.
    pub icons: FontId,
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

    /// The text styles at a layout's sizes.
    #[must_use]
    pub fn styles(&self, metrics: &Metrics) -> Styles {
        let style = |font, size_px| TextStyle { font, size_px };
        Styles {
            text: style(self.text, metrics.text_px),
            strong: style(self.strong, metrics.text_px),
            small: style(self.text, metrics.small_px),
            icon: style(self.icons, metrics.text_px),
            icon_small: style(self.icons, metrics.small_px),
            icon_large: style(self.icons, metrics.large_px),
            large: style(self.strong, metrics.large_px),
        }
    }
}

/// Text styles.
#[derive(Debug, Clone, Copy)]
pub struct Styles {
    /// Body text.
    pub text: TextStyle,
    /// Titles and names.
    pub strong: TextStyle,
    /// Details, hints.
    pub small: TextStyle,
    /// Icons beside body text.
    pub icon: TextStyle,
    /// Icons beside small text.
    pub icon_small: TextStyle,
    /// A big icon.
    pub icon_large: TextStyle,
    /// A big figure.
    pub large: TextStyle,
}

/// The sizes everything is measured in, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Output scale.
    pub scale: u32,
    /// One unit: the font size, scaled.
    pub unit: i32,
    /// Padding inside the border.
    pub pad: i32,
    /// Border thickness.
    pub border: i32,
    /// Corner radius of the panel.
    pub radius: i32,
    /// Body text height.
    pub text_px: u16,
    /// Small text height.
    pub small_px: u16,
    /// Large text height.
    pub large_px: u16,
    /// Panel width.
    pub width: i32,
}

impl Metrics {
    /// Sizes for a panel `width` logical pixels wide at a 16 px font, at an
    /// output scale.
    #[must_use]
    pub fn new(appearance: &Appearance, scale: u32, width: i32) -> Self {
        let scale = scale.max(1);
        let s = i32::try_from(scale).unwrap_or(1);
        let font = i32::from(appearance.font_size.clamp(8, 64));
        let u = font * s;
        let px16 = |v: i32| u16::try_from(v).unwrap_or(u16::MAX);
        Self {
            scale,
            unit: u,
            pad: u * 3 / 4,
            border: 2 * s,
            radius: 10 * s,
            text_px: px16(u),
            small_px: px16(u * 13 / 16),
            large_px: px16(u * 7 / 4),
            width: width * font / 16 * s,
        }
    }

    /// Logical pixels in physical ones.
    #[must_use]
    pub fn px(&self, logical: i32) -> i32 {
        logical * i32::try_from(self.scale).unwrap_or(1)
    }

    /// Where content starts, left of the padding.
    #[must_use]
    pub fn inner_x(&self) -> i32 {
        self.border + self.pad
    }

    /// How wide content is, inside the padding.
    #[must_use]
    pub fn inner_w(&self) -> i32 {
        self.width - 2 * self.inner_x()
    }

    /// The panel's size for content ending at `bottom`.
    #[must_use]
    pub fn size(&self, bottom: i32) -> Size {
        let height = bottom + self.border + self.pad / 4;
        Size::new(
            u32::try_from(self.width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
        )
    }
}

/// A menu colour as Denise's.
#[must_use]
pub fn colour(Colour([red, green, blue, alpha]): Colour) -> Color {
    Color::rgba(red, green, blue, alpha)
}

/// `a` moved towards `b` by `percent`, alpha included.
#[must_use]
pub fn mix(a: Colour, b: Colour, percent: u16) -> Color {
    let p = percent.min(100);
    let m = |x: u8, y: u8| {
        u8::try_from((u16::from(x) * (100 - p) + u16::from(y) * p) / 100).unwrap_or(u8::MAX)
    };
    let (Colour(a), Colour(b)) = (a, b);
    Color::rgba(m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3]))
}

/// The colours a widget paints with.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    /// The panel.
    pub background: Color,
    /// The panel's border.
    pub border: Color,
    /// Text.
    pub text: Color,
    /// Details and hints.
    pub dim: Color,
    /// Icons, lit controls.
    pub accent: Color,
    /// Behind the chosen and hovered.
    pub selection: Color,
    /// A card on the panel.
    pub card: Color,
    /// Text on an accent fill.
    pub on_accent: Color,
    /// Warnings and failures.
    pub warn: Color,
}

impl Ink {
    /// The appearance's colours.
    #[must_use]
    pub fn new(appearance: &Appearance) -> Self {
        let Colour([r, g, b, _]) = appearance.background;
        Self {
            background: colour(appearance.background),
            border: colour(appearance.border),
            text: colour(appearance.text),
            dim: colour(appearance.dim),
            accent: colour(appearance.accent),
            selection: colour(appearance.selection),
            card: mix(appearance.background, appearance.selection, 45),
            on_accent: Color::rgb(r, g, b),
            warn: WARN,
        }
    }
}

/// Where text sits to be centred vertically in a box at `y` of `h`.
#[must_use]
pub fn text_top(engine: &TextEngine, style: TextStyle, y: i32, h: i32) -> i32 {
    y + (h - engine.line_height(style)) / 2
}

/// Draw `text` centred vertically in `r`, from its left edge. Returns the
/// width drawn.
pub fn label(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    text: &str,
    colour: Color,
) -> i32 {
    let top = text_top(engine, style, r.y, r.height);
    let mut clip = pen.with_clip(r);
    let size = engine.draw(&mut clip, style, Point::new(r.x, top), text, colour);
    i32::try_from(size.width).unwrap_or(0)
}

/// Draw `text` centred in `r` both ways.
pub fn centred(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    text: &str,
    colour: Color,
) {
    let w = engine.measure_line(style, text);
    let top = text_top(engine, style, r.y, r.height);
    engine.draw(
        pen,
        style,
        Point::new(r.x + (r.width - w) / 2, top),
        text,
        colour,
    );
}

/// Draw `text` centred vertically in `r`, against its right edge. Returns
/// where it starts.
pub fn right_label(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    text: &str,
    colour: Color,
) -> i32 {
    let w = engine.measure_line(style, text);
    let x = r.right() - w;
    let top = text_top(engine, style, r.y, r.height);
    engine.draw(pen, style, Point::new(x, top), text, colour);
    x
}

/// The panel: its background, rounded, and border.
pub fn panel(pen: &mut Pen<'_>, size: Size, metrics: &Metrics, ink: &Ink) {
    let full = Rect::from_size(size);
    pen.clear(Color::rgba(0, 0, 0, 0));
    pen.fill_rounded_rect(full, metrics.radius, ink.background);
    pen.stroke_rounded_rect(full, metrics.radius, metrics.border, ink.border);
}

/// A card on the panel.
pub fn card(pen: &mut Pen<'_>, r: Rect, metrics: &Metrics, ink: &Ink) {
    pen.fill_rounded_rect(r, metrics.px(8), ink.card);
}

/// A pill-shaped button: a fill, an outline, or both, and a centred label.
pub fn button(
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
    centred(pen, engine, style, r, label, text);
}

/// An outlined button, lit while hovered: Disconnect, Forget.
#[allow(clippy::too_many_arguments)]
pub fn outline_button(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    label: &str,
    hovered: bool,
    metrics: &Metrics,
    ink: &Ink,
) {
    button(
        pen,
        engine,
        style,
        r,
        label,
        (hovered.then_some(ink.selection), ink.text),
        Some((metrics.px(1), ink.dim)),
    );
}

/// The size of a switch for a layout.
#[must_use]
pub fn switch_size(metrics: &Metrics) -> (i32, i32) {
    (metrics.unit * 11 / 4, metrics.unit * 3 / 2)
}

/// An on/off switch.
pub fn switch(pen: &mut Pen<'_>, r: Rect, on: bool, hovered: bool, metrics: &Metrics, ink: &Ink) {
    let track = if on { ink.accent } else { ink.selection };
    pen.fill_rounded_rect(r, r.height / 2, track);
    if hovered {
        pen.stroke_rounded_rect(r, r.height / 2, metrics.px(1), ink.text);
    }
    let knob_r = r.height / 2 - metrics.px(3);
    let cx = if on {
        r.right() - r.height / 2
    } else {
        r.x + r.height / 2
    };
    let knob = if on { ink.on_accent } else { ink.text };
    pen.fill_circle(Point::new(cx, r.y + r.height / 2), knob_r, knob);
}

/// The focus ring round `r`, whose corners have `radius`.
pub fn focus_ring(pen: &mut Pen<'_>, r: Rect, radius: i32, metrics: &Metrics, ink: &Ink) {
    let ring = r.inflate(metrics.px(3));
    pen.stroke_rounded_rect(ring, radius + metrics.px(3), metrics.px(2), ink.text);
}

/// `r` cut into `n` side-by-side parts `gap` apart, for a segmented control.
#[must_use]
pub fn segments(r: Rect, n: usize, gap: i32) -> Vec<Rect> {
    let Ok(count) = i32::try_from(n) else {
        return Vec::new();
    };
    if count == 0 {
        return Vec::new();
    }
    let w = (r.width - gap * (count - 1)) / count;
    (0..count)
        .map(|i| {
            let x = r.x + i * (w + gap);
            // The last takes what rounding left over.
            let width = if i == count - 1 { r.right() - x } else { w };
            Rect::new(x, r.y, width, r.height)
        })
        .collect()
}

/// One part of a segmented control: an icon over, or beside, a label.
pub struct Segment<'a> {
    /// A glyph from the icon font, or nothing.
    pub icon: &'a str,
    /// What it says.
    pub label: &'a str,
    /// Whether it is the chosen one.
    pub chosen: bool,
    /// Whether the pointer is over it.
    pub hovered: bool,
    /// Whether it can be chosen.
    pub enabled: bool,
}

/// Paint a segmented control's parts into the rectangles [`segments`] made.
pub fn segmented(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    rects: &[Rect],
    parts: &[Segment<'_>],
    metrics: &Metrics,
    ink: &Ink,
) {
    let radius = metrics.px(8);
    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
        let track = Rect::new(first.x, first.y, last.right() - first.x, first.height);
        pen.fill_rounded_rect(track, radius, ink.card);
    }
    for (r, part) in rects.iter().zip(parts) {
        let (fill, text) = if part.chosen {
            (Some(ink.accent), ink.on_accent)
        } else if part.hovered && part.enabled {
            (Some(ink.selection), ink.text)
        } else if part.enabled {
            (None, ink.text)
        } else {
            (None, ink.dim)
        };
        if let Some(fill) = fill {
            pen.fill_rounded_rect(*r, radius, fill);
        }
        let engine = &mut fonts.engine;
        let label_w = engine.measure_line(styles.small, part.label);
        let (icon_w, gap) = if part.icon.is_empty() {
            (0, 0)
        } else {
            (
                engine.measure_line(styles.icon, part.icon),
                metrics.unit / 3,
            )
        };
        let total = icon_w + gap + label_w;
        let mut clip = pen.with_clip(*r);
        let x = r.x + ((r.width - total) / 2).max(metrics.px(4));
        if !part.icon.is_empty() {
            let top = text_top(engine, styles.icon, r.y, r.height);
            engine.draw(&mut clip, styles.icon, Point::new(x, top), part.icon, text);
        }
        let top = text_top(engine, styles.small, r.y, r.height);
        engine.draw(
            &mut clip,
            styles.small,
            Point::new(x + icon_w + gap, top),
            part.label,
            text,
        );
    }
}

/// A check box and its label, the box against `r`'s left edge.
#[allow(clippy::too_many_arguments)]
pub fn check(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    r: Rect,
    label: &str,
    checked: bool,
    hovered: bool,
    metrics: &Metrics,
    ink: &Ink,
) {
    if hovered {
        pen.fill_rounded_rect(r, metrics.px(6), ink.selection);
    }
    let side = metrics.unit;
    let b = Rect::new(r.x + metrics.px(6), r.y + (r.height - side) / 2, side, side);
    if checked {
        pen.fill_rounded_rect(b, metrics.px(4), ink.accent);
        centred(
            pen,
            &mut fonts.engine,
            styles.icon_small,
            b,
            CHECK,
            ink.on_accent,
        );
    } else {
        pen.stroke_rounded_rect(b, metrics.px(4), metrics.px(2), ink.dim);
    }
    let x = b.right() + metrics.unit / 2;
    self::label(
        pen,
        &mut fonts.engine,
        styles.small,
        Rect::new(x, r.y, (r.right() - x).max(0), r.height),
        label,
        ink.text,
    );
}

/// A check mark, from Nerd Font's Material Design set.
pub const CHECK: &str = "\u{f012c}";
/// A chevron pointing left.
pub const CHEVRON_LEFT: &str = "\u{f0141}";
/// A chevron pointing right.
pub const CHEVRON_RIGHT: &str = "\u{f0142}";

/// A bar filled to `fraction`, 0 to 1.
pub fn meter(pen: &mut Pen<'_>, r: Rect, fraction: f64, fill: Color, track: Color) {
    let radius = r.height / 2;
    pen.fill_rounded_rect(r, radius, track);
    #[allow(clippy::cast_possible_truncation)]
    let w = (f64::from(r.width) * fraction.clamp(0.0, 1.0)).round() as i32;
    if w > 0 {
        // Never narrower than it is tall, so the rounded ends stay round.
        let w = w.max(r.height).min(r.width);
        pen.fill_rounded_rect(Rect::new(r.x, r.y, w, r.height), radius, fill);
    }
}

/// A setting whose value is stepped through with arrows: its name on the
/// left, `‹ value ›` on the right.
#[allow(clippy::too_many_arguments)]
pub fn stepper(
    pen: &mut Pen<'_>,
    fonts: &mut Fonts,
    styles: &Styles,
    r: Rect,
    name: &str,
    value: &str,
    hovered: bool,
    metrics: &Metrics,
    ink: &Ink,
) {
    if hovered {
        pen.fill_rounded_rect(r, metrics.px(6), ink.selection);
    }
    let inner = Rect::new(r.x + metrics.pad / 2, r.y, r.width - metrics.pad, r.height);
    let engine = &mut fonts.engine;
    label(pen, engine, styles.small, inner, name, ink.text);
    let x = right_label(
        pen,
        engine,
        styles.icon_small,
        inner,
        CHEVRON_RIGHT,
        ink.dim,
    );
    let value_rect = Rect::new(inner.x, inner.y, x - metrics.px(4) - inner.x, inner.height);
    let x = right_label(pen, engine, styles.small, value_rect, value, ink.accent);
    let left = Rect::new(inner.x, inner.y, x - metrics.px(4) - inner.x, inner.height);
    right_label(pen, engine, styles.icon_small, left, CHEVRON_LEFT, ink.dim);
}

/// Label and value pairs in two columns, labels dim and values bright, from
/// `(x, y)` across `width`, a row `row_h` tall. Returns the height used.
#[allow(clippy::too_many_arguments)]
pub fn details(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    ink: &Ink,
    metrics: &Metrics,
    (x, y, width): (i32, i32, i32),
    row_h: i32,
    pairs: &[(&str, String)],
) -> i32 {
    let col_w = width / 2;
    let mut widest = |column: usize| {
        pairs
            .iter()
            .skip(column)
            .step_by(2)
            .map(|(l, _)| engine.measure_line(style, l))
            .max()
            .unwrap_or(0)
    };
    let first = widest(0);
    let label_w = first.max(widest(1));
    for (i, (label, value)) in pairs.iter().enumerate() {
        let column = i32::try_from(i % 2).unwrap_or(0);
        let row = i32::try_from(i / 2).unwrap_or(0);
        let cx = x + column * col_w;
        let cy = y + row * row_h;
        let top = text_top(engine, style, cy, row_h);
        let mut clip = pen.with_clip(Rect::new(cx, cy, col_w - metrics.px(4), row_h));
        engine.draw(&mut clip, style, Point::new(cx, top), label, ink.dim);
        engine.draw(
            &mut clip,
            style,
            Point::new(cx + label_w + metrics.unit / 2, top),
            value,
            ink.text,
        );
    }
    details_height(pairs.len(), row_h)
}

/// How tall [`details`] draws `count` pairs.
#[must_use]
pub fn details_height(count: usize, row_h: i32) -> i32 {
    i32::try_from(count.div_ceil(2)).unwrap_or(0) * row_h
}

/// A line of key hints at the foot of a panel.
pub fn hints(
    pen: &mut Pen<'_>,
    engine: &mut TextEngine,
    style: TextStyle,
    r: Rect,
    text: &str,
    ink: &Ink,
) {
    label(pen, engine, style, r, text, ink.dim);
}

#[cfg(test)]
mod tests {
    use super::{Metrics, details_height, segments};
    use crate::Appearance;
    use denise::geom::Rect;

    #[test]
    fn segments_fill_the_rectangle_exactly() {
        let r = Rect::new(10, 0, 301, 20);
        let parts = segments(r, 3, 4);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].x, 10);
        assert_eq!(parts[2].right(), r.right());
        assert_eq!(parts[1].x, parts[0].right() + 4);
    }

    #[test]
    fn metrics_scale_with_the_output() {
        let a = Appearance::default();
        let one = Metrics::new(&a, 1, 380);
        let two = Metrics::new(&a, 2, 380);
        assert_eq!(two.width, one.width * 2);
        assert_eq!(two.unit, one.unit * 2);
        assert_eq!(details_height(3, 10), 20);
    }
}

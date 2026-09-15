//! Laying out and painting the popup.
//!
//! One panel, top to bottom: the battery — its icon, what it is doing and its
//! charge, a meter, and a card of details; the power modes; what the lid and
//! power button do; what the bar shows; a line for what just went wrong; and
//! a footer of keys. What is there depends on the machine, so the layout is
//! worked out from the popup each time, and the same [`Layout`] answers where
//! a click landed.

use crate::bar;
use crate::battery::{Power, Status};
use crate::popup::{Focus, Popup, Setting, Show, Target};
use crate::profile::Profile;
use alpymist_widget::Appearance;
use alpymist_widget::draw::{self, Ink, Metrics, Segment};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use denise_render::Canvas;

/// Popup width in logical pixels, at a 16 px font.
const WIDTH: i32 = 400;

pub use alpymist_widget::draw::Fonts;

/// Where everything goes, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The sizes everything is measured in.
    pub metrics: Metrics,
    /// The whole panel.
    pub size: Size,
    /// Icon, title, status and charge.
    pub header: Rect,
    /// The charge meter.
    pub meter: Option<Rect>,
    /// The details card.
    pub card: Option<Rect>,
    /// The "Power mode" heading.
    pub modes_heading: Rect,
    /// Each power mode's segment.
    pub modes: Vec<(Profile, Rect)>,
    /// A line said instead of the modes, when there are none.
    pub no_modes: Option<Rect>,
    /// The settings heading.
    pub settings_heading: Rect,
    /// Each setting's row.
    pub settings: Vec<(Setting, Rect)>,
    /// The "Show in bar" heading.
    pub shows_heading: Option<Rect>,
    /// Each check box.
    pub shows: Vec<(Show, Rect)>,
    /// The message line.
    pub message: Option<Rect>,
    /// The footer.
    pub footer: Rect,
}

/// The battery's details, as label and value.
#[must_use]
pub fn details(power: &Power) -> Vec<(&'static str, String)> {
    let Some(b) = power.main() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(w) = power.power_w().filter(|w| *w > 0.05) {
        let label = if power.status() == Status::Charging {
            "Charging"
        } else {
            "Draw"
        };
        out.push((label, format!("{w:.1} W")));
    }
    match (b.energy_wh, b.full_wh) {
        (Some(now), Some(full)) => out.push(("Energy", format!("{now:.1} / {full:.1} Wh"))),
        (Some(now), None) => out.push(("Energy", format!("{now:.1} Wh"))),
        _ => {}
    }
    if let Some(h) = b.health() {
        out.push(("Health", format!("{h}%")));
    }
    if let Some(c) = b.cycles {
        out.push(("Cycles", c.to_string()));
    }
    if let Some(v) = b.voltage_v {
        out.push(("Voltage", format!("{v:.2} V")));
    }
    if let Some(t) = b.temperature_c {
        out.push(("Temp", format!("{t:.0} °C")));
    }
    if let Some(t) = &b.technology {
        out.push(("Type", t.clone()));
    }
    if let Some(m) = b.model.as_ref().or(b.manufacturer.as_ref()) {
        out.push(("Model", m.clone()));
    }
    if power.batteries.len() > 1 {
        out.push(("Batteries", power.batteries.len().to_string()));
    }
    out
}

impl Layout {
    /// Lay out `popup` at an output scale.
    #[must_use]
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub fn new(appearance: &Appearance, popup: &Popup, scale: u32) -> Self {
        let m = Metrics::new(appearance, scale, WIDTH);
        let u = m.unit;
        let x = m.inner_x();
        let w = m.inner_w();
        let reading = popup.reading();
        let power = &reading.power;

        let header = Rect::new(x, m.border + m.pad / 2, w, u * 3);
        let mut y = header.bottom();
        let mut meter = None;
        let mut card = None;
        if power.has_battery() {
            let r = Rect::new(x, y + m.px(6), w, m.px(6));
            meter = Some(r);
            y = r.bottom() + m.px(10);
            let count = details(power).len();
            if count > 0 {
                let rows = draw::details_height(count, u * 5 / 4);
                let r = Rect::new(x, y, w, rows + m.pad);
                card = Some(r);
                y = r.bottom();
            }
        }

        let heading = |y: i32| Rect::new(x, y, w, u * 2);
        let modes_heading = heading(y + m.px(4));
        y = modes_heading.bottom();
        let mut no_modes = None;
        let modes = if reading.profiles.is_empty() {
            let r = Rect::new(x, y, w, u * 3 / 2);
            no_modes = Some(r);
            y = r.bottom();
            Vec::new()
        } else {
            let track = Rect::new(x, y, w, u * 2 + m.px(6));
            y = track.bottom();
            reading
                .profiles
                .iter()
                .copied()
                .zip(draw::segments(track, reading.profiles.len(), m.px(2)))
                .collect()
        };

        let settings_heading = heading(y + m.px(4));
        y = settings_heading.bottom();
        let row_h = u * 2;
        let list_x = m.border + m.pad / 2;
        let list_w = m.width - 2 * m.border - m.pad;
        let settings = popup
            .settings()
            .into_iter()
            .map(|s| {
                let r = Rect::new(list_x, y, list_w, row_h);
                y = r.bottom();
                (s, r)
            })
            .collect();

        let shows_list = popup.shows();
        let mut shows_heading = None;
        let mut shows = Vec::new();
        if !shows_list.is_empty() {
            let h = heading(y + m.px(4));
            shows_heading = Some(h);
            y = h.bottom();
            let col_w = list_w / 2;
            for (i, show) in shows_list.into_iter().enumerate() {
                let column = i32::try_from(i % 2).unwrap_or(0);
                let row = i32::try_from(i / 2).unwrap_or(0);
                shows.push((
                    show,
                    Rect::new(
                        list_x + column * col_w,
                        y + row * row_h,
                        col_w - m.px(4),
                        row_h,
                    ),
                ));
            }
            y = shows.last().map_or(y, |(_, r)| r.bottom());
        }

        let message = popup.message().map(|_| {
            let r = Rect::new(x, y + m.px(4), w, u * 7 / 4);
            y = r.bottom();
            r
        });
        let footer = Rect::new(x, y + m.px(4), w, u * 7 / 4);

        Self {
            metrics: m,
            size: m.size(footer.bottom()),
            header,
            meter,
            card,
            modes_heading,
            modes,
            no_modes,
            settings_heading,
            settings,
            shows_heading,
            shows,
            message,
            footer,
        }
    }

    /// What is under `point`.
    #[must_use]
    pub fn hit(&self, point: Point) -> Option<Target> {
        if let Some((p, _)) = self.modes.iter().find(|(_, r)| r.contains(point)) {
            return Some(Target::Profile(*p));
        }
        if let Some((s, _)) = self.settings.iter().find(|(_, r)| r.contains(point)) {
            return Some(Target::Setting(*s));
        }
        self.shows
            .iter()
            .find(|(_, r)| r.contains(point))
            .map(|(s, _)| Target::Show(*s))
    }

    fn modes_track(&self) -> Option<Rect> {
        let (first, last) = (self.modes.first()?.1, self.modes.last()?.1);
        Some(Rect::new(
            first.x,
            first.y,
            last.right() - first.x,
            first.height,
        ))
    }
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
    let m = &layout.metrics;
    let u = m.unit;
    let ink = Ink::new(appearance);
    let st = fonts.styles(m);
    draw::panel(&mut pen, layout.size, m, &ink);
    let reading = popup.reading();
    let power = &reading.power;
    let hover = popup.hover();

    // Header: the icon, what the battery is doing, and its charge.
    let h = layout.header;
    let level = power.level();
    let (icon, icon_ink) = if power.has_battery() {
        let alarm =
            level.is_some_and(|l| l <= bar::CRITICAL) && power.status() == Status::Discharging;
        (bar::icon(power), if alarm { ink.warn } else { ink.accent })
    } else {
        (bar::PLUG, ink.accent)
    };
    let icon_box = Rect::new(h.x - m.px(4), h.y, u * 2, h.height);
    draw::centred(
        &mut pen,
        &mut fonts.engine,
        st.icon_large,
        icon_box,
        icon,
        icon_ink,
    );
    let tx = icon_box.right() + u / 2;
    let figure = level.map(|l| format!("{l}%"));
    let figure_x = figure.as_deref().map_or(h.right(), |f| {
        draw::right_label(&mut pen, &mut fonts.engine, st.large, h, f, ink.text)
    });
    let text_w = (figure_x - tx - u / 2).max(0);
    let title = if power.has_battery() {
        "Battery"
    } else {
        "Power"
    };
    let line_h = h.height / 2;
    draw::label(
        &mut pen,
        &mut fonts.engine,
        st.strong,
        Rect::new(tx, h.y + m.px(2), text_w, line_h),
        title,
        ink.text,
    );
    let status = if popup.loaded() {
        bar::status_line(power)
    } else {
        "Reading the battery…".into()
    };
    draw::label(
        &mut pen,
        &mut fonts.engine,
        st.small,
        Rect::new(tx, h.y + line_h - m.px(2), text_w, line_h),
        &status,
        ink.dim,
    );

    if let (Some(r), Some(l)) = (layout.meter, level) {
        let fill = match power.status() {
            Status::Discharging if l <= bar::WARNING => ink.warn,
            _ => ink.accent,
        };
        draw::meter(&mut pen, r, f64::from(l) / 100.0, fill, ink.selection);
    }

    if let Some(r) = layout.card {
        draw::card(&mut pen, r, m, &ink);
        draw::details(
            &mut pen,
            &mut fonts.engine,
            st.small,
            &ink,
            m,
            (r.x + m.pad / 2 + m.px(2), r.y + m.pad / 2, r.width - m.pad),
            u * 5 / 4,
            &details(power),
        );
    }

    // Power modes.
    let heading = |pen: &mut denise::painter::Pen<'_>, fonts: &mut Fonts, r: Rect, text: &str| {
        draw::label(pen, &mut fonts.engine, st.small, r, text, ink.dim);
    };
    let modes_title = if popup.switching() {
        "Power mode · switching…"
    } else {
        "Power mode"
    };
    heading(&mut pen, fonts, layout.modes_heading, modes_title);
    if let Some(r) = layout.no_modes {
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.small,
            r,
            "This machine offers no power modes.",
            ink.text,
        );
    }
    let chosen = popup.profile();
    let parts: Vec<Segment<'_>> = layout
        .modes
        .iter()
        .map(|(p, _)| Segment {
            icon: p.icon(),
            label: p.label(),
            chosen: chosen == Some(*p),
            hovered: hover == Some(Target::Profile(*p)),
            enabled: !popup.switching(),
        })
        .collect();
    let rects: Vec<Rect> = layout.modes.iter().map(|(_, r)| *r).collect();
    draw::segmented(&mut pen, fonts, &st, &rects, &parts, m, &ink);

    // What the lid and power button do.
    heading(
        &mut pen,
        fonts,
        layout.settings_heading,
        if power.has_battery() {
            "Lid and power button"
        } else {
            "Buttons"
        },
    );
    for (setting, r) in &layout.settings {
        draw::stepper(
            &mut pen,
            fonts,
            &st,
            *r,
            setting.label(),
            &popup.value(*setting),
            hover == Some(Target::Setting(*setting)),
            m,
            &ink,
        );
    }

    // What the bar shows.
    if let Some(r) = layout.shows_heading {
        heading(&mut pen, fonts, r, "Show in the bar");
    }
    for (show, r) in &layout.shows {
        draw::check(
            &mut pen,
            fonts,
            &st,
            *r,
            show.label(),
            popup.shown(*show),
            hover == Some(Target::Show(*show)),
            m,
            &ink,
        );
    }

    if let (Some(r), Some(message)) = (layout.message, popup.message()) {
        draw::label(
            &mut pen,
            &mut fonts.engine,
            st.small,
            r,
            &message.text,
            if message.error { ink.warn } else { ink.accent },
        );
    }

    if popup.focus_visible() {
        let ring = match popup.focus() {
            Focus::Profiles => layout.modes_track().map(|r| (r, m.px(8))),
            Focus::Setting(s) => layout
                .settings
                .iter()
                .find(|(t, _)| *t == s)
                .map(|(_, r)| (*r, m.px(6))),
            Focus::Show(s) => layout
                .shows
                .iter()
                .find(|(t, _)| *t == s)
                .map(|(_, r)| (*r, m.px(6))),
        };
        if let Some((r, radius)) = ring {
            draw::focus_ring(&mut pen, r, radius, m, &ink);
        }
    }

    let hints = match popup.focus() {
        Focus::Profiles if popup.focus_visible() => "←→ choose mode    tab move    esc close",
        Focus::Setting(_) if popup.focus_visible() => "←→ change    tab move    esc close",
        Focus::Show(_) if popup.focus_visible() => "space tick    tab move    esc close",
        _ => "tab move    ←→ change    esc close",
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
    use crate::battery::sample;
    use crate::config::Config;
    use crate::popup::{Popup, Reading, Setting, Show, Target};
    use crate::profile::Profile;
    use alpymist_widget::Appearance;
    use denise::geom::{Point, Rect};

    fn centre(r: Rect) -> Point {
        Point::new(r.x + r.width / 2, r.y + r.height / 2)
    }

    fn popup() -> Popup {
        let mut p = Popup::new(Config::default());
        p.update(Reading {
            power: sample(),
            profile: Some(Profile::Balanced),
            profiles: Profile::ALL.to_vec(),
            can_hibernate: false,
        });
        p
    }

    #[test]
    fn everything_is_hit_where_it_is_drawn() {
        let l = Layout::new(&Appearance::default(), &popup(), 1);
        for (p, r) in &l.modes {
            assert_eq!(l.hit(centre(*r)), Some(Target::Profile(*p)));
        }
        for (s, r) in &l.settings {
            assert_eq!(l.hit(centre(*r)), Some(Target::Setting(*s)));
        }
        for (s, r) in &l.shows {
            assert_eq!(l.hit(centre(*r)), Some(Target::Show(*s)));
        }
        assert_eq!(l.settings.len(), 4);
        assert!(l.settings.iter().any(|(s, _)| *s == Setting::ChargeLimit));
        assert!(l.shows.iter().any(|(s, _)| *s == Show::Profile));
        assert_eq!(l.hit(centre(l.header)), None);
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

//! The Displays page: the screens drawn to scale, dragged to arrange them,
//! and each one's resolution, scale, rotation and whether it is on (#17,
//! ADR 0015).
//!
//! Nothing changes until Apply. Then the new layout is put in place and a
//! bar asks whether to keep it, going back to the one before on its own
//! after [`KEEP_S`] seconds: a resolution the screen cannot show, or every
//! screen turned the wrong way, must not leave anyone without a picture they
//! can use to undo it.
//!
//! The screens come from the window, which asks Hyprland, through
//! [`View::screens`]; the page hands a layout back as [`Effect::Layout`].

use super::{CONTROL_H, Effect, GAP, Msg, PAD, Page, ROW_H, SELECT_W, TOGGLE_H, TOGGLE_W, View};
use alpymist_displays::arrange::{self, Room};
use alpymist_displays::layout::{Layout, Output};
use alpymist_displays::screen::{self, Monitor};
use denise::painter::Pen;
use denise::theme::Role;
use denise::{ElementState, InputEvent, KeyCode, Modifiers, Point, Rect};
use denise_text::TextStyle;
use denise_ui::NodeId;
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};
use denise_ui::widgets::{Button, Label, Panel, Select, Toggle, open_select};

/// The area the page is.
pub const AREA: &str = "displays";

/// How long a new layout waits to be kept before it is undone.
pub const KEEP_S: u64 = 15;

/// How far Shift and an arrow slide a screen, in the layout's pixels.
const SLIDE: i32 = 10;

/// The height of the arrangement, in logical pixels.
const ARRANGE_H: i32 = 220;

/// Scales offered, beside whatever the screen has now.
const SCALES: [f64; 7] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// Rotations, as `wl_output` numbers them without flipping.
const TURNS: [&str; 4] = [
    "Normal",
    "Turned right (90°)",
    "Upside down (180°)",
    "Turned left (270°)",
];

/// A choice list on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Which screen the controls are for.
    Screen,
    /// Its resolution and refresh rate.
    Mode,
    /// Its scale.
    Scale,
    /// Its rotation.
    Rotation,
    /// A picture of its own, or the same as another screen's.
    Mirror,
}

/// A control on the page that can have focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The screens drawn to scale.
    Arrangement,
    /// A choice list.
    Choice(Field),
}

/// What the page's controls say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenMsg {
    /// A screen was pressed in the arrangement, or chosen in the list.
    Pick(usize),
    /// The arrangement moved: each screen that is on, and where it is now.
    Arranged(Vec<(usize, i32, i32)>),
    /// A choice list was asked to open.
    Open(Field),
    /// Something was chosen in the open list.
    Chose(usize),
    /// The chosen screen was turned on or off.
    On(bool),
    /// Show each screen's number on it.
    Identify,
    /// Put the new layout in place.
    Apply,
    /// Keep it.
    Keep,
    /// Go back to the layout before.
    Revert,
    /// Forget the changes not yet applied.
    Reset,
}

/// A layout put in place and not yet kept.
struct Trial {
    /// The layout to go back to.
    before: Layout,
    /// When it goes back on its own, in ms.
    deadline_ms: u64,
}

/// The screens, and what is being made of them.
pub struct Screens {
    /// As Hyprland has them, in its order; numbered from 1 in it.
    monitors: Vec<Monitor>,
    /// Their names in a layout.
    names: Vec<String>,
    /// The layout in place.
    applied: Layout,
    /// The layout being made, one output for each monitor, in its order.
    draft: Layout,
    /// The screen the controls are for.
    selected: usize,
    /// A layout put in place and not yet kept.
    trial: Option<Trial>,
    /// The choice list open, while one is.
    open: Option<Field>,
    /// The page's controls, while it is built.
    nodes: Nodes,
    /// Why the screens could not be read, if they could not.
    error: Option<String>,
    /// The time last seen, in ms.
    now_ms: u64,
}

#[derive(Default)]
struct Nodes {
    arrangement: Option<NodeId>,
    selects: Vec<(Field, NodeId)>,
    countdown: Option<NodeId>,
}

impl Screens {
    /// The screens Hyprland reports, and the layout remembered for them if
    /// there is one; as they are otherwise.
    #[must_use]
    pub fn new(monitors: Vec<Monitor>, remembered: Option<&Layout>) -> Self {
        let names = screen::names(&monitors);
        let outputs = names
            .iter()
            .zip(&monitors)
            .map(|(name, m)| {
                remembered
                    .and_then(|l| l.outputs.iter().find(|o| &o.screen == name))
                    .cloned()
                    .unwrap_or_else(|| Output::of(name, m))
            })
            .collect();
        let applied = Layout { outputs };
        Self {
            monitors,
            names,
            draft: applied.clone(),
            applied,
            selected: 0,
            trial: None,
            open: None,
            nodes: Nodes::default(),
            error: None,
            now_ms: 0,
        }
    }

    /// Screens that could not be read, and why.
    #[must_use]
    pub fn failed(why: String) -> Self {
        Self {
            error: Some(why),
            ..Self::new(Vec::new(), None)
        }
    }

    fn changed(&self) -> bool {
        self.draft != self.applied
    }

    /// The seconds left before a trial goes back, if one is running.
    fn left_s(&self) -> Option<u64> {
        self.trial
            .as_ref()
            .map(|t| t.deadline_ms.saturating_sub(self.now_ms).div_ceil(1000))
    }

    /// Each screen that is on, as room in the layout, and which monitor.
    fn rooms(&self) -> (Vec<Room>, Vec<usize>) {
        let mut rooms = Vec::new();
        let mut index = Vec::new();
        for (i, (o, m)) in self.draft.outputs.iter().zip(&self.monitors).enumerate() {
            // A screen showing the same as another has no place of its own.
            if !o.placed() {
                continue;
            }
            let (w, h) = arrange::size(o, m);
            rooms.push(Room {
                x: o.position[0],
                y: o.position[1],
                w,
                h,
            });
            index.push(i);
        }
        (rooms, index)
    }

    /// After a change of size, turning or which screens are on: put every
    /// screen that is on against the others again, so none overlap and
    /// there is no gap, keeping the order they were in.
    fn settle(&mut self) {
        let (mut rooms, index) = self.rooms();
        if rooms.is_empty() {
            return;
        }
        // Left to right, top to bottom, each dropped where it was.
        let mut order: Vec<usize> = (0..rooms.len()).collect();
        order.sort_by_key(|&i| (rooms[i].x, rooms[i].y));
        let mut placed: Vec<Room> = Vec::new();
        let mut placed_index: Vec<usize> = Vec::new();
        for &i in &order {
            placed.push(rooms[i]);
            placed_index.push(i);
            let at = (rooms[i].x, rooms[i].y);
            let n = placed.len() - 1;
            if n > 0 {
                arrange::drop(&mut placed, n, at);
            } else {
                placed[0].x = 0;
                placed[0].y = 0;
            }
        }
        for (room, i) in placed.iter().zip(&placed_index) {
            rooms[*i] = *room;
        }
        for (room, &m) in rooms.iter().zip(&index) {
            self.draft.outputs[m].position = [room.x, room.y];
        }
    }

    /// The modes the chosen screen offers, as the rule and as read.
    fn modes(&self) -> Vec<(String, String)> {
        let Some(m) = self.monitors.get(self.selected) else {
            return Vec::new();
        };
        let mut modes: Vec<(String, String)> = m
            .available_modes
            .iter()
            .filter_map(|mode| {
                let rule = mode.trim_end_matches("Hz").to_owned();
                let (w, h) = arrange::pixels(&rule)?;
                let rate: f64 = rule.split_once('@')?.1.parse().ok()?;
                Some((rule, format!("{w} × {h} · {rate:.0} Hz")))
            })
            .collect();
        let current = &self.draft.outputs[self.selected].mode;
        if !modes.iter().any(|(rule, _)| rule == current) {
            let label = if current == "preferred" {
                "The screen's own".to_owned()
            } else {
                current.clone()
            };
            modes.insert(0, (current.clone(), label));
        }
        modes
    }

    /// What the chosen screen can show: its own picture, or the same as
    /// each other screen that has one of its own.
    fn mirrors(&self) -> Vec<Option<usize>> {
        let mut choices = vec![None];
        choices.extend(
            (0..self.monitors.len())
                .filter(|&j| j != self.selected && self.draft.outputs[j].placed())
                .map(Some),
        );
        choices
    }

    /// The scales offered to the chosen screen.
    fn scales(&self) -> Vec<f64> {
        let now = self.draft.outputs[self.selected].scale;
        let mut scales = SCALES.to_vec();
        if !scales.iter().any(|s| (s - now).abs() < 0.001) {
            scales.push(now);
            scales.sort_by(f64::total_cmp);
        }
        scales
    }

    fn label(&self, i: usize) -> String {
        let Some(m) = self.monitors.get(i) else {
            return String::new();
        };
        if m.internal() {
            // On in the layout, off in fact: the lid is closed, and the
            // layout keeps its place for when it opens.
            let lid = m.disabled && self.draft.outputs.get(i).is_some_and(|o| o.enabled);
            let off = if lid {
                ", off while the lid is closed"
            } else {
                ""
            };
            format!("{} · {}{off}", i + 1, m.short())
        } else {
            format!("{} · {} ({})", i + 1, m.short(), m.name)
        }
    }
}

/// A row's choice list: which, what it offers, and which is chosen; or
/// none, for the row with the switch.
type Choices = Option<(Field, Vec<String>, Option<usize>)>;

/// A screen being dragged.
#[derive(Debug, Clone, Copy)]
struct Grab {
    /// Which room.
    room: usize,
    /// Where in it it was taken hold of, in layout pixels.
    hold: (i32, i32),
    /// Where the pointer is now, in layout pixels.
    at: (i32, i32),
    /// Whether it has moved far enough to be a drag rather than a press.
    moved: bool,
}

/// The screens as drawn to scale, pressed and dragged to arrange them.
pub struct Arrangement {
    rooms: Vec<Room>,
    /// Which monitor each room is.
    index: Vec<usize>,
    /// What each says: its number, and its name.
    tags: Vec<(String, String)>,
    /// The monitor chosen.
    selected: usize,
    /// A room being dragged: which, where it was grabbed within it, and
    /// where it is now, in layout pixels.
    grab: Option<Grab>,
    number: TextStyle,
    name: TextStyle,
}

impl Arrangement {
    /// Where the layout is drawn in `bounds`: how many screen pixels a
    /// layout pixel is, and where the layout's 0,0 is.
    fn fit(&self, bounds: Rect) -> (f64, (f64, f64)) {
        let right = self.rooms.iter().map(|r| r.x + r.w).max().unwrap_or(1);
        let bottom = self.rooms.iter().map(|r| r.y + r.h).max().unwrap_or(1);
        let pad = f64::from(bounds.height.min(bounds.width) / 10);
        let fx = (f64::from(bounds.width) - 2.0 * pad) / f64::from(right.max(1));
        let fy = (f64::from(bounds.height) - 2.0 * pad) / f64::from(bottom.max(1));
        let f = fx.min(fy).max(0.0001);
        let x = f64::from(bounds.x) + (f64::from(bounds.width) - f * f64::from(right)) / 2.0;
        let y = f64::from(bounds.y) + (f64::from(bounds.height) - f * f64::from(bottom)) / 2.0;
        (f, (x, y))
    }

    #[allow(clippy::cast_possible_truncation)] // a layout is not 2³¹ pixels
    fn on_screen(&self, bounds: Rect, room: Room) -> Rect {
        let (f, (x, y)) = self.fit(bounds);
        let px = |v: f64| v.round() as i32;
        Rect::new(
            px(x + f * f64::from(room.x)),
            px(y + f * f64::from(room.y)),
            px(f * f64::from(room.w)).max(1),
            px(f * f64::from(room.h)).max(1),
        )
    }

    #[allow(clippy::cast_possible_truncation)]
    fn in_layout(&self, bounds: Rect, p: Point) -> (i32, i32) {
        let (f, (x, y)) = self.fit(bounds);
        (
            ((f64::from(p.x) - x) / f).round() as i32,
            ((f64::from(p.y) - y) / f).round() as i32,
        )
    }

    fn arranged(&self, rooms: &[Room]) -> ScreenMsg {
        ScreenMsg::Arranged(
            rooms
                .iter()
                .zip(&self.index)
                .map(|(r, &m)| (m, r.x, r.y))
                .collect(),
        )
    }
}

impl Widget<Msg> for Arrangement {
    fn paint(&self, ctx: &mut PaintCtx<'_>, pen: &mut Pen<'_>) {
        let theme = ctx.theme;
        for (i, room) in self.rooms.iter().enumerate() {
            let mut room = *room;
            if let Some(g) = self.grab
                && g.room == i
            {
                room.x = g.at.0 - g.hold.0;
                room.y = g.at.1 - g.hold.1;
            }
            // A hairline between screens that touch, so each is its own.
            let r = self.on_screen(ctx.bounds, room);
            let r = Rect::new(
                r.x + 1,
                r.y + 1,
                (r.width - 2).max(1),
                (r.height - 2).max(1),
            );
            let chosen = self.index[i] == self.selected;
            let radius = (r.height / 12).clamp(2, 10);
            pen.fill_rounded_rect(r, radius, theme.color(Role::Base300));
            let (width, edge) = if chosen {
                (3, theme.color(Role::Primary))
            } else {
                (1, theme.color(Role::Neutral))
            };
            pen.stroke_rounded_rect(r, radius, width, edge);
            let (number, name) = &self.tags[i];
            let mut clip = pen.with_clip(r);
            let n = ctx.text.measure(self.number, number);
            let line = ctx.text.line_height(self.name);
            let n_w = i32::try_from(n.width).unwrap_or(0);
            let n_h = i32::try_from(n.height).unwrap_or(0);
            let top = r.y + (r.height - n_h - line) / 2;
            let colour = if chosen {
                theme.color(Role::Primary)
            } else {
                theme.color(Role::BaseContent)
            };
            ctx.text.draw(
                &mut clip,
                self.number,
                Point::new(r.x + (r.width - n_w) / 2, top),
                number,
                colour,
            );
            let w = ctx.text.measure_line(self.name, name);
            ctx.text.draw(
                &mut clip,
                self.name,
                Point::new(r.x + ((r.width - w) / 2).max(4), top + n_h),
                name,
                theme.color(Role::Secondary),
            );
        }
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, Msg>) -> Handled {
        let bounds = ctx.bounds;
        match event {
            Event::Input(InputEvent::PointerButton {
                state: ElementState::Down,
                position,
                ..
            }) => {
                let hit = (0..self.rooms.len())
                    .rev()
                    .find(|&i| self.on_screen(bounds, self.rooms[i]).contains(*position));
                ctx.request_focus();
                let Some(i) = hit else {
                    return Handled::Yes;
                };
                let at = self.in_layout(bounds, *position);
                self.grab = Some(Grab {
                    room: i,
                    hold: (at.0 - self.rooms[i].x, at.1 - self.rooms[i].y),
                    at,
                    moved: false,
                });
                self.selected = self.index[i];
                ctx.emit(Msg::Screen(ScreenMsg::Pick(self.index[i])));
                ctx.invalidate();
                Handled::Yes
            }
            Event::Input(InputEvent::PointerMoved { position }) => {
                let Some(g) = self.grab else {
                    return Handled::No;
                };
                let at = self.in_layout(bounds, *position);
                let (f, _) = self.fit(bounds);
                #[allow(clippy::cast_possible_truncation)]
                let slack = (4.0 / f) as i32;
                // A press that wanders a pixel or two is still a press.
                let moved =
                    g.moved || (at.0 - g.at.0).abs() > slack || (at.1 - g.at.1).abs() > slack;
                if moved {
                    self.grab = Some(Grab { at, moved, ..g });
                }
                ctx.invalidate();
                Handled::Yes
            }
            Event::Input(InputEvent::PointerButton {
                state: ElementState::Up,
                ..
            }) => {
                let Some(g) = self.grab.take() else {
                    return Handled::No;
                };
                if g.moved {
                    let mut rooms = self.rooms.clone();
                    arrange::drop(&mut rooms, g.room, (g.at.0 - g.hold.0, g.at.1 - g.hold.1));
                    ctx.emit(Msg::Screen(self.arranged(&rooms)));
                    self.rooms = rooms;
                }
                ctx.invalidate();
                Handled::Yes
            }
            Event::PressCancelled => {
                self.grab = None;
                ctx.invalidate();
                Handled::Yes
            }
            Event::Input(InputEvent::Key {
                code,
                state: ElementState::Down,
                modifiers,
                ..
            }) => {
                let way = match code {
                    KeyCode::ArrowLeft => (-1, 0),
                    KeyCode::ArrowRight => (1, 0),
                    KeyCode::ArrowUp => (0, -1),
                    KeyCode::ArrowDown => (0, 1),
                    _ => return Handled::No,
                };
                let Some(i) = self.index.iter().position(|&m| m == self.selected) else {
                    return Handled::No;
                };
                let mut rooms = self.rooms.clone();
                // With Shift, a little way along the edge it is against: for
                // lining it up as it is on the desk.
                let moved = if modifiers.contains(Modifiers::SHIFT) {
                    arrange::slide(&mut rooms, i, way, SLIDE)
                } else {
                    arrange::nudge(&mut rooms, i, way)
                };
                if moved {
                    ctx.emit(Msg::Screen(self.arranged(&rooms)));
                    self.rooms = rooms;
                    ctx.invalidate();
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn accepts_pointer(&self) -> bool {
        true
    }

    fn focusable(&self) -> bool {
        true
    }
}

impl View {
    /// The screens as they are now, from the window: at the start, and after
    /// a layout was put in place.
    pub fn screens(&mut self, screens: Screens) {
        let keep = self.screens.take();
        let mut screens = screens;
        if let Some(old) = keep {
            // Still the same set: carry on where the page was.
            if old.names == screens.names {
                screens.selected = old.selected.min(screens.monitors.len().saturating_sub(1));
                screens.trial = old.trial;
                screens.now_ms = old.now_ms;
            }
        }
        self.screens = Some(screens);
        if self.on_displays() {
            self.build();
        }
    }

    /// The screens did not take the layout just applied, and the one before
    /// is back: there is nothing left to keep.
    pub fn layout_refused(&mut self) {
        if let Some(s) = self.screens.as_mut() {
            s.trial = None;
        }
    }

    /// Whether the Displays page is up.
    pub(super) fn on_displays(&self) -> bool {
        matches!(self.page, Page::Area(i) if self.settings.pages().get(i).is_some_and(|a| a.id == AREA))
    }

    /// When the page next needs waking: the countdown's next second.
    pub(super) fn displays_wake_ms(&self) -> Option<u64> {
        let s = self.screens.as_ref()?;
        let t = s.trial.as_ref()?;
        let left = t.deadline_ms.saturating_sub(s.now_ms);
        Some(s.now_ms + (left % 1000).max(1).min(left.max(1)))
    }

    /// Time passes: count down, and go back when the time is up.
    pub(super) fn displays_tick(&mut self, now_ms: u64, effects: &mut Vec<Effect>) {
        let Some(s) = self.screens.as_mut() else {
            return;
        };
        let before = s.left_s();
        s.now_ms = now_ms;
        if let Some(t) = &s.trial
            && now_ms >= t.deadline_ms
        {
            self.displays_message(ScreenMsg::Revert, effects);
            return;
        }
        if s.left_s() != before
            && let (Some(node), Some(left)) = (s.nodes.countdown, s.left_s())
            && let Some(label) = self.ui.widget_mut::<Label>(node)
        {
            label.set_text(countdown(left));
        }
    }

    /// What the page's controls said.
    #[allow(clippy::too_many_lines)] // one arm a control
    pub(super) fn displays_message(&mut self, m: ScreenMsg, effects: &mut Vec<Effect>) {
        let Some(s) = self.screens.as_mut() else {
            return;
        };
        let mut rebuild = true;
        match m {
            ScreenMsg::Pick(i) => {
                if i == s.selected {
                    return;
                }
                s.selected = i;
            }
            ScreenMsg::Arranged(places) => {
                for (m, x, y) in places {
                    if let Some(o) = s.draft.outputs.get_mut(m) {
                        o.position = [x, y];
                    }
                }
            }
            ScreenMsg::Open(field) => {
                rebuild = false;
                let node = s
                    .nodes
                    .selects
                    .iter()
                    .find(|(f, _)| *f == field)
                    .map(|(_, n)| *n);
                if let Some(node) = node {
                    s.open = Some(field);
                    open_select(&mut self.ui, node, |i| Msg::Screen(ScreenMsg::Chose(i)));
                }
            }
            ScreenMsg::Chose(choice) => {
                self.ui.close_popup();
                let Some(field) = s.open.take() else {
                    return;
                };
                let i = s.selected;
                match field {
                    Field::Screen => s.selected = choice.min(s.monitors.len().saturating_sub(1)),
                    Field::Mode => {
                        if let Some((rule, _)) = s.modes().get(choice).cloned() {
                            s.draft.outputs[i].mode = rule;
                            s.settle();
                        }
                    }
                    Field::Scale => {
                        if let Some(&scale) = s.scales().get(choice) {
                            s.draft.outputs[i].scale = scale;
                            s.settle();
                        }
                    }
                    Field::Mirror => {
                        if let Some(&choice) = s.mirrors().get(choice) {
                            s.draft.outputs[i].mirror = choice.map(|j| s.names[j].clone());
                            s.settle();
                        }
                    }
                    Field::Rotation => {
                        let flip = s.draft.outputs[i].transform & 4;
                        s.draft.outputs[i].transform = u8::try_from(choice % 4).unwrap_or(0) | flip;
                        s.settle();
                    }
                }
            }
            ScreenMsg::On(on) => {
                let i = s.selected;
                let others_on = s
                    .draft
                    .outputs
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != i && o.enabled);
                if !on && !others_on {
                    self.ui
                        .toast("At least one screen has to stay on.", Role::Warning);
                } else {
                    s.draft.outputs[i].enabled = on;
                    s.settle();
                }
            }
            ScreenMsg::Identify => {
                rebuild = false;
                effects.push(Effect::Action(super::Action::Identify));
            }
            ScreenMsg::Apply => {
                if !s.changed() {
                    return;
                }
                s.trial = Some(Trial {
                    before: s.applied.clone(),
                    deadline_ms: s.now_ms + KEEP_S * 1000,
                });
                let before = s.applied.clone();
                s.applied = s.draft.clone();
                effects.push(Effect::Layout {
                    layout: s.draft.clone(),
                    before: Some(before),
                });
            }
            ScreenMsg::Keep => s.trial = None,
            ScreenMsg::Revert => {
                if let Some(t) = s.trial.take() {
                    s.applied = t.before.clone();
                    s.draft = t.before.clone();
                    effects.push(Effect::Layout {
                        layout: t.before,
                        before: None,
                    });
                }
            }
            ScreenMsg::Reset => s.draft = s.applied.clone(),
        }
        if rebuild && self.on_displays() {
            self.build();
        }
    }

    /// The arrangement, the chosen screen's controls, and Apply, from `y`
    /// down. Returns where the page's settings go after it.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)] // the page, top to bottom
    pub(super) fn build_displays(&mut self, parent: NodeId, y: i32, width: i32) -> i32 {
        let sc = self.scale;
        let text = self.style(self.text, 15);
        let dim = self.style(self.text, 13);
        let Some(s) = self.screens.as_mut() else {
            return y;
        };
        s.nodes = Nodes::default();
        if let Some(why) = s.error.clone() {
            self.add_label(
                parent,
                &format!("The screens could not be read: {why}"),
                text,
                Role::Warning,
                Rect::new(PAD * sc, y, width, 24 * sc),
            );
            return y + (24 + GAP) * sc;
        }
        if s.monitors.is_empty() {
            return y;
        }
        let (rooms, index) = s.rooms();
        let tags = index
            .iter()
            .map(|&i| {
                let short = if s.monitors[i].internal() {
                    "Built-in".to_owned()
                } else {
                    s.monitors[i].short()
                };
                ((i + 1).to_string(), short)
            })
            .collect();
        let arrangement = Arrangement {
            rooms,
            index,
            tags,
            selected: s.selected,
            grab: None,
            number: TextStyle {
                font: self.strong,
                size_px: super::px(28, sc),
            },
            name: TextStyle {
                font: self.text,
                size_px: super::px(12, sc),
            },
        };
        let mut y = y;
        let card = self.ui.add(
            parent,
            Panel::default(),
            Rect::new(PAD * sc, y, width, ARRANGE_H * sc),
        );
        let arranged = card.and_then(|card| {
            self.ui
                .add(card, arrangement, Rect::new(0, 0, width, ARRANGE_H * sc))
        });
        let Some(s) = self.screens.as_mut() else {
            return y;
        };
        s.nodes.arrangement = arranged;
        y += (ARRANGE_H + GAP) * sc;
        let hint = "Drag a screen to where it is on the desk. With the keyboard: the arrows move it \
                    beside another, Shift and the arrows a little at a time.";
        for line in super::wrap(&mut self.ui, dim, hint, width) {
            self.add_label(
                parent,
                &line,
                dim,
                Role::Secondary,
                Rect::new(PAD * sc, y, width, 18 * sc),
            );
            y += 18 * sc;
        }
        y += GAP * sc;

        // The chosen screen.
        let Some(s) = self.screens.as_ref() else {
            return y;
        };
        let i = s.selected;
        let o = s.draft.outputs[i].clone();
        let screen_labels: Vec<String> = (0..s.monitors.len()).map(|j| s.label(j)).collect();
        let modes = s.modes();
        let mode_at = modes.iter().position(|(rule, _)| *rule == o.mode);
        let scales = s.scales();
        let scale_at = scales.iter().position(|v| (v - o.scale).abs() < 0.001);
        let mirrors = s.mirrors();
        let mirror_at = mirrors
            .iter()
            .position(|m| m.map(|j| &s.names[j]) == o.mirror.as_ref());
        let mirror_labels = mirrors
            .iter()
            .map(|m| {
                m.map_or_else(
                    || "Its own".to_owned(),
                    |j| format!("The same as {}", s.label(j)),
                )
            })
            .collect();
        let rows: [(&str, Choices); 6] = [
            ("Screen", Some((Field::Screen, screen_labels, Some(i)))),
            ("Show things on it", None),
            ("Picture", Some((Field::Mirror, mirror_labels, mirror_at))),
            (
                "Resolution",
                Some((
                    Field::Mode,
                    modes.into_iter().map(|(_, l)| l).collect(),
                    mode_at,
                )),
            ),
            (
                "Scale",
                Some((
                    Field::Scale,
                    scales
                        .iter()
                        .map(|v| format!("{:.0} %", v * 100.0))
                        .collect(),
                    scale_at,
                )),
            ),
            (
                "Rotation",
                Some((
                    Field::Rotation,
                    TURNS.iter().map(|t| (*t).to_owned()).collect(),
                    Some(usize::from(o.transform % 4)),
                )),
            ),
        ];
        let row_h = ROW_H * sc * 3 / 4;
        let Some(card) = self.ui.add(
            parent,
            Panel::default(),
            Rect::new(PAD * sc, y, width, row_h * 6),
        ) else {
            return y;
        };
        let right = width - PAD * sc;
        let title = self.style(self.strong, 15);
        let mut selects = Vec::new();
        for (n, (label, choice)) in rows.into_iter().enumerate() {
            let top = i32::try_from(n).unwrap_or(0) * row_h;
            let middle = top + row_h / 2;
            self.add_label(
                card,
                label,
                title,
                Role::BaseContent,
                Rect::new(PAD * sc, middle - 11 * sc, width / 2, 22 * sc),
            );
            let off = !o.enabled && n > 1;
            match choice {
                None => {
                    let t = Toggle::new("", |on| Msg::Screen(ScreenMsg::On(on)))
                        .with_checked(o.enabled);
                    self.ui.add(
                        card,
                        t,
                        Rect::new(
                            right - TOGGLE_W * sc,
                            middle - TOGGLE_H * sc / 2,
                            TOGGLE_W * sc,
                            TOGGLE_H * sc,
                        ),
                    );
                }
                Some((field, labels, at)) => {
                    let select = Select::new(labels, Msg::Screen(ScreenMsg::Open(field)))
                        .with_selected(at)
                        .with_style(text);
                    if let Some(node) = self.ui.add(
                        card,
                        select,
                        Rect::new(
                            right - SELECT_W * sc * 5 / 4,
                            middle - CONTROL_H * sc / 2,
                            SELECT_W * sc * 5 / 4,
                            CONTROL_H * sc,
                        ),
                    ) {
                        if off {
                            self.ui.set_enabled(node, false);
                        }
                        selects.push((field, node));
                    }
                }
            }
        }
        y += row_h * 6 + GAP * sc;

        // Identify, and Apply or the question after it.
        let Some(s) = self.screens.as_mut() else {
            return y;
        };
        s.nodes.selects = selects;
        let changed = s.changed();
        let left = s.left_s();
        let button_w = 150 * sc;
        let control_h = CONTROL_H * sc;
        if let Some(left) = left {
            // The question on a line of its own, above the answers.
            let countdown_node = self.add_label(
                parent,
                &countdown(left),
                text,
                Role::BaseContent,
                Rect::new(PAD * sc, y, width, 22 * sc),
            );
            y += (22 + GAP) * sc;
            self.ui.add(
                parent,
                Button::new("Identify", Msg::Screen(ScreenMsg::Identify))
                    .with_role(Role::Neutral)
                    .with_style(text),
                Rect::new(PAD * sc, y, button_w, control_h),
            );
            self.ui.add(
                parent,
                Button::new("Go back", Msg::Screen(ScreenMsg::Revert))
                    .with_role(Role::Neutral)
                    .with_style(text),
                Rect::new(
                    PAD * sc + width - 2 * button_w - GAP * sc,
                    y,
                    button_w,
                    control_h,
                ),
            );
            let keep = self.ui.add(
                parent,
                Button::new("Keep", Msg::Screen(ScreenMsg::Keep))
                    .with_role(Role::Primary)
                    .with_style(text),
                Rect::new(PAD * sc + width - button_w, y, button_w, control_h),
            );
            if let Some(s) = self.screens.as_mut() {
                s.nodes.countdown = countdown_node;
            }
            if let Some(keep) = keep {
                self.ui.focus(Some(keep));
            }
        } else {
            self.ui.add(
                parent,
                Button::new("Identify", Msg::Screen(ScreenMsg::Identify))
                    .with_role(Role::Neutral)
                    .with_style(text),
                Rect::new(PAD * sc, y, button_w, control_h),
            );
            let reset = self.ui.add(
                parent,
                Button::new("Undo changes", Msg::Screen(ScreenMsg::Reset))
                    .with_role(Role::Neutral)
                    .with_style(text),
                Rect::new(
                    PAD * sc + width - 2 * button_w - GAP * sc,
                    y,
                    button_w,
                    control_h,
                ),
            );
            let apply = self.ui.add(
                parent,
                Button::new("Apply", Msg::Screen(ScreenMsg::Apply))
                    .with_role(Role::Primary)
                    .with_style(text),
                Rect::new(PAD * sc + width - button_w, y, button_w, control_h),
            );
            for node in [reset, apply].into_iter().flatten() {
                self.ui.set_enabled(node, changed);
            }
        }
        y + control_h + 2 * GAP * sc
    }

    /// Which of the page's controls has focus, for it to come back to after
    /// the page is built again.
    pub(super) fn displays_focus(&self) -> Option<Focus> {
        let s = self.screens.as_ref()?;
        let focused = self.ui.focused()?;
        if s.nodes.arrangement == Some(focused) {
            return Some(Focus::Arrangement);
        }
        s.nodes
            .selects
            .iter()
            .find(|(_, n)| *n == focused)
            .map(|(f, _)| Focus::Choice(*f))
    }

    /// The control `focus` names on the page as built now: the arrangement,
    /// or a choice list.
    pub(super) fn displays_node(&self, focus: Focus) -> Option<NodeId> {
        let s = self.screens.as_ref()?;
        match focus {
            Focus::Arrangement => s.nodes.arrangement,
            Focus::Choice(field) => s
                .nodes
                .selects
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(_, n)| *n),
        }
    }
}

fn countdown(left: u64) -> String {
    match left {
        1 => "Keep these settings? Going back in 1 second.".to_owned(),
        n => format!("Keep these settings? Going back in {n} seconds."),
    }
}

#[cfg(test)]
mod tests {
    use super::{Arrangement, ScreenMsg, Screens};
    use crate::view::{Effect, Fonts, Msg, View};
    use alpymist_about::info::About;
    use alpymist_displays::screen::Monitor;
    use alpymist_settings::Settings;
    use denise::{ElementState, InputEvent, Modifiers, Point, PointerButton, Size};

    fn desk() -> Vec<Monitor> {
        let screen = |name: &str, x: i32| Monitor {
            name: name.into(),
            description: format!("Maker {name}"),
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
            x,
            scale: 1.0,
            available_modes: vec!["1920x1080@60.00Hz".into(), "1280x720@60.00Hz".into()],
            ..Monitor::default()
        };
        vec![
            screen("eDP-1", 0),
            screen("DP-3", 1920),
            screen("DP-5", 3840),
        ]
    }

    fn view() -> View {
        let settings = Settings::new();
        let values = settings
            .all()
            .iter()
            .map(|s| (s.id, Ok(s.default.clone())))
            .collect();
        let mut v = View::new(
            Size::new(920, 900),
            1,
            alpymist_theme::ThemeFile::default().denise(),
            Fonts {
                text: None,
                strong: None,
                icons: None,
            },
            settings,
            values,
            About::sample(),
        );
        v.screens(Screens::new(desk(), None));
        assert!(v.open("displays"));
        v
    }

    fn positions(v: &View) -> Vec<[i32; 2]> {
        let s = v.screens.as_ref().unwrap();
        s.draft.outputs.iter().map(|o| o.position).collect()
    }

    /// The middle of screen `room` in the arrangement, on the window.
    fn centre(v: &View, room: usize) -> Point {
        let node = v.screens.as_ref().unwrap().nodes.arrangement.unwrap();
        let bounds = v.ui.bounds(node).unwrap();
        let a = v.ui.widget::<Arrangement>(node).unwrap();
        let r = a.on_screen(bounds, a.rooms[room]);
        Point::new(r.x + r.width / 2, r.y + r.height / 2)
    }

    fn drag(v: &mut View, from: Point, to: Point) -> Vec<Effect> {
        let button = |state, position| InputEvent::PointerButton {
            button: PointerButton::Left,
            state,
            position,
            modifiers: Modifiers::NONE,
        };
        v.handle(
            &[
                InputEvent::PointerMoved { position: from },
                button(ElementState::Down, from),
                InputEvent::PointerMoved {
                    position: Point::new(from.x.midpoint(to.x), from.y.midpoint(to.y)),
                },
                InputEvent::PointerMoved { position: to },
                button(ElementState::Up, to),
            ],
            100,
        )
    }

    #[test]
    fn a_screen_dragged_left_of_the_laptop_goes_there_and_waits_for_apply() {
        let mut v = view();
        let from = centre(&v, 2);
        let laptop = centre(&v, 0);
        let effects = drag(&mut v, from, Point::new(laptop.x - 200, laptop.y));
        assert!(
            effects.is_empty(),
            "nothing changes before Apply: {effects:?}"
        );
        assert_eq!(positions(&v), [[1920, 0], [3840, 0], [0, 0]]);
        assert_eq!(
            v.screens.as_ref().unwrap().selected,
            2,
            "the dragged one is chosen"
        );

        let mut effects = Vec::new();
        v.displays_message(ScreenMsg::Apply, &mut effects);
        let [
            Effect::Layout {
                layout: applied,
                before: Some(_),
            },
        ] = effects.as_slice()
        else {
            panic!("Apply puts the layout in place: {effects:?}");
        };
        assert_eq!(applied.outputs[2].position, [0, 0]);

        // Nobody answers the question: it goes back on its own.
        let later = v.handle(&[], 100 + super::KEEP_S * 1000 + 1);
        let [
            Effect::Layout {
                layout: back,
                before: None,
            },
        ] = later.as_slice()
        else {
            panic!("the layout before comes back: {later:?}");
        };
        assert_eq!(back.outputs[2].position, [3840, 0]);
        assert_eq!(positions(&v), [[0, 0], [1920, 0], [3840, 0]]);
        assert!(v.screens.as_ref().unwrap().trial.is_none());
    }

    #[test]
    fn a_screen_dragged_a_little_lower_stays_lower() {
        let mut v = view();
        let from = centre(&v, 1);
        let node = v.screens.as_ref().unwrap().nodes.arrangement.unwrap();
        let bounds = v.ui.bounds(node).unwrap();
        let a = v.ui.widget::<Arrangement>(node).unwrap();
        let quarter = a.on_screen(bounds, a.rooms[1]).height / 4;
        drag(&mut v, from, Point::new(from.x, from.y + quarter));
        let [laptop, middle, _] = positions(&v)[..] else {
            panic!("three screens");
        };
        assert_eq!(laptop, [0, 0]);
        assert_eq!(middle[0], 1920, "still beside the laptop");
        assert!(
            (200..=340).contains(&middle[1]),
            "a quarter of its height lower, not snapped back: {middle:?}"
        );
    }

    #[test]
    fn the_laptop_screen_says_when_the_lid_has_it_off() {
        // Remembered with the lid open; off now, for the lid.
        let remembered = alpymist_displays::layout::Layout::extended(&desk());
        let mut monitors = desk();
        monitors[0].disabled = true;
        let s = Screens::new(monitors, Some(&remembered));
        assert!(
            s.label(0).ends_with("off while the lid is closed"),
            "{}",
            s.label(0)
        );
        assert!(!s.label(1).contains("lid"));
    }

    #[test]
    fn a_screen_can_show_the_same_as_another_and_leaves_the_arrangement() {
        let mut v = view();
        let mut effects = Vec::new();
        v.displays_message(ScreenMsg::Pick(2), &mut effects);
        let s = v.screens.as_ref().unwrap();
        assert_eq!(s.mirrors(), [None, Some(0), Some(1)]);
        v.screens.as_mut().unwrap().open = Some(super::Field::Mirror);
        v.displays_message(ScreenMsg::Chose(1), &mut effects);
        let s = v.screens.as_ref().unwrap();
        assert_eq!(s.draft.outputs[2].mirror.as_deref(), Some("Maker eDP-1"));
        assert_eq!(s.rooms().1, [0, 1], "a mirror has no place of its own");
        // Another screen cannot then mirror the mirror.
        v.displays_message(ScreenMsg::Pick(1), &mut effects);
        assert_eq!(v.screens.as_ref().unwrap().mirrors(), [None, Some(0)]);
    }

    #[test]
    fn keep_stops_the_countdown_and_the_last_screen_stays_on() {
        let mut v = view();
        let mut effects = Vec::new();
        v.displays_message(ScreenMsg::Pick(1), &mut effects);
        v.displays_message(ScreenMsg::On(false), &mut effects);
        assert_eq!(positions(&v)[2], [1920, 0], "the screens close the gap");
        v.displays_message(ScreenMsg::Apply, &mut effects);
        v.message(Msg::Screen(ScreenMsg::Keep), &mut effects);
        effects.clear();
        let later = v.handle(&[], super::KEEP_S * 1000 + 5000);
        assert!(later.is_empty(), "kept: {later:?}");

        for i in [0, 2] {
            v.displays_message(ScreenMsg::Pick(i), &mut effects);
            v.displays_message(ScreenMsg::On(false), &mut effects);
        }
        let s = v.screens.as_ref().unwrap();
        assert_eq!(
            s.draft.outputs.iter().filter(|o| o.enabled).count(),
            1,
            "one screen stays on"
        );
    }
}

//! The Settings window's contents, drawn with Denise's widgets.
//!
//! Nothing here knows about Wayland: input arrives as Denise events, and what
//! the window has to do comes back as [`Effect`]s. So the same view is painted
//! into a PNG by the snapshot example, on any machine.
//!
//! Down the side, a search field and the areas; beside them, the chosen area's
//! settings, or every setting that matches the search. Typing anywhere
//! searches; arrows move through the areas; Tab moves into the page; Escape
//! clears a search, then closes.
//!
//! A screensaver's own settings are the one thing not down the side. They live
//! behind a button on the Screensaver page, beside the list that chooses
//! between screensavers, and open in a dialog over it — so installing five
//! screensavers adds five entries to that list and none to the side.

use alpymist_about::info::About;
use alpymist_settings::{Area, Kind, Setting, Settings, Value};
use denise::theme::{Radius, Role, Theme};
use denise::{ElementState, Frame, InputEvent, KeyCode, Modifiers, Point, Rect, Size};
use denise_text::{FontId, GlyphSource, TextStyle};
use denise_ui::widgets::{
    Align, Button, Label, List, ListItem, Panel, Select, Slider, TextInput, Toggle, open_select,
};
use denise_ui::{NodeId, Ui};
use std::collections::BTreeMap;

/// Sizes in logical pixels, scaled when laid out.
const SIDEBAR: i32 = 260;
const PAD: i32 = 20;
const SEARCH_H: i32 = 38;
const LIST_ROW: i32 = 38;
const HEADER_H: i32 = 96;
const ROW_H: i32 = 72;
const LINE_H: i32 = 18;
const GAP: i32 = 10;
const TOGGLE_W: i32 = 52;
const TOGGLE_H: i32 = 28;
const SLIDER_W: i32 = 200;
const VALUE_W: i32 = 124;
const SELECT_W: i32 = 240;
const CONTROL_H: i32 = 36;
const BUTTON_W: i32 = 180;
/// The screensaver dialog's width, before it is clamped to the window.
///
/// Wide enough that a setting's own sentence still wraps to two lines beside
/// its slider, and narrow enough to fit the smallest window Settings opens in.
const DIALOG_W: i32 = 620;
/// Its header: the name of what is being changed, and a line about it.
const DIALOG_HEAD: i32 = 74;
/// Its footer: the button that closes it.
const DIALOG_FOOT: i32 = CONTROL_H + 2 * GAP;
/// How dark the page goes behind it. A conventional modal veil.
const DIALOG_DIM: u8 = 128;
/// The page the screensaver dialog opens over, and the area its list lives in.
const SCREENSAVER: &str = "screensaver";
/// The setting that says which screensaver is shown.
const SHOW: &str = "screensaver.show";

/// A message from a widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// An area was chosen in the list.
    Area(usize),
    /// A toggle or slider moved; the rows are read to find which.
    Changed,
    /// A choice list was asked to open; focus or the pointer says which.
    OpenSelect,
    /// An entry in the open choice list was chosen.
    Chose(usize),
    /// A setting that is a thing to do rather than a thing to be: its button
    /// was pressed, and the row it belongs to says which.
    Do(usize),
    /// A button beyond settings.
    Action(Action),
    /// The Screensaver page's button: open the chosen screensaver's own
    /// settings in a dialog over it.
    Configure,
    /// The dialog's button: take it away.
    Done,
    /// Enter in the search field.
    Submit,
}

/// Something only the window can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open the Wi-Fi popup.
    OpenWifi,
    /// Open the battery popup.
    OpenPower,
    /// Open the store at its updates.
    CheckUpdates,
    /// Put the About details on the clipboard.
    CopyAbout,
}

/// What the window should do after input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Change a setting.
    Set {
        /// Which.
        id: &'static str,
        /// To what.
        value: Value,
    },
    /// Do an [`Action`].
    Action(Action),
    /// Close the window.
    Close,
}

/// The faces text is drawn in.
pub struct Fonts {
    /// Body text.
    pub text: Option<Box<dyn GlyphSource>>,
    /// Titles.
    pub strong: Option<Box<dyn GlyphSource>>,
    /// Nerd Font icons.
    pub icons: Option<Box<dyn GlyphSource>>,
}

/// What is on the page beside the areas.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    /// An area's settings, by index.
    Area(usize),
    /// About this computer.
    About,
    /// Every setting matching the search.
    Search(String),
}

/// A control on the page and the setting it shows.
struct Row {
    setting: usize,
    control: NodeId,
    value_label: Option<NodeId>,
    shown: Option<Value>,
}

/// The screensaver settings dialog, while it is up.
struct Dialog {
    /// The screensaver areas it shows, in order.
    ///
    /// One when a screensaver is chosen. All of them when the choice is a
    /// different one each time: every screensaver installed is in that
    /// rotation, so every screensaver's settings are what that choice means.
    areas: Vec<&'static str>,
    /// Its sheet, so a press beside it can take it away.
    sheet: NodeId,
    /// Where its rows start in [`View::rows`]; the page's come before them.
    first: usize,
}

/// The window's contents.
pub struct View {
    ui: Ui<Msg>,
    settings: Settings,
    values: BTreeMap<&'static str, Result<Value, String>>,
    about: About,
    scale: i32,
    size: Size,
    text: FontId,
    strong: FontId,
    icons: FontId,
    backdrop: NodeId,
    sidebar: NodeId,
    search: NodeId,
    areas: NodeId,
    content: Option<NodeId>,
    page: Page,
    /// The area or About page a search returns to.
    last: Page,
    rows: Vec<Row>,
    open_row: Option<usize>,
    /// The screensaver settings dialog, when one is open.
    dialog: Option<Dialog>,
    /// The button that opens it, so focus comes back to it when it closes.
    configure: Option<NodeId>,
    pointer: Point,
    query: String,
}

impl View {
    /// A view of `settings`, their current `values` and `about`, `size`
    /// physical pixels at `scale`, in `theme`.
    #[must_use]
    pub fn new(
        size: Size,
        scale: u32,
        theme: Theme,
        fonts: Fonts,
        settings: Settings,
        values: BTreeMap<&'static str, Result<Value, String>>,
        about: About,
    ) -> Self {
        let s = i32::try_from(scale.max(1)).unwrap_or(1);
        #[allow(clippy::cast_precision_loss)]
        let mut ui = Ui::new(size, theme.scaled(s as f32));
        ui.show_cursor(false);
        let default = FontId::DEFAULT;
        let text = fonts.text.map_or(default, |f| ui.add_font(f));
        ui.set_default_font(text);
        let strong = fonts.strong.map_or(text, |f| ui.add_font(f));
        let icons = fonts.icons.map_or(text, |f| ui.add_font(f));
        let root = ui.root();
        let backdrop = ui
            .add(
                root,
                Panel::filled(Role::Base100).with_radius(Radius::Selector),
                Rect::from_size(size),
            )
            .unwrap_or(root);
        let sidebar = ui
            .add(
                root,
                Panel::filled(Role::Base200).with_radius(Radius::Selector),
                Rect::ZERO,
            )
            .unwrap_or(root);
        let search = ui
            .add(
                sidebar,
                TextInput::new()
                    .with_placeholder("Search settings")
                    .with_submit(Msg::Submit)
                    .with_style(TextStyle {
                        font: text,
                        size_px: px(15, s),
                    }),
                Rect::ZERO,
            )
            .unwrap_or(root);
        let mut items: Vec<ListItem> = settings
            .pages()
            .iter()
            .map(|a| ListItem::new(a.title))
            .collect();
        items.push(ListItem::new("About"));
        let areas = ui
            .add(
                sidebar,
                List::new(items, Msg::Area)
                    .on_activate(Msg::Area)
                    .activate_on_click()
                    .with_selected(Some(0))
                    .with_row_height(LIST_ROW * s)
                    .with_style(TextStyle {
                        font: text,
                        size_px: px(15, s),
                    }),
                Rect::ZERO,
            )
            .unwrap_or(root);
        let mut view = Self {
            ui,
            settings,
            values,
            about,
            scale: s,
            size,
            text,
            strong,
            icons,
            backdrop,
            sidebar,
            search,
            areas,
            content: None,
            page: Page::Area(0),
            last: Page::Area(0),
            rows: Vec::new(),
            open_row: None,
            dialog: None,
            configure: None,
            pointer: Point::new(0, 0),
            query: String::new(),
        };
        view.place_sidebar();
        view.build();
        view.ui.focus(Some(areas));
        view
    }

    fn place_sidebar(&mut self) {
        let sidebar = self.sidebar;
        let s = self.scale;
        let h = i32::try_from(self.size.height).unwrap_or(0);
        self.ui
            .set_layout(self.backdrop, Rect::from_size(self.size));
        self.ui.set_layout(sidebar, Rect::new(0, 0, SIDEBAR * s, h));
        self.ui.set_layout(
            self.search,
            Rect::new(PAD * s / 2, PAD * s / 2, (SIDEBAR - PAD) * s, SEARCH_H * s),
        );
        let top = (PAD + SEARCH_H) * s;
        self.ui.set_layout(
            self.areas,
            Rect::new(PAD * s / 4, top, (SIDEBAR - PAD / 2) * s, h - top - PAD * s),
        );
    }

    /// What the view is doing, for `ALPYMIST_SETTINGS_TRACE`.
    #[must_use]
    pub fn trace(&self) -> String {
        let focused = self.ui.focused();
        let what = if focused == Some(self.search) {
            "search".to_owned()
        } else if focused == Some(self.areas) {
            "areas".to_owned()
        } else if let Some(r) = self.rows.iter().find(|r| Some(r.control) == focused) {
            self.settings.all()[r.setting].id.to_owned()
        } else {
            format!("{focused:?}")
        };
        format!(
            "page {:?}, focus {what}, rows {}",
            self.page,
            self.rows.len()
        )
    }

    /// Whether there is anything new to paint.
    #[must_use]
    pub fn needs_paint(&self) -> bool {
        self.ui.needs_paint()
    }

    /// When the tree next wants to be woken for an animation, in ms.
    #[must_use]
    pub fn next_wake_ms(&self) -> Option<u64> {
        self.ui.next_wake_ms()
    }

    /// Paint everything into `frame`, which may be a fresh buffer.
    pub fn paint(&mut self, frame: &mut Frame<'_>) {
        self.ui.invalidate_all();
        self.ui.paint(frame);
        self.ui.presented();
    }

    /// Show `id`, an area or a setting, and focus it. Returns whether it
    /// exists.
    ///
    /// A screensaver's own area — `screensaver-mountains`, or one of its
    /// settings — has no page of its own: it opens the Screensaver page with
    /// that screensaver's dialog over it, which is the one place those
    /// settings are changed. So `alpymist-settings screensaver-mountains.block`
    /// still lands on the control it names.
    pub fn open(&mut self, id: &str) -> bool {
        if id == "about" {
            self.show(Page::About);
            return true;
        }
        let area = id.split_once('.').map_or(id, |(a, _)| a);
        let screensaver = self
            .settings
            .screensavers()
            .iter()
            .find(|a| a.id == area)
            .map(|a| a.id);
        let Some(index) = self
            .settings
            .pages()
            .iter()
            .position(|a| a.id == area || (screensaver.is_some() && a.id == SCREENSAVER))
        else {
            return false;
        };
        self.clear_search();
        self.show(Page::Area(index));
        if let Some(area) = screensaver {
            self.open_dialog(vec![area]);
        }
        if let Some(row) = self
            .rows
            .iter()
            .find(|r| self.settings.all()[r.setting].id == id)
        {
            let control = row.control;
            self.ui.focus(Some(control));
        }
        true
    }

    /// A setting's value changed, or could not be changed: show what it is.
    pub fn set_value(&mut self, id: &'static str, value: Result<Value, String>) {
        self.values.insert(id, value.clone());
        let Some(index) = self
            .rows
            .iter()
            .position(|r| self.settings.all()[r.setting].id == id)
        else {
            return;
        };
        if let Ok(v) = value {
            self.show_value(index, &v);
        }
    }

    /// Say something in a toast.
    pub fn toast(&mut self, text: &str, error: bool) {
        self.ui
            .toast(text, if error { Role::Error } else { Role::Info });
    }

    /// New size or scale: lay everything out again.
    pub fn resize(&mut self, size: Size, scale: u32) {
        let s = i32::try_from(scale.max(1)).unwrap_or(1);
        // Compositors configure again on focus and other state changes.
        if size == self.size && s == self.scale {
            return;
        }
        self.size = size;
        self.scale = s;
        self.ui.handle(&[InputEvent::SurfaceResized {
            size,
            #[allow(clippy::cast_precision_loss)]
            scale_factor: s as f32,
        }]);
        self.place_sidebar();
        self.build();
    }

    /// Handle input, `now_ms` since some start, and say what to do.
    pub fn handle(&mut self, events: &[InputEvent], now_ms: u64) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut pass = Vec::with_capacity(events.len());
        for event in events {
            match *event {
                // A modal takes every press, so one landing beside its sheet
                // has nowhere to go; letting it dismiss the dialog is what
                // keeps the window from feeling stuck. A choice list open over
                // the dialog answers its own presses first.
                InputEvent::PointerButton {
                    position,
                    state: ElementState::Down,
                    ..
                } if self.dialog.is_some()
                    && !self.ui.popup_open()
                    && !self.over_dialog(position) =>
                {
                    self.pointer = position;
                    self.close_dialog();
                }
                InputEvent::PointerMoved { position }
                | InputEvent::PointerButton { position, .. } => {
                    self.pointer = position;
                    pass.push(event.clone());
                }
                InputEvent::Key {
                    code,
                    state: ElementState::Down,
                    modifiers,
                    ..
                } if !self.ui.popup_open() => {
                    if let Some(effect) = self.key(code, modifiers) {
                        effects.push(effect);
                        continue;
                    }
                    if self.consumed(code, modifiers) {
                        continue;
                    }
                    pass.push(event.clone());
                }
                InputEvent::Text { ch }
                    if self.ui.focused() != Some(self.search)
                        && !self.ui.popup_open()
                        && self.dialog.is_none() =>
                {
                    // Typing anywhere searches; a space is a toggle's.
                    if !ch.is_whitespace() {
                        self.ui.focus(Some(self.search));
                        pass.push(event.clone());
                    }
                }
                _ => pass.push(event.clone()),
            }
        }
        self.ui.handle(&pass);
        self.ui.tick(now_ms);
        let messages: Vec<Msg> = self.ui.drain_messages().collect();
        for m in messages {
            self.message(m, &mut effects);
        }
        self.poll(&mut effects);
        self.search_changed();
        effects
    }

    /// Keys the view answers itself, before the widgets see them.
    fn key(&mut self, code: KeyCode, modifiers: Modifiers) -> Option<Effect> {
        let ctrl = modifiers.contains(Modifiers::CTRL);
        match code {
            KeyCode::Escape if self.dialog.is_some() => {
                self.close_dialog();
                None
            }
            KeyCode::Escape if !self.query.is_empty() => {
                self.clear_search();
                self.show(self.last.clone());
                self.ui.focus(Some(self.areas));
                None
            }
            KeyCode::Escape => Some(Effect::Close),
            KeyCode::Q | KeyCode::W if ctrl => Some(Effect::Close),
            KeyCode::C
                if ctrl && self.page == Page::About && self.ui.focused() != Some(self.search) =>
            {
                Some(Effect::Action(Action::CopyAbout))
            }
            _ => None,
        }
    }

    /// Keys that move focus between the search, the areas and the page.
    fn consumed(&mut self, code: KeyCode, modifiers: Modifiers) -> bool {
        let ctrl = modifiers.contains(Modifiers::CTRL);
        let focused = self.ui.focused();
        match code {
            KeyCode::F if ctrl && self.dialog.is_none() => {
                self.ui.focus(Some(self.search));
                true
            }
            KeyCode::ArrowDown if focused == Some(self.search) => {
                self.focus_page();
                true
            }
            KeyCode::ArrowRight if focused == Some(self.areas) => {
                self.focus_page();
                true
            }
            _ => false,
        }
    }

    fn focus_page(&mut self) {
        let first = self.rows.first().map(|r| r.control);
        self.ui.focus(first.or(Some(self.areas)));
    }

    #[allow(clippy::needless_pass_by_value)] // drained messages, used once
    fn message(&mut self, m: Msg, effects: &mut Vec<Effect>) {
        match m {
            Msg::Area(i) => {
                self.clear_search();
                if i >= self.settings.pages().len() {
                    self.show(Page::About);
                } else {
                    self.show(Page::Area(i));
                }
            }
            Msg::Configure => {
                let areas = self.chosen_screensavers();
                self.open_dialog(areas);
            }
            Msg::Done => self.close_dialog(),
            Msg::Submit => self.focus_page(),
            Msg::Do(index) => {
                if let Some(setting) = self.settings.all().get(index) {
                    // Nothing to read back off a button: what it is worth is
                    // that it was pressed.
                    effects.push(Effect::Set {
                        id: setting.id,
                        value: Value::Text(String::new()),
                    });
                }
            }
            Msg::Action(a) => effects.push(Effect::Action(a)),
            Msg::OpenSelect => {
                let focused = self.ui.focused();
                let hit = self.rows.iter().position(|r| {
                    Some(r.control) == focused
                        || self
                            .ui
                            .bounds(r.control)
                            .is_some_and(|b| b.contains(self.pointer))
                });
                if let Some(row) = hit {
                    self.open_row = Some(row);
                    open_select(&mut self.ui, self.rows[row].control, Msg::Chose);
                }
            }
            Msg::Chose(choice) => {
                self.ui.close_popup();
                let Some(row) = self.open_row.take() else {
                    return;
                };
                let control = self.rows[row].control;
                let setting = &self.settings.all()[self.rows[row].setting];
                if let Kind::Choice(choices) = &setting.kind
                    && let Some(c) = choices.get(choice)
                {
                    let value = Value::Text(c.value.clone());
                    if let Some(select) = self.ui.widget_mut::<Select<Msg>>(control) {
                        select.set_selected(Some(choice));
                    }
                    self.ui.focus(Some(control));
                    if self.rows[row].shown.as_ref() != Some(&value) {
                        self.rows[row].shown = Some(value.clone());
                        effects.push(Effect::Set {
                            id: setting.id,
                            value,
                        });
                    }
                }
            }
            Msg::Changed => {}
        }
    }

    /// Read the toggles and sliders, and change what moved.
    fn poll(&mut self, effects: &mut Vec<Effect>) {
        for i in 0..self.rows.len() {
            let setting = &self.settings.all()[self.rows[i].setting];
            let control = self.rows[i].control;
            let now = match setting.kind {
                Kind::Switch => self
                    .ui
                    .widget::<Toggle<Msg>>(control)
                    .map(|t| Value::Bool(t.checked())),
                Kind::Number { .. } => self.ui.widget::<Slider<Msg>>(control).and_then(|sl| {
                    #[allow(clippy::cast_possible_truncation)]
                    let n = Value::Number(sl.value().round() as i64);
                    (!sl.dragging()).then_some(n)
                }),
                // A button holds nothing to read back, and a choice answers
                // through its own list rather than here.
                Kind::Action { .. } | Kind::Choice(_) => None,
            };
            // While a slider is dragged, only its number follows.
            if let Kind::Number { .. } = setting.kind
                && let Some(sl) = self.ui.widget::<Slider<Msg>>(control)
                && let Some(label) = self.rows[i].value_label
            {
                #[allow(clippy::cast_possible_truncation)]
                let text = setting.describe(&Value::Number(sl.value().round() as i64));
                if let Some(l) = self.ui.widget_mut::<Label>(label)
                    && l.text() != text
                {
                    l.set_text(text);
                }
            }
            if let Some(now) = now
                && self.rows[i].shown.as_ref() != Some(&now)
            {
                let id = setting.id;
                self.rows[i].shown = Some(now.clone());
                effects.push(Effect::Set { id, value: now });
            }
        }
    }

    fn search_changed(&mut self) {
        let text = self
            .ui
            .widget::<TextInput<Msg>>(self.search)
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        if text == self.query {
            return;
        }
        self.query.clone_from(&text);
        if text.trim().is_empty() {
            self.show(self.last.clone());
        } else {
            self.show(Page::Search(text));
        }
    }

    fn clear_search(&mut self) {
        if let Some(t) = self.ui.widget_mut::<TextInput<Msg>>(self.search) {
            t.clear();
        }
        self.query.clear();
    }

    fn show(&mut self, page: Page) {
        if let Page::Area(i) = page
            && let Some(list) = self.ui.widget_mut::<List<Msg>>(self.areas)
        {
            list.set_selected(Some(i));
        }
        if page == Page::About
            && let Some(list) = self.ui.widget_mut::<List<Msg>>(self.areas)
        {
            list.set_selected(Some(self.settings.pages().len()));
        }
        if matches!(page, Page::Search(_))
            && let Some(list) = self.ui.widget_mut::<List<Msg>>(self.areas)
        {
            list.set_selected(None);
        }
        if !matches!(page, Page::Search(_)) {
            self.last = page.clone();
        }
        self.page = page;
        self.build();
    }

    fn style(&self, font: FontId, size: i32) -> TextStyle {
        TextStyle {
            font,
            size_px: px(size, self.scale),
        }
    }

    /// Build the page beside the areas again.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)] // one page, top to bottom
    fn build(&mut self) {
        let focused_search = self.ui.focused() == Some(self.search);
        let focused_setting = self
            .rows
            .iter()
            .find(|r| Some(r.control) == self.ui.focused())
            .map(|r| r.setting);
        // The dialog is a scene over this one, so rebuilding the page beneath
        // it means taking it down and putting it back: its rows live in the
        // same list as the page's, and that list is about to be emptied.
        let reopen = self.dialog.as_ref().map(|d| d.areas.clone());
        self.close_dialog();
        if let Some(old) = self.content.take() {
            self.ui.remove(old);
        }
        self.rows.clear();
        self.open_row = None;
        self.configure = None;
        let s = self.scale;
        let root = self.ui.root();
        let w = i32::try_from(self.size.width).unwrap_or(0) - SIDEBAR * s;
        let h = i32::try_from(self.size.height).unwrap_or(0);
        let Some(content) = self
            .ui
            .add(root, Panel::bare(), Rect::new(SIDEBAR * s, 0, w, h))
        else {
            return;
        };
        self.content = Some(content);
        self.ui.set_scrollable(content, true);
        let inner = w - 2 * PAD * s;

        let (icon, title, description) = match &self.page {
            Page::Area(i) => {
                let a = &self.settings.pages()[*i];
                (
                    a.icon.to_owned(),
                    a.title.to_owned(),
                    a.description.to_owned(),
                )
            }
            Page::About => (
                "\u{f02fd}".to_owned(),
                self.about.title(),
                self.about.subtitle(),
            ),
            Page::Search(q) => (
                "\u{f0349}".to_owned(),
                format!("Settings matching “{}”", q.trim()),
                "Tab moves to the first; Escape clears the search.".to_owned(),
            ),
        };
        let icon_style = self.style(self.icons, 34);
        let title_style = self.style(self.strong, 24);
        let dim_style = self.style(self.text, 14);
        self.add_label(
            content,
            &icon,
            icon_style,
            Role::Primary,
            Rect::new(PAD * s, PAD * s, 48 * s, 48 * s),
        );
        self.add_label(
            content,
            &title,
            title_style,
            Role::BaseContent,
            Rect::new((PAD + 60) * s, PAD * s, inner - 60 * s, 32 * s),
        );
        self.add_label(
            content,
            &description,
            dim_style,
            Role::Secondary,
            Rect::new((PAD + 60) * s, (PAD + 34) * s, inner - 60 * s, 22 * s),
        );
        let mut y = HEADER_H * s;

        match self.page.clone() {
            Page::About => y = self.build_about(content, y, inner),
            Page::Area(i) => {
                let area = self.settings.pages()[i].id;
                let chosen: Vec<usize> = (0..self.settings.all().len())
                    .filter(|&j| self.settings.all()[j].area() == area)
                    .collect();
                for j in chosen {
                    y = self.build_row(content, j, y, inner, false);
                }
                let extra = match area {
                    "wifi" => Some(("Networks…".to_owned(), Msg::Action(Action::OpenWifi))),
                    "power" => Some(("Battery…".to_owned(), Msg::Action(Action::OpenPower))),
                    "updates" => Some((
                        "Check for updates".to_owned(),
                        Msg::Action(Action::CheckUpdates),
                    )),
                    // The one door to a screensaver's own settings. Nothing is
                    // behind it when nothing is installed, and the button is
                    // then not there to be pressed.
                    SCREENSAVER if !self.settings.screensavers().is_empty() => {
                        Some((configure_label(self.chosen_screensaver()), Msg::Configure))
                    }
                    _ => None,
                };
                let configure = matches!(extra, Some((_, Msg::Configure)));
                if let Some((label, message)) = extra {
                    let width = if configure { BUTTON_W + 60 } else { BUTTON_W };
                    let button = self.ui.add(
                        content,
                        Button::new(label, message)
                            .with_role(Role::Neutral)
                            .with_style(self.style(self.text, 15)),
                        Rect::new(PAD * s, y + GAP * s, width * s, CONTROL_H * s),
                    );
                    if configure {
                        self.configure = button;
                    }
                    y += (GAP + CONTROL_H) * s;
                }
            }
            Page::Search(q) => {
                let matching: Vec<usize> = (0..self.settings.all().len())
                    .filter(|&j| {
                        let st = &self.settings.all()[j];
                        let area = self.settings.area(st.area()).map_or("", |a| a.title);
                        let keywords = st.keywords.join(" ");
                        alpymist_core::catalog::matches(
                            &q,
                            &[st.title, st.description, st.id, area, &keywords],
                        )
                    })
                    .collect();
                if matching.is_empty() {
                    self.add_label(
                        content,
                        "Nothing matches. Try another word, such as scroll, layout or battery.",
                        self.style(self.text, 15),
                        Role::Secondary,
                        Rect::new(PAD * s, y, inner, 24 * s),
                    );
                }
                for j in matching {
                    y = self.build_row(content, j, y, inner, true);
                }
            }
        }
        // Room at the bottom, so the last row scrolls clear of the edge.
        self.ui
            .add(content, Panel::bare(), Rect::new(0, y, 1, PAD * s));
        if let Some(areas) = reopen {
            self.build_dialog(areas);
        }
        if focused_search {
            self.ui.focus(Some(self.search));
        } else if let Some(setting) = focused_setting
            && let Some(row) = self.rows.iter().find(|r| r.setting == setting)
        {
            let control = row.control;
            self.ui.focus(Some(control));
        }
    }

    /// The screensaver areas the Screensaver page's button leads to.
    ///
    /// The one chosen, or — when the choice is a different one each time —
    /// every screensaver installed, since that choice is all of them.
    fn chosen_screensavers(&self) -> Vec<&'static str> {
        match self.chosen_screensaver() {
            Some(area) => vec![area.id],
            None => self.settings.screensavers().iter().map(|a| a.id).collect(),
        }
    }

    /// The area of the screensaver now chosen, if one in particular is.
    fn chosen_screensaver(&self) -> Option<&'static Area> {
        let show = self
            .values
            .get(SHOW)
            .and_then(|v| v.as_ref().ok())
            .and_then(Value::as_text)?;
        self.settings.screensaver(show)
    }

    /// Whether `at` is on the dialog's sheet rather than beside it.
    fn over_dialog(&self, at: Point) -> bool {
        self.dialog
            .as_ref()
            .and_then(|d| self.ui.bounds(d.sheet))
            .is_some_and(|b| b.contains(at))
    }

    /// Put the dialog up over the page, showing `areas`, and focus into it.
    fn open_dialog(&mut self, areas: Vec<&'static str>) {
        if areas.is_empty() {
            return;
        }
        self.close_dialog();
        self.build_dialog(areas);
        let first = self
            .dialog
            .as_ref()
            .and_then(|d| self.rows.get(d.first))
            .map(|r| r.control);
        if let Some(control) = first {
            self.ui.focus(Some(control));
        }
    }

    /// Take the dialog away, and put focus back where it came from.
    fn close_dialog(&mut self) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        // A choice list opened over it is part of it, and goes with it.
        while self.ui.popup_open() {
            self.ui.close_popup();
        }
        self.ui.pop_scene();
        self.rows.truncate(dialog.first);
        self.open_row = None;
        let back = self.configure;
        self.ui.focus(back.or(Some(self.areas)));
    }

    /// The one area a list of them names, when it names exactly one that is
    /// installed.
    fn only(&self, areas: &[&'static str]) -> Option<&'static Area> {
        match areas {
            [one] => self.settings.area(one),
            _ => None,
        }
    }

    /// Build the dialog: a sheet over a dimmed page, holding `areas`' settings.
    ///
    /// Laid out and then measured, rather than measured and then laid out: how
    /// tall a row is depends on how many lines its description wraps to, which
    /// is not known until it has been built. So the sheet goes up at a
    /// provisional size, the rows are put in it, and the size that came out of
    /// that is what it is finally given — children keep their places, since
    /// they are positioned relative to it.
    #[allow(clippy::too_many_lines)] // one dialog, top to bottom
    fn build_dialog(&mut self, areas: Vec<&'static str>) {
        let s = self.scale;
        let w = i32::try_from(self.size.width).unwrap_or(0);
        let h = i32::try_from(self.size.height).unwrap_or(0);
        let dw = (DIALOG_W * s).min(w - 2 * PAD * s).max(4 * PAD * s);
        let inner = dw - 2 * PAD * s;
        let head = DIALOG_HEAD * s;
        let foot = DIALOG_FOOT * s;
        let root = self.ui.push_scene(DIALOG_DIM);
        let sheet = self.ui.add(
            root,
            Panel::filled(Role::Base100)
                .with_radius(Radius::Box)
                .backdrop(),
            Rect::new((w - dw) / 2, PAD * s, dw, h - 2 * PAD * s),
        );
        let Some(sheet) = sheet else {
            self.ui.pop_scene();
            return;
        };

        let (title, description) = dialog_header(self.only(&areas));
        let title_style = self.style(self.strong, 19);
        let dim_style = self.style(self.text, 13);
        let heading_style = self.style(self.strong, 15);
        self.add_label(
            sheet,
            &title,
            title_style,
            Role::BaseContent,
            Rect::new(PAD * s, PAD * s, inner, 26 * s),
        );
        self.add_label(
            sheet,
            &description,
            dim_style,
            Role::Secondary,
            Rect::new(PAD * s, (PAD + 28) * s, inner, 20 * s),
        );

        let Some(body) = self.ui.add(sheet, Panel::bare(), Rect::new(0, head, dw, h)) else {
            self.ui.pop_scene();
            return;
        };
        self.ui.set_scrollable(body, true);
        let first = self.rows.len();
        let named = areas.len() > 1;
        let mut y = 0;
        for area in &areas {
            if named {
                let name = self.settings.area(area).map_or(*area, |a| a.title);
                self.add_label(
                    body,
                    name,
                    heading_style,
                    Role::BaseContent,
                    Rect::new(PAD * s, y + GAP * s, inner, 22 * s),
                );
                y += (GAP + 26) * s;
            }
            let chosen: Vec<usize> = (0..self.settings.all().len())
                .filter(|&j| self.settings.all()[j].area() == *area)
                .collect();
            for j in chosen {
                y = self.build_row(body, j, y, inner, false);
            }
        }

        let dh = (head + y + foot).min(h - 2 * PAD * s).max(head + foot);
        self.ui
            .set_layout(sheet, Rect::new((w - dw) / 2, (h - dh) / 2, dw, dh));
        self.ui
            .set_layout(body, Rect::new(0, head, dw, dh - head - foot));
        let done = self.style(self.text, 15);
        self.ui.add(
            sheet,
            Button::new("Done", Msg::Done).with_style(done),
            Rect::new(
                dw - (PAD + 120) * s,
                dh - foot + GAP * s,
                120 * s,
                CONTROL_H * s,
            ),
        );
        self.dialog = Some(Dialog {
            areas,
            sheet,
            first,
        });
    }

    fn add_label(
        &mut self,
        parent: NodeId,
        text: &str,
        style: TextStyle,
        role: Role,
        r: Rect,
    ) -> Option<NodeId> {
        self.ui.add(
            parent,
            Label::new(text).with_style(style).with_role(role),
            r,
        )
    }

    /// A setting's card: its title, what it does, and its control. Returns
    /// where the next card goes.
    #[allow(clippy::too_many_lines)] // a card, left to right
    fn build_row(
        &mut self,
        parent: NodeId,
        index: usize,
        y: i32,
        width: i32,
        show_area: bool,
    ) -> i32 {
        let s = self.scale;
        let setting: Setting = self.settings.all()[index].clone();
        let value = self
            .values
            .get(setting.id)
            .cloned()
            .unwrap_or_else(|| Ok(setting.default.clone()));
        let control_w = match setting.kind {
            Kind::Switch => TOGGLE_W,
            Kind::Number { .. } => SLIDER_W + VALUE_W,
            Kind::Choice(_) => SELECT_W,
            Kind::Action { .. } => BUTTON_W,
        } * s;
        let text_w = width - control_w - 3 * PAD * s;

        let mut description = setting.description.to_owned();
        if show_area && let Some(a) = self.settings.area(setting.area()) {
            description = format!("{} · {description}", a.title);
        }
        if let Err(e) = &value {
            description = format!("Unavailable: {e}");
        }
        if setting.scope == alpymist_settings::Scope::System && value.is_ok() {
            description.push_str(" Asks for an administrator's password.");
        }
        let dim = self.style(self.text, 13);
        let lines = wrap(&mut self.ui, dim, &description, text_w);
        let lines_h = i32::try_from(lines.len()).unwrap_or(1) * LINE_H * s;
        let card_h = (ROW_H * s).max(40 * s + lines_h);
        let Some(card) = self.ui.add(
            parent,
            Panel::default(),
            Rect::new(PAD * s, y, width, card_h),
        ) else {
            return y;
        };
        let title = self.style(self.strong, 15);
        self.add_label(
            card,
            setting.title,
            title,
            Role::BaseContent,
            Rect::new(PAD * s, 12 * s, text_w, 22 * s),
        );
        let role = if value.is_err() {
            Role::Warning
        } else {
            Role::Secondary
        };
        for (n, line) in lines.iter().enumerate() {
            let top = (36 + LINE_H * i32::try_from(n).unwrap_or(0)) * s;
            self.add_label(
                card,
                line,
                dim,
                role,
                Rect::new(PAD * s, top, text_w, LINE_H * s),
            );
        }

        let right = width - PAD * s;
        let middle = card_h / 2;
        let shown = value.as_ref().ok().cloned();
        let (control, value_label) = match &setting.kind {
            Kind::Switch => {
                let on = shown.as_ref().and_then(Value::as_bool).unwrap_or(false);
                let t = Toggle::new("", |_| Msg::Changed).with_checked(on);
                let r = Rect::new(
                    right - TOGGLE_W * s,
                    middle - TOGGLE_H * s / 2,
                    TOGGLE_W * s,
                    TOGGLE_H * s,
                );
                (self.ui.add(card, t, r), None)
            }
            Kind::Number { min, max, step, .. } => {
                let n = shown.as_ref().and_then(Value::as_number).unwrap_or(*min);
                #[allow(clippy::cast_precision_loss)]
                let slider = Slider::new(*min as f32, *max as f32, n as f32, |_| Msg::Changed)
                    .with_step(*step as f32);
                let r = Rect::new(
                    right - (SLIDER_W + VALUE_W) * s,
                    middle - 14 * s,
                    SLIDER_W * s,
                    28 * s,
                );
                let control = self.ui.add(card, slider, r);
                let label = self.ui.add(
                    card,
                    Label::new(setting.describe(&Value::Number(n)))
                        .with_style(self.style(self.text, 15))
                        .with_align(Align::End, Align::Center),
                    Rect::new(right - VALUE_W * s, middle - 12 * s, VALUE_W * s, 24 * s),
                );
                (control, label)
            }
            Kind::Action { label } => {
                let button = Button::new(*label, Msg::Do(index))
                    .with_role(Role::Neutral)
                    .with_style(self.style(self.text, 15));
                let r = Rect::new(
                    right - BUTTON_W * s,
                    middle - CONTROL_H * s / 2,
                    BUTTON_W * s,
                    CONTROL_H * s,
                );
                (self.ui.add(card, button, r), None)
            }
            Kind::Choice(choices) => {
                let selected = shown
                    .as_ref()
                    .and_then(Value::as_text)
                    .and_then(|t| choices.iter().position(|c| c.value == t));
                let select = Select::new(choices.iter().map(|c| c.label.clone()), Msg::OpenSelect)
                    .with_selected(selected)
                    .with_style(self.style(self.text, 15));
                let r = Rect::new(
                    right - SELECT_W * s,
                    middle - CONTROL_H * s / 2,
                    SELECT_W * s,
                    CONTROL_H * s,
                );
                (self.ui.add(card, select, r), None)
            }
        };
        if let Some(control) = control {
            if value.is_err() {
                self.ui.set_enabled(control, false);
            }
            self.rows.push(Row {
                setting: index,
                control,
                value_label,
                shown,
            });
        }
        y + card_h + GAP * s
    }

    fn build_about(&mut self, parent: NodeId, mut y: i32, width: i32) -> i32 {
        let s = self.scale;
        let rows = self.about.rows();
        let packages: Vec<(String, String)> = self
            .about
            .shown_packages()
            .into_iter()
            .map(|p| (p.name.clone(), p.version.clone()))
            .collect();
        let line = 26 * s;
        let text = self.style(self.text, 15);
        let small = self.style(self.text, 13);

        let card_h = PAD * s + line * i32::try_from(rows.len()).unwrap_or(0);
        if let Some(card) = self.ui.add(
            parent,
            Panel::default(),
            Rect::new(PAD * s, y, width, card_h),
        ) {
            for (n, (label, value)) in rows.iter().enumerate() {
                let top = PAD * s / 2 + line * i32::try_from(n).unwrap_or(0);
                self.add_label(
                    card,
                    label,
                    text,
                    Role::Secondary,
                    Rect::new(PAD * s, top, 110 * s, line),
                );
                self.add_label(
                    card,
                    value,
                    text,
                    Role::BaseContent,
                    Rect::new((PAD + 120) * s, top, width - 140 * s, line),
                );
            }
        }
        y += card_h + GAP * s;
        self.add_label(
            parent,
            "Packages",
            self.style(self.strong, 15),
            Role::BaseContent,
            Rect::new(PAD * s, y, width, 28 * s),
        );
        y += 30 * s;
        let row = 22 * s;
        let card_h = PAD * s + row * i32::try_from(packages.len().max(1)).unwrap_or(1);
        if let Some(card) = self.ui.add(
            parent,
            Panel::default(),
            Rect::new(PAD * s, y, width, card_h),
        ) {
            for (n, (name, version)) in packages.iter().enumerate() {
                let top = PAD * s / 2 + row * i32::try_from(n).unwrap_or(0);
                self.add_label(
                    card,
                    name,
                    small,
                    Role::Secondary,
                    Rect::new(PAD * s, top, width / 2, row),
                );
                self.ui.add(
                    card,
                    Label::new(version.as_str())
                        .with_style(small)
                        .with_align(Align::End, Align::Center),
                    Rect::new(width / 2, top, width / 2 - PAD * s, row),
                );
            }
        }
        y += card_h + GAP * s;
        self.ui.add(
            parent,
            Button::new("Copy details", Msg::Action(Action::CopyAbout))
                .with_role(Role::Neutral)
                .with_style(self.style(self.text, 15)),
            Rect::new(PAD * s, y, BUTTON_W * s, CONTROL_H * s),
        );
        y + CONTROL_H * s
    }

    /// Put a row's control and number back to `value`, silently.
    fn show_value(&mut self, row: usize, value: &Value) {
        let control = self.rows[row].control;
        let setting = self.settings.all()[self.rows[row].setting].clone();
        self.rows[row].shown = Some(value.clone());
        match &setting.kind {
            Kind::Switch => {
                if let Some(t) = self.ui.widget_mut::<Toggle<Msg>>(control) {
                    t.set_checked(value.as_bool().unwrap_or(false));
                }
            }
            Kind::Number { .. } => {
                #[allow(clippy::cast_precision_loss)]
                if let Some(sl) = self.ui.widget_mut::<Slider<Msg>>(control) {
                    sl.set_value(value.as_number().unwrap_or(0) as f32);
                }
                if let Some(label) = self.rows[row].value_label
                    && let Some(l) = self.ui.widget_mut::<Label>(label)
                {
                    l.set_text(setting.describe(value));
                }
            }
            // A button says the same thing however often it is pressed.
            Kind::Action { .. } => {}
            Kind::Choice(choices) => {
                let i = value
                    .as_text()
                    .and_then(|t| choices.iter().position(|c| c.value == t));
                if let Some(sel) = self.ui.widget_mut::<Select<Msg>>(control) {
                    sel.set_selected(i);
                }
            }
        }
    }
}

/// What the Screensaver page's button says.
///
/// The screensaver's own name when one is chosen, because that is what pressing
/// it changes. With a different one each time there is no such name, and the
/// dialog behind the button holds all of them.
fn configure_label(chosen: Option<&Area>) -> String {
    match chosen {
        Some(area) => format!("{} settings…", area.title),
        None => "Screensaver settings…".to_owned(),
    }
}

/// What the dialog is called, and the line under it.
///
/// `one` is the single screensaver it shows, when it shows one.
fn dialog_header(one: Option<&Area>) -> (String, String) {
    match one {
        Some(area) => (area.title.to_owned(), area.description.to_owned()),
        None => (
            "Screensavers".to_owned(),
            "A different one each time is all of them, so here is every one's own page.".to_owned(),
        ),
    }
}

/// Logical pixels at a scale, as a text size.
fn px(logical: i32, scale: i32) -> u16 {
    u16::try_from(logical * scale).unwrap_or(u16::MAX)
}

/// `text` in lines no wider than `width`.
fn wrap(ui: &mut Ui<Msg>, style: TextStyle, text: &str, width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && ui.text_mut().measure_line(style, &candidate) > width {
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

#[cfg(test)]
mod tests {
    use super::{Effect, Fonts, Page, View, configure_label, dialog_header};
    use alpymist_about::info::About;
    use alpymist_settings::{Area, Settings, Value};
    use denise::{ElementState, InputEvent, KeyCode, Modifiers, Size};

    fn view() -> View {
        let settings = Settings::new();
        let values = settings
            .all()
            .iter()
            .map(|s| (s.id, Ok(s.default.clone())))
            .collect();
        View::new(
            Size::new(920, 660),
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
        )
    }

    fn key(code: KeyCode) -> [InputEvent; 2] {
        let e = |state| InputEvent::Key {
            code,
            state,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        [e(ElementState::Down), e(ElementState::Up)]
    }

    #[test]
    fn arrows_move_through_the_areas_after_a_resize() {
        let mut v = view();
        v.resize(Size::new(1260, 754), 1);
        let _ = v.handle(&key(KeyCode::ArrowDown), 10);
        assert_eq!(v.page, Page::Area(1));
    }

    #[test]
    fn tab_from_the_areas_reaches_the_first_switch() {
        let mut v = view();
        v.resize(Size::new(1260, 754), 1);
        let _ = v.handle(&key(KeyCode::ArrowDown), 10);
        let _ = v.handle(&key(KeyCode::ArrowDown), 20);
        assert_eq!(v.page, Page::Area(2));
        let _ = v.handle(&key(KeyCode::Tab), 30);
        assert_eq!(
            v.ui.focused(),
            v.rows.first().map(|r| r.control),
            "focus after Tab"
        );
        let mut events = key(KeyCode::Space).to_vec();
        events.insert(1, InputEvent::Text { ch: ' ' });
        let effects = v.handle(&events, 40);
        assert_eq!(effects.len(), 1, "{effects:?}");
    }

    #[test]
    fn a_rebuild_keeps_focus_on_the_same_setting() {
        let mut v = view();
        assert!(v.open("touchpad.tap-to-click"));
        v.resize(Size::new(1000, 700), 1);
        assert!(
            v.trace().contains("focus touchpad.tap-to-click"),
            "{}",
            v.trace()
        );
    }

    #[test]
    fn space_on_a_focused_switch_changes_it() {
        let mut v = view();
        assert!(v.open("touchpad.natural-scroll"));
        let mut events = key(KeyCode::Space).to_vec();
        events.insert(1, InputEvent::Text { ch: ' ' });
        let effects = v.handle(&events, 10);
        assert_eq!(
            effects,
            [Effect::Set {
                id: "touchpad.natural-scroll",
                value: Value::Bool(true)
            }]
        );
    }

    #[test]
    fn typing_searches_and_escape_clears_then_closes() {
        let mut v = view();
        let typed: Vec<InputEvent> = "scroll".chars().map(|ch| InputEvent::Text { ch }).collect();
        let _ = v.handle(&typed, 10);
        assert_eq!(v.page, Page::Search("scroll".into()));
        let _ = v.handle(&key(KeyCode::Escape), 20);
        assert!(matches!(v.page, Page::Area(_)));
        assert_eq!(v.handle(&key(KeyCode::Escape), 30), [Effect::Close]);
    }

    /// An area as a screensaver's definition file would produce one.
    const MOUNTAINS: Area = Area {
        id: "screensaver-mountains",
        title: "Mountains",
        description: "The ranges from the wallpaper.",
        icon: "\u{f0594}",
        keywords: &["screensaver", "mountains"],
    };

    #[test]
    fn the_side_list_is_the_pages_and_about_and_nothing_else() {
        let settings = Settings::new();
        // Whatever is installed, a screensaver never gets a page of its own:
        // its settings are reached from the Screensaver page's dialog.
        for area in settings.pages() {
            assert!(
                !settings.screensavers().iter().any(|s| s.id == area.id),
                "{} is a screensaver and is in the side list",
                area.id
            );
        }
        for area in settings.screensavers() {
            assert!(
                settings.areas().iter().any(|a| a.id == area.id),
                "{} is nowhere, so its ids resolve to nothing",
                area.id
            );
        }
    }

    #[test]
    fn the_button_is_named_after_the_screensaver_it_changes() {
        assert_eq!(configure_label(Some(&MOUNTAINS)), "Mountains settings…");
        assert_eq!(configure_label(None), "Screensaver settings…");
    }

    #[test]
    fn the_dialog_is_the_one_screensaver_or_all_of_them() {
        let (title, description) = dialog_header(Some(&MOUNTAINS));
        assert_eq!(title, "Mountains");
        assert_eq!(description, MOUNTAINS.description);
        let (title, _) = dialog_header(None);
        assert_eq!(title, "Screensavers", "a different one each time is all");
    }

    #[test]
    fn an_id_no_area_answers_for_opens_nothing() {
        let mut v = view();
        assert!(!v.open("screensaver-nothing-installed.block"));
        assert!(!v.open("nonsense"));
        assert!(
            v.open("screensaver.after"),
            "the page itself is still there"
        );
    }
}

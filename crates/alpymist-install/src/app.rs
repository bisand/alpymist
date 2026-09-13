//! The interactive installer: input handling and drawing.
//!
//! The navigation model is deliberately keyboard-first. This runs before any
//! desktop exists, on machines whose touchpad may need a driver that is not
//! loaded yet, so arrow keys and Enter have to be enough on their own.

use crate::answers::Answers;
use crate::editing;
use crate::screens::{self, Row, TextTarget};
use crate::wizard::{Step, Wizard};
use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::chrome::Chrome;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{
    ButtonStyle, button_ink, colour, paint_backdrop, paint_button, paint_panel,
};
use alpymist_ui::typeface::{self, Typeface};
use denise::geom::Point;
use denise::input::{ElementState, InputEvent, KeyCode};
use denise::painter::Pen;
use denise_render::Canvas;

/// Translate an input event into an action, or ignore it.
///
/// Separate from the event loop so the whole key map is testable without a
/// window, and so the DRM build and the desktop build cannot drift.
///
/// Space chooses and Enter continues, rather than Enter doing both. On the disk
/// screen Enter-as-choose would toggle "erase everything" and advance in one
/// keystroke, which is precisely the wrong place to be clever.
///
/// `editing_text` says whether a text field currently has focus, which is the
/// one thing that changes the map: Space is a choice everywhere else and a
/// space character inside a field. Printable characters arrive separately as
/// `Text` events, already composed, so `¨` then `o` yields one `ö`.
#[must_use]
pub fn action_for(event: &InputEvent, editing_text: bool) -> Option<Action> {
    if let InputEvent::Text { ch } = event {
        // Control characters have their own keys; only real text belongs here.
        return (editing_text && !ch.is_control()).then_some(Action::Type(*ch));
    }
    let InputEvent::Key { code, state, .. } = event else {
        return None;
    };
    if *state != ElementState::Down {
        return None;
    }
    Some(match code {
        KeyCode::ArrowUp => Action::Up,
        KeyCode::ArrowDown => Action::Down,
        KeyCode::Enter => Action::Advance,
        KeyCode::Escape => Action::Back,
        KeyCode::F10 => Action::Quit,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::Delete,
        KeyCode::ArrowLeft => Action::CaretLeft,
        KeyCode::ArrowRight => Action::CaretRight,
        KeyCode::Home => Action::CaretHome,
        KeyCode::End => Action::CaretEnd,
        // Space is a choose only where nothing is being typed; on a text field
        // it is a space, and arrives as a Text event instead.
        KeyCode::Space if !editing_text => Action::Choose,
        _ => return None,
    })
}

/// Pixel height for a layout scale.
///
/// The layout is still expressed in bitmap glyph-cell multiples, which is what
/// the panel geometry was measured against. A real font wants a pixel height,
/// and one cell is eight pixels tall.
fn px_for(scale: i32) -> u16 {
    u16::try_from(scale * 8).unwrap_or(16)
}

/// Same seed as the splash, so the mountains do not change at the handover.
pub const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// What a key press asked for, once the key itself is out of the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move the cursor up a row.
    Up,
    /// Move the cursor down a row.
    Down,
    /// Choose the row under the cursor.
    Choose,
    /// Leave this screen.
    Advance,
    /// Go back a screen.
    Back,
    /// Quit the installer.
    Quit,
    /// Type a character into the focused field.
    Type(char),
    /// Delete the character before the caret.
    Backspace,
    /// Delete the character under the caret.
    Delete,
    /// Move the caret one character left.
    CaretLeft,
    /// Move the caret one character right.
    CaretRight,
    /// Move the caret to the start of the field.
    CaretHome,
    /// Move the caret to the end of the field.
    CaretEnd,
}

/// The installer's interactive state.
pub struct App {
    /// Where we are and what has been answered.
    pub wizard: Wizard,
    /// Index into this screen's rows, not into its selectable rows.
    cursor: usize,
    /// Colours.
    pub palette: Palette,
    /// The mountains, recomposed on resize.
    backdrop: Backdrop,
    /// Panel geometry, recomputed on resize.
    chrome: Chrome,
    /// Size the above were built for.
    size: (u32, u32),
    /// Blockers reported by the last refused advance, shown until it succeeds.
    pub reported: Vec<String>,
    /// Caret position within the focused text field, in characters.
    caret: usize,
    /// Set when the user asks to quit.
    pub quitting: bool,
    /// Fira Mono where available, the built-in bitmap otherwise.
    pub face: Typeface,
}

impl App {
    /// Start at the welcome screen.
    #[must_use]
    pub fn new(answers: Answers, width: u32, height: u32) -> Self {
        let palette = Palette::alpymist();
        let mut app = Self {
            wizard: Wizard::new(answers),
            cursor: 0,
            backdrop: Backdrop::compose(width, height, &palette, SCENE_SEED),
            chrome: Chrome::for_screen(width, height),
            palette,
            size: (width, height),
            reported: Vec::new(),
            caret: 0,
            quitting: false,
            face: typeface::load(),
        };
        app.snap_cursor();
        app
    }

    /// The row the cursor is on.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Put the cursor on the first selectable row, or on the chosen one.
    ///
    /// Landing on the current choice rather than the top of the list means
    /// arrowing down from a screen you have already answered does not silently
    /// jump your selection back to the first option.
    fn snap_cursor(&mut self) {
        let rows = screens::rows(self.wizard.step(), &self.wizard.answers);
        self.cursor = rows
            .iter()
            .position(|r| r.selectable() && r.chosen)
            .or_else(|| rows.iter().position(Row::selectable))
            .unwrap_or(0);
        self.place_caret();
    }

    /// Put the caret after whatever is already in the focused field.
    ///
    /// Called from both ways of arriving at a field — arrowing onto it and
    /// landing on it when the screen opens. Only one of those did it at first,
    /// so opening the account screen put the caret in front of the name
    /// already there.
    fn place_caret(&mut self) {
        self.caret = self
            .focused_field()
            .map_or(0, |f| editing::length(f.value(&self.wizard.answers)));
    }

    /// Move the cursor, skipping rows it cannot land on.
    ///
    /// Stops at the ends rather than wrapping: wrapping in a short list makes
    /// it easy to overshoot and not notice.
    fn move_cursor(&mut self, down: bool) {
        let options = screens::selectable(self.wizard.step(), &self.wizard.answers);
        if options.is_empty() {
            return;
        }
        let here = options.iter().position(|i| *i == self.cursor);
        let next = match (here, down) {
            (Some(i), true) => (i + 1).min(options.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        self.cursor = options[next];

        // Arrowing through a list of layouts should pick as you go, but
        // arrowing past a checkbox must never flip it.
        let rows = screens::rows(self.wizard.step(), &self.wizard.answers);
        if rows.get(self.cursor).is_some_and(Row::selects_on_focus) {
            screens::choose(self.wizard.step(), self.cursor, &mut self.wizard.answers);
            self.reported.clear();
        }
        // Arriving in a field puts the caret after what is already there, which
        // is where you want it when correcting a value rather than replacing it.
        self.place_caret();
    }

    /// The text field the cursor is on, if it is on one.
    #[must_use]
    pub fn focused_field(&self) -> Option<TextTarget> {
        screens::rows(self.wizard.step(), &self.wizard.answers)
            .get(self.cursor)
            .and_then(Row::text_target)
    }

    /// Where the caret sits in the focused field.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Apply an edit to the focused field, if there is one.
    ///
    /// Typing anywhere else is simply ignored rather than being an error: a
    /// stray keystroke on the disk screen should do nothing at all.
    fn edit(&mut self, action: Action) {
        let Some(field) = self.focused_field() else {
            return;
        };
        let caret = self.caret;
        let value = field.value_mut(&mut self.wizard.answers);
        self.caret = match action {
            Action::Type(ch) => editing::insert(value, caret, ch),
            Action::Backspace => editing::backspace(value, caret),
            Action::Delete => editing::delete(value, caret),
            Action::CaretLeft => editing::left(value, caret),
            Action::CaretRight => editing::right(value, caret),
            Action::CaretHome => editing::home(),
            Action::CaretEnd => editing::end(value),
            _ => caret,
        };
        // Typing is how you fix what the last refusal complained about, so the
        // complaint should not outlive the first keystroke.
        self.reported.clear();
    }

    /// Apply an action.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Type(_)
            | Action::Backspace
            | Action::Delete
            | Action::CaretLeft
            | Action::CaretRight
            | Action::CaretHome
            | Action::CaretEnd => self.edit(action),
            Action::Up => self.move_cursor(false),
            Action::Down => self.move_cursor(true),
            Action::Choose => {
                screens::choose(self.wizard.step(), self.cursor, &mut self.wizard.answers);
                self.reported.clear();
            }
            Action::Advance => match self.wizard.advance() {
                Ok(_) => {
                    self.reported.clear();
                    self.snap_cursor();
                }
                Err(blockers) => {
                    self.reported = blockers.into_iter().map(|i| i.message).collect();
                }
            },
            Action::Back => {
                if self.wizard.back() {
                    self.reported.clear();
                    self.snap_cursor();
                }
            }
            Action::Quit => self.quitting = true,
        }
    }

    /// Recompose for a new screen size. Returns whether anything changed.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        if self.size == (width, height) {
            return false;
        }
        self.backdrop = Backdrop::compose(width, height, &self.palette, SCENE_SEED);
        self.chrome = Chrome::for_screen(width, height);
        self.size = (width, height);
        true
    }

    /// The label and style of the primary button on this screen.
    #[must_use]
    pub fn primary_button(&self) -> (&'static str, ButtonStyle) {
        match self.wizard.step() {
            Step::Welcome => ("Enter  Begin", ButtonStyle::Primary),
            Step::Confirm => ("Enter  Install", ButtonStyle::Primary),
            Step::Install => ("Working", ButtonStyle::Disabled),
            Step::Done => ("Enter  Restart", ButtonStyle::Primary),
            _ if self.wizard.can_advance() => ("Enter  Continue", ButtonStyle::Primary),
            _ => ("Enter  Continue", ButtonStyle::Disabled),
        }
    }

    /// Draw the whole screen.
    pub fn draw(&mut self, canvas: &mut Canvas<'_>) {
        let size = canvas.size();
        self.resize(size.width, size.height);

        let chrome = self.chrome;
        let palette = self.palette;
        let step = self.wizard.step();

        paint_backdrop(canvas, &self.backdrop);
        paint_panel(canvas, &chrome, &palette);
        self.paint_buttons(canvas);

        // One Pen for every glyph on the screen. It borrows the canvas, not
        // self, so the typeface can still be borrowed mutably to rasterise.
        let mut pen = Pen::new(canvas);
        let text_px = px_for(chrome.text_scale);
        let title_px = px_for(chrome.title_scale);

        if let Some(n) = step.question_number() {
            self.face.draw(
                &mut pen,
                Point::new(chrome.counter_at.0, chrome.counter_at.1),
                text_px,
                &format!("STEP {n} OF {}", Step::questions().count()),
                colour(palette.accent),
            );
        }
        self.face.draw(
            &mut pen,
            Point::new(chrome.title_at.0, chrome.title_at.1),
            title_px,
            step.title(),
            colour(palette.ink),
        );
        self.face.draw(
            &mut pen,
            Point::new(chrome.subtitle_at.0, chrome.subtitle_at.1),
            text_px,
            step.subtitle(),
            colour(palette.ink_dim),
        );

        self.draw_rows(&mut pen);
        self.draw_footer_text(&mut pen);
    }

    /// The button shapes, which need a painter rather than a pen.
    fn paint_buttons(&self, canvas: &mut Canvas<'_>) {
        let back_style = if self.wizard.can_go_back() {
            ButtonStyle::Quiet
        } else {
            ButtonStyle::Disabled
        };
        paint_button(canvas, self.chrome.back_button, back_style, &self.palette);
        paint_button(
            canvas,
            self.chrome.next_button,
            self.primary_button().1,
            &self.palette,
        );
    }

    fn draw_rows(&mut self, pen: &mut Pen<'_>) {
        let chrome = self.chrome;
        let palette = self.palette;
        let rows: Vec<Row> = screens::rows(self.wizard.step(), &self.wizard.answers);

        for (index, row) in rows.iter().enumerate() {
            if row.text.is_empty() {
                continue;
            }
            let y = chrome.body_row(i32::try_from(index).unwrap_or(0));
            let under_cursor = row.selectable() && index == self.cursor;
            // Cursor is brightest, the current choice is accented, and
            // everything else — selectable or not — is dim. Whether a dim row
            // can be landed on is carried by the cursor moving there, not by
            // its colour, so an unreachable row does not look disabled.
            let ink = if under_cursor {
                palette.ink
            } else if row.chosen {
                palette.accent
            } else {
                palette.ink_dim
            };
            let prefix = match (under_cursor, row.chosen) {
                (true, _) => ">  ",
                (false, true) => "*  ",
                (false, false) => "   ",
            };

            // A text row draws its label and its current value; everything else
            // is already the whole line.
            let line = match row.text_target() {
                Some(field) => {
                    let shown = editing::with_caret(
                        field.value(&self.wizard.answers),
                        self.caret,
                        field.is_secret(),
                        under_cursor,
                    );
                    format!("{prefix}{:<12}{shown}", row.text)
                }
                None => format!("{prefix}{}", row.text),
            };

            self.face.draw(
                pen,
                Point::new(chrome.body.0, y),
                px_for(chrome.text_scale),
                &line,
                colour(ink),
            );
        }
    }

    fn draw_footer_text(&mut self, pen: &mut Pen<'_>) {
        let chrome = self.chrome;
        let palette = self.palette;

        // Blockers take precedence over advisories: one is why you cannot
        // continue, the other is only worth knowing.
        let note = self
            .reported
            .first()
            .cloned()
            .or_else(|| self.wizard.advisories().first().map(|a| a.message.clone()));
        if let Some(note) = note {
            let ink = if self.reported.is_empty() {
                palette.ink_dim
            } else {
                palette.accent
            };
            self.face.draw(
                pen,
                Point::new(chrome.advisory_at.0, chrome.advisory_at.1),
                px_for(chrome.text_scale),
                &note,
                colour(ink),
            );
        }

        let back_style = if self.wizard.can_go_back() {
            ButtonStyle::Quiet
        } else {
            ButtonStyle::Disabled
        };
        for (rect, label, style) in [
            (chrome.back_button, "Esc  Back", back_style),
            (
                chrome.next_button,
                self.primary_button().0,
                self.primary_button().1,
            ),
        ] {
            let size_px = px_for(chrome.text_scale);
            let at = self.face.centre_in(rect, size_px, label);
            self.face
                .draw(pen, at, size_px, label, colour(button_ink(style, &palette)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, App};
    use crate::answers::{Answers, DiskPlan, Network};
    use crate::screens::{self, TextTarget};
    use crate::wizard::Step;
    use alpymist_core::Tier;
    use denise::input::{ElementState, InputEvent, KeyCode};

    fn app() -> App {
        App::new(
            Answers {
                detected_tier: Some(Tier::Lite),
                ..Answers::default()
            },
            1280,
            800,
        )
    }

    #[test]
    fn the_cursor_starts_on_a_selectable_row() {
        let mut a = app();
        a.act(Action::Advance); // Welcome -> Keyboard
        let rows = screens::rows(a.wizard.step(), &a.wizard.answers);
        assert!(rows[a.cursor()].selectable());
    }

    #[test]
    fn moving_down_then_up_returns_to_where_it_started() {
        let mut a = app();
        a.act(Action::Advance);
        let start = a.cursor();
        a.act(Action::Down);
        assert_ne!(a.cursor(), start);
        a.act(Action::Up);
        assert_eq!(a.cursor(), start);
    }

    /// Wrapping in a short list makes it easy to overshoot without noticing.
    #[test]
    fn the_cursor_stops_at_the_ends_rather_than_wrapping() {
        let mut a = app();
        a.act(Action::Advance);
        for _ in 0..50 {
            a.act(Action::Down);
        }
        let bottom = a.cursor();
        a.act(Action::Down);
        assert_eq!(a.cursor(), bottom, "wrapped off the bottom");
        for _ in 0..50 {
            a.act(Action::Up);
        }
        let top = a.cursor();
        a.act(Action::Up);
        assert_eq!(a.cursor(), top, "wrapped off the top");
    }

    #[test]
    fn the_cursor_never_lands_on_an_unselectable_row() {
        let mut a = app();
        a.act(Action::Advance);
        while a.wizard.step() != Step::Confirm {
            for _ in 0..12 {
                a.act(Action::Down);
                let rows = screens::rows(a.wizard.step(), &a.wizard.answers);
                if !screens::selectable(a.wizard.step(), &a.wizard.answers).is_empty() {
                    assert!(
                        rows[a.cursor()].selectable(),
                        "{:?}: cursor on unselectable row {}",
                        a.wizard.step(),
                        a.cursor()
                    );
                }
                a.act(Action::Choose);
            }
            let before = a.wizard.step();
            a.act(Action::Advance);
            if a.wizard.step() == before {
                break; // genuinely blocked; the account screen needs typing
            }
        }
    }

    #[test]
    fn choosing_then_advancing_records_the_choice() {
        let mut a = app();
        a.act(Action::Advance); // Keyboard
        a.act(Action::Down);
        a.act(Action::Choose);
        a.act(Action::Advance);
        assert_eq!(a.wizard.step(), Step::Region);
        assert!(a.wizard.answers.keyboard.is_some());
    }

    #[test]
    fn a_refused_advance_reports_why_and_stays_put() {
        let mut a = app();
        a.act(Action::Advance); // Keyboard, nothing chosen
        a.act(Action::Advance);
        assert_eq!(a.wizard.step(), Step::Keyboard);
        assert!(!a.reported.is_empty(), "the user is told nothing");
    }

    #[test]
    fn the_report_clears_once_the_screen_is_satisfied() {
        let mut a = app();
        a.act(Action::Advance);
        a.act(Action::Advance); // refused
        assert!(!a.reported.is_empty());
        a.act(Action::Choose);
        assert!(a.reported.is_empty(), "a stale error would linger");
    }

    #[test]
    fn going_back_returns_to_the_previous_screen() {
        let mut a = app();
        a.act(Action::Advance);
        a.act(Action::Choose);
        a.act(Action::Advance); // Region
        a.act(Action::Back);
        assert_eq!(a.wizard.step(), Step::Keyboard);
    }

    #[test]
    fn the_cursor_lands_on_the_existing_choice_when_returning_to_a_screen() {
        let mut a = app();
        a.act(Action::Advance);
        a.act(Action::Down);
        a.act(Action::Down);
        a.act(Action::Choose);
        let chosen = a.cursor();
        a.act(Action::Advance);
        a.act(Action::Back);
        assert_eq!(a.cursor(), chosen, "returning reset the selection");
    }

    #[test]
    fn the_primary_button_is_disabled_while_the_screen_is_incomplete() {
        use alpymist_ui::render::ButtonStyle;
        let mut a = app();
        a.act(Action::Advance); // Keyboard, nothing chosen
        assert_eq!(a.primary_button().1, ButtonStyle::Disabled);
        a.act(Action::Choose);
        assert_eq!(a.primary_button().1, ButtonStyle::Primary);
    }

    #[test]
    fn resizing_recomposes_once_and_not_again_for_the_same_size() {
        let mut a = app();
        assert!(a.resize(1920, 1080));
        assert!(!a.resize(1920, 1080));
    }

    fn key(code: KeyCode) -> InputEvent {
        InputEvent::Key {
            code,
            state: ElementState::Down,
            repeat: false,
            modifiers: denise::input::Modifiers::default(),
        }
    }

    #[test]
    fn the_navigation_keys_map_to_the_expected_actions() {
        for (code, expected) in [
            (KeyCode::ArrowUp, Action::Up),
            (KeyCode::ArrowDown, Action::Down),
            (KeyCode::Space, Action::Choose),
            (KeyCode::Enter, Action::Advance),
            (KeyCode::Escape, Action::Back),
            (KeyCode::F10, Action::Quit),
        ] {
            assert_eq!(
                super::action_for(&key(code), false),
                Some(expected),
                "{code:?}"
            );
        }
    }

    #[test]
    fn keys_we_do_not_use_are_ignored() {
        for code in [KeyCode::A, KeyCode::Tab, KeyCode::F1] {
            assert_eq!(super::action_for(&key(code), false), None, "{code:?}");
        }
    }

    #[test]
    fn the_editing_keys_map_to_editing_actions() {
        for (code, expected) in [
            (KeyCode::Backspace, Action::Backspace),
            (KeyCode::Delete, Action::Delete),
            (KeyCode::ArrowLeft, Action::CaretLeft),
            (KeyCode::ArrowRight, Action::CaretRight),
            (KeyCode::Home, Action::CaretHome),
            (KeyCode::End, Action::CaretEnd),
        ] {
            assert_eq!(
                super::action_for(&key(code), true),
                Some(expected),
                "{code:?}"
            );
        }
    }

    /// Space is a choice on a list and a space character in a field. Getting
    /// this backwards means you cannot type a space in your own full name.
    #[test]
    fn space_chooses_on_a_list_but_not_while_typing() {
        assert_eq!(
            super::action_for(&key(KeyCode::Space), false),
            Some(Action::Choose)
        );
        assert_eq!(super::action_for(&key(KeyCode::Space), true), None);
    }

    #[test]
    fn typed_characters_only_arrive_while_a_field_has_focus() {
        let text = InputEvent::Text { ch: 'å' };
        assert_eq!(super::action_for(&text, true), Some(Action::Type('å')));
        assert_eq!(
            super::action_for(&text, false),
            None,
            "typing off a field does nothing"
        );
    }

    #[test]
    fn control_characters_are_not_treated_as_text() {
        let tab = InputEvent::Text { ch: '\t' };
        assert_eq!(super::action_for(&tab, true), None);
    }

    /// Acting on release as well as press would double every keystroke.
    #[test]
    fn key_releases_do_nothing() {
        let release = InputEvent::Key {
            code: KeyCode::Enter,
            state: ElementState::Up,
            repeat: false,
            modifiers: denise::input::Modifiers::default(),
        };
        assert_eq!(super::action_for(&release, false), None);
    }

    /// Arrowing through a list of layouts should pick as you go.
    #[test]
    fn moving_onto_a_radio_row_selects_it() {
        let mut a = app();
        a.act(Action::Advance); // Keyboard
        a.act(Action::Down);
        assert!(
            a.wizard.answers.keyboard.is_some(),
            "arrowing did not select a layout"
        );
    }

    /// A wizard sitting on the Disk screen with a disk already chosen.
    ///
    /// Built by setting answers rather than walking, so the test exercises the
    /// toggle behaviour and not the route to it.
    fn at_disk() -> App {
        let answers = Answers {
            keyboard: Some("no".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sda".into(),
                encrypt: false,
            }),
            detected_tier: Some(Tier::Lite),
            ..Answers::default()
        };
        let mut a = App::new(answers, 1280, 800);
        for _ in 0..4 {
            a.act(Action::Advance);
        }
        assert_eq!(
            a.wizard.step(),
            Step::Disk,
            "fixture did not reach the disk screen"
        );
        a
    }

    /// ...but arrowing past a checkbox must never flip it.
    #[test]
    fn moving_onto_a_toggle_row_leaves_it_alone() {
        let mut a = at_disk();
        let confirmed = a.wizard.answers.disk_confirmed;
        let encrypted = matches!(
            &a.wizard.answers.disk,
            Some(DiskPlan::WholeDisk { encrypt: true, .. })
        );
        for _ in 0..8 {
            a.act(Action::Down);
        }
        assert_eq!(
            a.wizard.answers.disk_confirmed, confirmed,
            "arrowing over the erase checkbox flipped it"
        );
        assert_eq!(
            matches!(
                &a.wizard.answers.disk,
                Some(DiskPlan::WholeDisk { encrypt: true, .. })
            ),
            encrypted,
            "arrowing over the encryption checkbox flipped it"
        );
    }

    #[test]
    fn choosing_a_toggle_row_flips_it() {
        let mut a = at_disk();
        for _ in 0..8 {
            a.act(Action::Down);
        }
        let before = a.wizard.answers.disk_confirmed;
        a.act(Action::Choose);
        assert_ne!(
            a.wizard.answers.disk_confirmed, before,
            "Space did not toggle"
        );
    }

    /// A wizard sitting on the Account screen with everything before it done.
    fn at_account() -> App {
        let answers = Answers {
            keyboard: Some("no".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sda".into(),
                encrypt: true,
            }),
            disk_confirmed: true,
            detected_tier: Some(Tier::Lite),
            ..Answers::default()
        };
        let mut a = App::new(answers, 1280, 800);
        for _ in 0..5 {
            a.act(Action::Advance);
        }
        assert_eq!(
            a.wizard.step(),
            Step::Account,
            "fixture did not reach the account screen"
        );
        a
    }

    fn type_text(a: &mut App, text: &str) {
        for ch in text.chars() {
            a.act(Action::Type(ch));
        }
    }

    #[test]
    fn the_account_screen_starts_focused_on_a_field() {
        let a = at_account();
        assert_eq!(a.focused_field(), Some(TextTarget::FullName));
    }

    /// Opening the screen must place the caret just as arrowing onto a field
    /// does, or the first keystroke lands in front of the existing value.
    #[test]
    fn the_caret_is_placed_when_a_screen_opens_not_only_when_arrowing() {
        let answers = Answers {
            keyboard: Some("no".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sda".into(),
                encrypt: true,
            }),
            disk_confirmed: true,
            full_name: "André Biseth".into(),
            detected_tier: Some(Tier::Lite),
            ..Answers::default()
        };
        let mut a = App::new(answers, 1280, 800);
        for _ in 0..5 {
            a.act(Action::Advance);
        }
        assert_eq!(a.wizard.step(), Step::Account);
        assert_eq!(
            a.caret(),
            "André Biseth".chars().count(),
            "caret not at the end"
        );
        type_text(&mut a, "!");
        assert_eq!(a.wizard.answers.full_name, "André Biseth!");
    }

    #[test]
    fn typing_fills_the_focused_field_and_nothing_else() {
        let mut a = at_account();
        type_text(&mut a, "André Biseth");
        assert_eq!(a.wizard.answers.full_name, "André Biseth");
        assert!(
            a.wizard.answers.username.is_empty(),
            "typing leaked into another field"
        );
    }

    /// The whole reason the caret counts characters rather than bytes.
    #[test]
    fn norwegian_characters_survive_being_typed_and_corrected() {
        let mut a = at_account();
        type_text(&mut a, "blåbærsyltetøy");
        assert_eq!(a.wizard.answers.full_name, "blåbærsyltetøy");
        a.act(Action::Backspace);
        assert_eq!(a.wizard.answers.full_name, "blåbærsyltetø");
        a.act(Action::CaretHome);
        type_text(&mut a, "Ø");
        assert_eq!(a.wizard.answers.full_name, "Øblåbærsyltetø");
    }

    #[test]
    fn arrowing_between_fields_moves_the_focus() {
        let mut a = at_account();
        a.act(Action::Down);
        assert_eq!(a.focused_field(), Some(TextTarget::Username));
        type_text(&mut a, "andre");
        assert_eq!(a.wizard.answers.username, "andre");
        assert!(a.wizard.answers.full_name.is_empty());
    }

    /// Arriving in a field should let you correct it, not overwrite it.
    #[test]
    fn the_caret_lands_after_existing_text_when_entering_a_field() {
        let mut a = at_account();
        type_text(&mut a, "andre");
        a.act(Action::Down);
        a.act(Action::Up);
        assert_eq!(
            a.caret(),
            5,
            "caret should sit at the end of the existing value"
        );
        type_text(&mut a, "!");
        assert_eq!(a.wizard.answers.full_name, "andre!");
    }

    #[test]
    fn every_account_field_can_be_reached_and_typed_into() {
        let mut a = at_account();
        for (index, field) in TextTarget::ACCOUNT.iter().enumerate() {
            for _ in 0..index {
                a.act(Action::Down);
            }
            assert_eq!(a.focused_field(), Some(*field), "could not reach {field:?}");
            type_text(&mut a, "x");
            assert_eq!(field.value(&a.wizard.answers), "x");
            for _ in 0..index {
                a.act(Action::Up);
            }
        }
    }

    #[test]
    fn a_completed_account_screen_advances() {
        let mut a = at_account();
        type_text(&mut a, "André Biseth");
        a.act(Action::Down);
        type_text(&mut a, "andre");
        a.act(Action::Down);
        type_text(&mut a, "a good passphrase");
        a.act(Action::Down);
        type_text(&mut a, "a good passphrase");
        a.act(Action::Down);
        type_text(&mut a, "alpymist");
        a.act(Action::Advance);
        assert_eq!(
            a.wizard.step(),
            Step::Desktop,
            "blocked by: {:?}",
            a.reported
        );
    }

    #[test]
    fn typing_clears_a_complaint_from_the_previous_refusal() {
        let mut a = at_account();
        a.act(Action::Advance); // refused: nothing filled in
        assert!(!a.reported.is_empty());
        type_text(&mut a, "a");
        assert!(a.reported.is_empty(), "the complaint outlived the fix");
    }

    #[test]
    fn typing_where_there_is_no_field_does_nothing() {
        let mut a = app();
        a.act(Action::Advance); // Keyboard: a list, not fields
        let before = a.wizard.answers.clone();
        type_text(&mut a, "hello");
        a.act(Action::Backspace);
        assert_eq!(a.wizard.answers, before);
    }

    #[test]
    fn quitting_is_recorded() {
        let mut a = app();
        assert!(!a.quitting);
        a.act(Action::Quit);
        assert!(a.quitting);
    }
}

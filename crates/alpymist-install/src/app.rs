//! The interactive installer: input handling and drawing.
//!
//! The navigation model is deliberately keyboard-first. This runs before any
//! desktop exists, on machines whose touchpad may need a driver that is not
//! loaded yet, so arrow keys and Enter have to be enough on their own.

use crate::answers::Answers;
use crate::editing;
use crate::execute::{Mode, Progress};
use crate::plan;
use crate::pointer;
use crate::screens::{self, Row, TextTarget};
use crate::wizard::{Step, Wizard};
use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::chrome::Chrome;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{
    ButtonStyle, button_ink, colour, new_cursor, paint_backdrop, paint_button, paint_cursor,
    paint_panel,
};
use alpymist_ui::typeface::{self, Typeface};
use denise::geom::Point;
use denise::input::{ElementState, InputEvent, KeyCode, Modifiers};
use denise::painter::Pen;
use denise::theme::Theme;
use denise_render::Canvas;
use denise_ui::cursor::Cursor;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

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
    match event {
        InputEvent::PointerMoved { position } => {
            return Some(Action::PointerTo(position.x, position.y));
        }
        InputEvent::PointerButton {
            button,
            state,
            position,
            ..
        } if *button == denise::input::PointerButton::Left && *state == ElementState::Down => {
            return Some(Action::ClickAt(position.x, position.y));
        }
        _ => {}
    }
    if let InputEvent::Text { ch } = event {
        // Control characters have their own keys; only real text belongs here.
        return (editing_text && !ch.is_control()).then_some(Action::Type(*ch));
    }
    let InputEvent::Key {
        code,
        state,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if *state != ElementState::Down {
        return None;
    }
    Some(match code {
        KeyCode::ArrowUp => Action::Up,
        KeyCode::ArrowDown => Action::Down,
        KeyCode::Tab if modifiers.contains(Modifiers::SHIFT) => Action::PreviousField,
        KeyCode::Tab => Action::NextField,
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
    /// Jump to the next text field, wrapping after the last.
    NextField,
    /// Jump to the previous text field, wrapping before the first.
    PreviousField,
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
    /// The pointer moved to this surface position.
    PointerTo(i32, i32),
    /// The primary pointer button went down at this position.
    ClickAt(i32, i32),
}

/// An install in flight.
///
/// The work runs on its own thread so the screen keeps redrawing — an
/// installer that freezes while partitioning looks exactly like one that has
/// hung, and this is the worst possible moment to look like that.
struct Running {
    events: Receiver<Progress>,
    /// What has been reported, newest last.
    lines: Vec<String>,
    /// `(step, total)` currently running.
    at: Option<(usize, usize)>,
    /// Set when the plan finished, with whether it succeeded.
    outcome: Option<bool>,
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
    /// Where the pointer is, once it has moved at all.
    ///
    /// `None` until the first motion, which is what keeps a keyboard-only
    /// machine from showing a pointer it has no way to move.
    pub pointer: Option<(i32, i32)>,
    /// Set when the user asks to quit.
    pub quitting: bool,
    /// Fira Mono where available, the built-in bitmap otherwise.
    pub face: Typeface,
    /// The mouse pointer sprite, hidden until the pointer moves.
    ///
    /// Named apart from `cursor`, which in this app means the keyboard
    /// selection — two different things that both want that word.
    pointer_sprite: Cursor,
    /// Colours for the cursor sprite.
    theme: Theme,
    /// Whether an install would really be carried out.
    mode: Mode,
    /// The install, once it has started.
    install: Option<Running>,
}

impl App {
    /// Start at the welcome screen.
    #[must_use]
    pub fn new(answers: Answers, width: u32, height: u32) -> Self {
        Self::with_mode(answers, width, height, Mode::DryRun)
    }

    /// Start with an explicit install mode.
    ///
    /// `DryRun` is the default everywhere else, so nothing can destroy a disk
    /// by omitting an argument.
    #[must_use]
    pub fn with_mode(answers: Answers, width: u32, height: u32, mode: Mode) -> Self {
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
            pointer: None,
            quitting: false,
            face: typeface::load(),
            pointer_sprite: new_cursor(),
            theme: denise::theme::DARK,
            mode,
            install: None,
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
        self.land_on(options[next]);
    }

    /// Tab between text fields, the way every form on every desktop does.
    ///
    /// Unlike the arrows this wraps, because that is what Tab does elsewhere
    /// and fields are few enough that overshooting is obvious. Screens without
    /// fields ignore it rather than guessing what Tab should mean on a list.
    fn move_to_field(&mut self, forward: bool) {
        let rows = screens::rows(self.wizard.step(), &self.wizard.answers);
        let fields: Vec<usize> = (0..rows.len())
            .filter(|i| rows[*i].text_target().is_some())
            .collect();
        let (Some(&first), Some(&last)) = (fields.first(), fields.last()) else {
            return;
        };
        let next = if forward {
            fields
                .iter()
                .copied()
                .find(|i| *i > self.cursor)
                .unwrap_or(first)
        } else {
            fields
                .iter()
                .copied()
                .rev()
                .find(|i| *i < self.cursor)
                .unwrap_or(last)
        };
        self.land_on(next);
    }

    /// Put the cursor on a row and do what arriving there implies.
    fn land_on(&mut self, index: usize) {
        self.cursor = index;

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
            Action::NextField => self.move_to_field(true),
            Action::PreviousField => self.move_to_field(false),
            Action::Choose => {
                screens::choose(self.wizard.step(), self.cursor, &mut self.wizard.answers);
                self.reported.clear();
            }
            Action::Advance => match self.wizard.advance() {
                Ok(step) => {
                    self.reported.clear();
                    self.snap_cursor();
                    if step == Step::Install {
                        self.begin_install();
                    }
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
            Action::PointerTo(x, y) => {
                self.pointer = Some((x, y));
                self.pointer_sprite.position = Point::new(x, y);
                // First motion is what reveals it; see new_cursor.
                self.pointer_sprite.visible = true;
            }
            Action::ClickAt(x, y) => {
                self.pointer = Some((x, y));
                self.click(x, y);
            }
            Action::Quit => self.quitting = true,
        }
    }

    /// Act on a click at a surface position.
    ///
    /// Clicking a row both moves the cursor there and chooses it. Keyboard
    /// navigation separates those — arrow then Space — because the cursor has
    /// to pass over rows on its way. A pointer does not pass over anything, so
    /// requiring a second click to confirm what was just aimed at would be
    /// pure ceremony.
    fn click(&mut self, x: i32, y: i32) {
        let rows = screens::rows(self.wizard.step(), &self.wizard.answers);
        match pointer::hit_test(&self.chrome, rows.len(), x, y) {
            pointer::Hit::Row(index) => {
                if rows.get(index).is_some_and(Row::selectable) {
                    self.cursor = index;
                    self.place_caret();
                    // A text field only takes focus; anything else is a choice.
                    if rows[index].text_target().is_none() {
                        self.act(Action::Choose);
                    } else {
                        self.reported.clear();
                    }
                }
            }
            pointer::Hit::Next => self.act(Action::Advance),
            pointer::Hit::Back => self.act(Action::Back),
            pointer::Hit::Nothing => {}
        }
    }

    /// Build the plan and start carrying it out.
    ///
    /// Failure to plan is reported on the screen rather than thrown away: the
    /// reasons are things the user can go back and fix.
    fn begin_install(&mut self) {
        if self.install.is_some() {
            return;
        }
        let plan = match plan::build(&self.wizard.answers) {
            Ok(plan) => plan,
            Err(why) => {
                self.reported = vec![why.message()];
                return;
            }
        };

        let (tx, events) = channel();
        let mode = self.mode;
        // Its own thread, so the screen keeps redrawing while a disk is being
        // partitioned. The plan is moved in; nothing is shared.
        std::thread::spawn(move || {
            crate::execute::run(&plan, mode, &mut |progress| {
                // The receiver going away means the installer is shutting down,
                // which is not this thread's problem.
                let _ = tx.send(progress);
            });
        });

        self.install = Some(Running {
            events,
            lines: Vec::new(),
            at: None,
            outcome: None,
        });
    }

    /// Advance anything that happens on its own, without drawing.
    ///
    /// Separate from [`draw`](Self::draw) deliberately. Draining progress only
    /// while rendering would tie the install's visible state to the redraw
    /// rate, so a screen that stopped repainting would look like an install
    /// that had frozen — and would be untestable without a framebuffer.
    pub fn tick(&mut self) {
        self.drain_install();
    }

    /// Take whatever the install thread has reported since the last frame.
    fn drain_install(&mut self) {
        let Some(running) = self.install.as_mut() else {
            return;
        };
        loop {
            match running.events.try_recv() {
                Ok(Progress::Starting {
                    index,
                    total,
                    title,
                    ..
                }) => {
                    running.at = Some((index, total));
                    running.lines.push(title);
                }
                Ok(Progress::Output(line)) => running.lines.push(format!("   {line}")),
                Ok(Progress::Finished { .. }) => {}
                Ok(Progress::Refused(reasons)) => {
                    running.lines.push("Refused to write to this disk:".into());
                    running
                        .lines
                        .extend(reasons.into_iter().map(|r| format!("   {r}")));
                }
                Ok(Progress::Done { ok }) => running.outcome = Some(ok),
                Err(TryRecvError::Empty) => break,
                // The thread finished and dropped the sender.
                Err(TryRecvError::Disconnected) => {
                    if running.outcome.is_none() {
                        running.outcome = Some(false);
                    }
                    break;
                }
            }
        }
    }

    /// Whether an install is running and still has something to report.
    #[must_use]
    pub fn installing(&self) -> bool {
        self.install.as_ref().is_some_and(|r| r.outcome.is_none())
    }

    /// What the Install screen should show right now.
    #[must_use]
    pub fn install_lines(&self) -> Vec<String> {
        match self.install.as_ref() {
            None => vec!["Preparing".into()],
            Some(running) => {
                let mut lines = Vec::new();
                if let Some((index, total)) = running.at {
                    lines.push(format!("Step {} of {total}", index + 1));
                }
                // Only the tail fits, and the tail is what matters. Two rows
                // are kept for the step counter and the final outcome.
                let room = usize::try_from(self.chrome.body_rows() - 2)
                    .unwrap_or(1)
                    .max(1);
                let shown = running.lines.len().saturating_sub(room);
                lines.extend(running.lines[shown..].iter().cloned());
                match running.outcome {
                    Some(true) => lines.push("Finished.".into()),
                    Some(false) => lines.push("Stopped. Nothing further was done.".into()),
                    None => {}
                }
                lines
            }
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
        self.tick();

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

        // Last, so it is over everything.
        paint_cursor(&mut pen, &self.pointer_sprite, &self.theme);
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
        // The Install screen reports what is actually happening rather than a
        // fixed list of what was going to happen.
        let rows: Vec<Row> = if self.wizard.step() == Step::Install {
            self.install_lines()
                .into_iter()
                .map(Row::progress)
                .collect()
        } else {
            screens::rows(self.wizard.step(), &self.wizard.answers)
        };

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
    use denise::geom::Point;
    use denise::input::{ElementState, InputEvent, KeyCode, Modifiers};

    fn app() -> App {
        App::new(
            Answers {
                detected_tier: Some(Tier::Lite),
                disks: crate::disks::sample(),
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
    fn tab_and_shift_tab_move_between_fields_even_while_typing() {
        let shift_tab = InputEvent::Key {
            code: KeyCode::Tab,
            state: ElementState::Down,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        };
        for editing in [false, true] {
            assert_eq!(
                super::action_for(&key(KeyCode::Tab), editing),
                Some(Action::NextField)
            );
            assert_eq!(
                super::action_for(&shift_tab, editing),
                Some(Action::PreviousField)
            );
        }
    }

    #[test]
    fn tab_walks_the_account_fields_and_wraps() {
        let mut a = at_account();
        let fields = TextTarget::ACCOUNT.len();
        let start = a.focused_field();
        assert!(start.is_some());
        let mut seen = vec![start];
        for _ in 1..fields {
            a.act(Action::NextField);
            seen.push(a.focused_field());
        }
        for (i, f) in seen.iter().enumerate() {
            assert!(!seen[..i].contains(f), "Tab revisited {f:?} early");
        }
        a.act(Action::NextField);
        assert_eq!(
            a.focused_field(),
            start,
            "Tab should wrap to the first field"
        );
        a.act(Action::PreviousField);
        assert_eq!(
            a.focused_field(),
            seen[fields - 1],
            "Shift+Tab should wrap back"
        );
    }

    #[test]
    fn tab_puts_the_caret_at_the_end_of_the_field_it_lands_on() {
        let mut a = at_account();
        a.act(Action::Type('x'));
        a.act(Action::Type('y'));
        a.act(Action::NextField);
        a.act(Action::PreviousField);
        assert_eq!(a.caret(), 2);
    }

    #[test]
    fn keys_we_do_not_use_are_ignored() {
        for code in [KeyCode::A, KeyCode::F1] {
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
            disks: crate::disks::sample(),
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
            disks: crate::disks::sample(),
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
            disks: crate::disks::sample(),
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

    fn point(x: i32, y: i32) -> InputEvent {
        InputEvent::PointerMoved {
            position: Point::new(x, y),
        }
    }

    fn click(x: i32, y: i32) -> InputEvent {
        InputEvent::PointerButton {
            button: denise::input::PointerButton::Left,
            state: ElementState::Down,
            position: Point::new(x, y),
            modifiers: denise::input::Modifiers::default(),
        }
    }

    #[test]
    fn pointer_motion_and_clicks_map_to_actions() {
        assert_eq!(
            super::action_for(&point(10, 20), false),
            Some(Action::PointerTo(10, 20))
        );
        assert_eq!(
            super::action_for(&click(30, 40), false),
            Some(Action::ClickAt(30, 40))
        );
    }

    /// A keyboard-only machine must never show a pointer it cannot move.
    #[test]
    fn the_pointer_is_hidden_until_it_actually_moves() {
        let mut a = app();
        assert!(
            a.pointer.is_none(),
            "a pointer exists before anything moved"
        );
        a.act(Action::PointerTo(100, 100));
        assert_eq!(a.pointer, Some((100, 100)));
    }

    #[test]
    fn releasing_the_button_is_not_a_second_click() {
        let release = InputEvent::PointerButton {
            button: denise::input::PointerButton::Left,
            state: ElementState::Up,
            position: Point::new(5, 5),
            modifiers: denise::input::Modifiers::default(),
        };
        assert_eq!(super::action_for(&release, false), None);
    }

    #[test]
    fn clicking_a_row_selects_it_in_one_click() {
        let mut a = app();
        a.act(Action::Advance); // Keyboard
        let chrome = alpymist_ui::chrome::Chrome::for_screen(1280, 800);
        let (left, top, width, height) = chrome.row_rect(2);
        a.act(Action::ClickAt(left + width / 2, top + height / 2));
        assert_eq!(a.cursor(), 2, "the click did not move the selection");
        assert!(
            a.wizard.answers.keyboard.is_some(),
            "the click did not choose"
        );
    }

    #[test]
    fn clicking_the_primary_button_advances() {
        let mut a = app();
        let chrome = alpymist_ui::chrome::Chrome::for_screen(1280, 800);
        let (left, top, width, height) = chrome.next_button;
        a.act(Action::ClickAt(left + width / 2, top + height / 2));
        assert_eq!(
            a.wizard.step(),
            Step::Keyboard,
            "welcome should have advanced"
        );
    }

    #[test]
    fn clicking_back_goes_back() {
        let mut a = app();
        a.act(Action::Advance);
        a.act(Action::Choose);
        a.act(Action::Advance); // Region
        let chrome = alpymist_ui::chrome::Chrome::for_screen(1280, 800);
        let (left, top, width, height) = chrome.back_button;
        a.act(Action::ClickAt(left + width / 2, top + height / 2));
        assert_eq!(a.wizard.step(), Step::Keyboard);
    }

    #[test]
    fn clicking_the_backdrop_changes_nothing() {
        let mut a = app();
        a.act(Action::Advance);
        let before = (a.cursor(), a.wizard.step(), a.wizard.answers.clone());
        a.act(Action::ClickAt(3, 3));
        assert_eq!(
            (a.cursor(), a.wizard.step(), a.wizard.answers.clone()),
            before
        );
    }

    /// A text field takes focus on click but must not be "chosen" — there is
    /// nothing to choose, and firing Choose there would be a no-op at best.
    #[test]
    fn clicking_a_text_field_focuses_it_without_choosing() {
        let mut a = at_account();
        let chrome = alpymist_ui::chrome::Chrome::for_screen(1280, 800);
        let (left, top, width, height) = chrome.row_rect(3); // Confirm password
        a.act(Action::ClickAt(left + width / 2, top + height / 2));
        assert_eq!(a.focused_field(), Some(TextTarget::PasswordConfirm));
    }

    /// A wizard with everything answered, sitting one step before Install.
    fn at_confirm() -> App {
        let answers = Answers {
            keyboard: Some("no".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sdb".into(),
                encrypt: false,
            }),
            disk_confirmed: true,
            username: "andre".into(),
            full_name: "André Biseth".into(),
            password: "a good passphrase".into(),
            password_confirm: "a good passphrase".into(),
            hostname: "alpymist".into(),
            detected_tier: Some(Tier::Lite),
            disks: crate::disks::sample(),
            ..Answers::default()
        };
        let mut a = App::new(answers, 1280, 800);
        while a.wizard.step() != Step::Confirm {
            a.act(Action::Advance);
        }
        a
    }

    #[test]
    fn reaching_the_install_step_starts_the_install() {
        let mut a = at_confirm();
        a.act(Action::Advance);
        assert_eq!(a.wizard.step(), Step::Install);
        // Give the worker a moment; the point is that one exists at all.
        std::thread::sleep(std::time::Duration::from_millis(200));
        a.tick();
        let lines = a.install_lines();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Step ") || l.contains("would run")),
            "the install never reported anything: {lines:?}"
        );
    }

    /// The installer's own default must be the harmless one.
    #[test]
    fn an_app_built_without_saying_otherwise_does_not_touch_a_disk() {
        let mut a = at_confirm();
        a.act(Action::Advance);
        std::thread::sleep(std::time::Duration::from_millis(300));
        a.tick();
        let lines = a.install_lines().join("\n");
        assert!(
            lines.contains("would run"),
            "a default App must dry-run, but it reported: {lines}"
        );
    }

    #[test]
    fn an_install_that_cannot_be_planned_says_why_instead_of_starting() {
        let mut a = at_confirm();
        // Encryption has no plan yet; the wizard should report that, not hang.
        a.wizard.answers.disk = Some(DiskPlan::WholeDisk {
            device: "/dev/sdb".into(),
            encrypt: true,
        });
        a.act(Action::Advance);
        assert!(!a.reported.is_empty(), "no reason was given");
        assert!(a.reported[0].to_lowercase().contains("encryption"));
    }

    #[test]
    fn the_confirm_screen_names_the_disk_that_will_be_erased() {
        let a = at_confirm();
        let text: String = screens::rows(Step::Confirm, &a.wizard.answers)
            .iter()
            .map(|r| r.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("/dev/sdb"), "the target is not named: {text}");
        assert!(
            text.contains("erased"),
            "the consequence is not stated: {text}"
        );
    }

    #[test]
    fn quitting_is_recorded() {
        let mut a = app();
        assert!(!a.quitting);
        a.act(Action::Quit);
        assert!(a.quitting);
    }
}

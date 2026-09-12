//! The interactive installer: input handling and drawing.
//!
//! The navigation model is deliberately keyboard-first. This runs before any
//! desktop exists, on machines whose touchpad may need a driver that is not
//! loaded yet, so arrow keys and Enter have to be enough on their own.

use crate::answers::Answers;
use crate::screens::{self, Row};
use crate::wizard::{Step, Wizard};
use alpymist_ui::backdrop::Backdrop;
use alpymist_ui::chrome::Chrome;
use alpymist_ui::palette::Palette;
use alpymist_ui::render::{
    ButtonStyle, button_ink, button_label_at, colour, paint_backdrop, paint_button, paint_panel,
};
use alpymist_ui::typeface::{self, Typeface};
use denise::geom::Point;
use denise::painter::Pen;
use denise_render::Canvas;

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
            .position(|r| r.selectable && r.chosen)
            .or_else(|| rows.iter().position(|r| r.selectable))
            .unwrap_or(0);
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
    }

    /// Apply an action.
    pub fn act(&mut self, action: Action) {
        match action {
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
            let under_cursor = row.selectable && index == self.cursor;
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
            self.face.draw(
                pen,
                Point::new(chrome.body.0, y),
                px_for(chrome.text_scale),
                &format!("{prefix}{}", row.text),
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
            let (x, y) = button_label_at(rect, label, chrome.text_scale);
            self.face.draw(
                pen,
                Point::new(x, y),
                px_for(chrome.text_scale),
                label,
                colour(button_ink(style, &palette)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, App};
    use crate::answers::Answers;
    use crate::screens;
    use crate::wizard::Step;
    use alpymist_core::Tier;

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
        assert!(rows[a.cursor()].selectable);
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
                        rows[a.cursor()].selectable,
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

    #[test]
    fn quitting_is_recorded() {
        let mut a = app();
        assert!(!a.quitting);
        a.act(Action::Quit);
        assert!(a.quitting);
    }
}

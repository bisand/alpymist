//! The login screen: what it holds, what keys do, and how it is drawn.
//!
//! The same mountains as the splash and the installer, a clock in the sky, and
//! one card low on the screen with the person and a password field. The first
//! thing Alpymist shows after the first boot should look like the thing that
//! installed it.
//!
//! The login itself runs on a thread. PAM deliberately waits a couple of
//! seconds before admitting a password was wrong, and a screen that stops
//! drawing for that long looks like it has crashed.

use crate::login::Outcome;
use crate::users::User;
use alpymist_ui::palette::{Palette, Rgb};
use alpymist_ui::render::{
    ButtonStyle, Scenery, button_ink, colour, new_cursor, paint_button, paint_cursor,
};
use alpymist_ui::typeface::{self, Typeface};
use denise::color::Color;
use denise::geom::{Point, Rect};
use denise::input::{ElementState, InputEvent, KeyCode, Modifiers, PointerButton};
use denise::paint::Paint;
use denise::painter::Pen;
use denise::theme::Theme;
use denise_render::Canvas;
use denise_ui::cursor::Cursor;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// The splash's and installer's seed, so the mountains are the same ones.
pub const SCENE_SEED: u64 = 0x_A1B2_C3D4_E5F6;

/// Carries out one login attempt: `(username, password)` to how it went, with
/// any notices PAM sent. Injected, so the screen can be driven with no greetd.
pub type Authenticator = Arc<dyn Fn(&str, &str) -> (Outcome, Vec<String>) + Send + Sync>;

/// What the machine should do instead of logging in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    /// Restart.
    Restart,
    /// Switch off.
    PowerOff,
}

/// What this screen is for.
///
/// The lock screen is this screen: `alpymist-lock` puts it on an
/// `ext-session-lock-v1` surface with PAM behind it instead of greetd, so
/// getting back into a session looks like starting one. What differs is small
/// and lives here — there is nobody else to choose, and a locked machine is
/// not where restart and power off belong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// Starting a session: greetd, the whole list of accounts, and the footer's
    /// power buttons.
    #[default]
    Login,
    /// Getting back into one that is already running.
    Unlock,
}

/// What an input event asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A character for the password.
    Type(char),
    /// Delete before the caret.
    Backspace,
    /// Delete under the caret.
    Delete,
    /// Caret one left.
    CaretLeft,
    /// Caret one right.
    CaretRight,
    /// Caret to the start.
    CaretHome,
    /// Caret to the end.
    CaretEnd,
    /// The previous person.
    PreviousUser,
    /// The next person.
    NextUser,
    /// Log in.
    Submit,
    /// Clear the password and any message.
    Clear,
    /// Restart or power off.
    Power(Power),
    /// The pointer moved.
    PointerTo(i32, i32),
    /// The primary button went down.
    ClickAt(i32, i32),
}

/// Translate an input event into an action.
#[must_use]
pub fn action_for(event: &InputEvent) -> Option<Action> {
    match event {
        InputEvent::PointerMoved { position } => Some(Action::PointerTo(position.x, position.y)),
        InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ElementState::Down,
            position,
            ..
        } => Some(Action::ClickAt(position.x, position.y)),
        InputEvent::Text { ch } if !ch.is_control() => Some(Action::Type(*ch)),
        InputEvent::Key {
            code,
            state: ElementState::Down,
            modifiers,
            ..
        } => Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Action::Submit,
            KeyCode::Escape => Action::Clear,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Delete => Action::Delete,
            KeyCode::ArrowLeft => Action::CaretLeft,
            KeyCode::ArrowRight => Action::CaretRight,
            KeyCode::Home => Action::CaretHome,
            KeyCode::End => Action::CaretEnd,
            KeyCode::Tab if modifiers.contains(Modifiers::SHIFT) => Action::PreviousUser,
            KeyCode::ArrowUp => Action::PreviousUser,
            KeyCode::ArrowDown | KeyCode::Tab => Action::NextUser,
            KeyCode::F11 => Action::Power(Power::Restart),
            KeyCode::F12 => Action::Power(Power::PowerOff),
            _ => return None,
        }),
        _ => None,
    }
}

/// Where everything sits, for one screen size.
///
/// Pure geometry, so every size down to 640×480 can be checked for overlap
/// without drawing anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Body text height in pixels.
    pub text_px: u16,
    /// The person's name.
    pub name_px: u16,
    /// The clock.
    pub clock_px: u16,
    /// Centre of the clock line, and its top.
    pub clock_at: (i32, i32),
    /// Top of the date line.
    pub date_y: i32,
    /// The card, as `(x, y, width, height)`.
    pub card: (i32, i32, i32, i32),
    /// Top of the name line.
    pub name_y: i32,
    /// The arrows either side of the name, when there is more than one person.
    pub previous: (i32, i32, i32, i32),
    /// See `previous`.
    pub next: (i32, i32, i32, i32),
    /// The password field.
    pub field: (i32, i32, i32, i32),
    /// Top of the message line under the field.
    pub message_y: i32,
    /// The log in button.
    pub button: (i32, i32, i32, i32),
    /// Top of the footer line.
    pub footer_y: i32,
    /// The restart button in the footer.
    pub restart: (i32, i32, i32, i32),
    /// The power off button in the footer.
    pub power_off: (i32, i32, i32, i32),
    /// Distance from the screen edge.
    pub margin: i32,
}

fn to_px(v: i32) -> u16 {
    u16::try_from(v.max(1)).unwrap_or(u16::MAX)
}

impl Layout {
    /// Lay the screen out for `width` × `height`.
    #[must_use]
    pub fn for_screen(width: u32, height: u32) -> Self {
        let w = i32::try_from(width.max(320)).unwrap_or(i32::MAX / 4);
        let h = i32::try_from(height.max(240)).unwrap_or(i32::MAX / 4);

        // Everything scales from the text height, which follows the shorter
        // side: a 1366×768 laptop and a 1024×768 one want the same sizes.
        let text = (w.min(h * 16 / 10) / 64).clamp(13, 30);
        let gap = text;
        let margin = (text * 3 / 2).max(16);

        let clock = text * 4;
        let clock_top = h * 9 / 100;
        let date_y = clock_top + clock + text / 2;

        let card_w = (text * 26).min(w - margin * 2);
        let name = text * 3 / 2;
        let field_h = text * 5 / 2;
        let button_h = text * 5 / 2;
        let card_h = gap + name + gap + field_h + gap / 2 + text + gap + button_h + gap;
        let card_x = (w - card_w) / 2;
        let footer_y = h - margin - text;
        // Low on the screen like the installer's panel, but never into the footer.
        let card_y = (h * 50 / 100)
            .min(footer_y - gap - card_h)
            .max(date_y + text * 2);

        let inner_x = card_x + gap;
        let inner_w = card_w - gap * 2;
        let name_y = card_y + gap;
        let arrow = name + gap / 2;
        let field_y = name_y + name + gap;
        let message_y = field_y + field_h + gap / 2;
        let button_y = message_y + text + gap;
        let button_w = (text * 10).min(inner_w);

        let foot_button_h = text * 2;
        let foot_button_y = footer_y - (foot_button_h - text) / 2;
        let power_w = text * 10;
        let restart_w = text * 9;
        let power_off = (w - margin - power_w, foot_button_y, power_w, foot_button_h);
        let restart = (
            power_off.0 - gap / 2 - restart_w,
            foot_button_y,
            restart_w,
            foot_button_h,
        );

        Self {
            text_px: to_px(text),
            name_px: to_px(name),
            clock_px: to_px(clock),
            clock_at: (w / 2, clock_top),
            date_y,
            card: (card_x, card_y, card_w, card_h),
            name_y,
            previous: (inner_x, name_y - gap / 4, arrow, arrow),
            next: (inner_x + inner_w - arrow, name_y - gap / 4, arrow, arrow),
            field: (inner_x, field_y, inner_w, field_h),
            message_y,
            button: (
                card_x + (card_w - button_w) / 2,
                button_y,
                button_w,
                button_h,
            ),
            footer_y,
            restart,
            power_off,
            margin,
        }
    }
}

fn inside(rect: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= rect.0 && x < rect.0 + rect.2 && y >= rect.1 && y < rect.1 + rect.3
}

fn rect(r: (i32, i32, i32, i32)) -> Rect {
    Rect::new(r.0, r.1, r.2, r.3)
}

fn alpha(c: Rgb, a: u8) -> Color {
    Color::rgba(c.r, c.g, c.b, a)
}

/// What the line under the password field says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing to say.
    Idle,
    /// A login is being checked.
    Checking,
    /// Something to know, not a problem.
    Notice(String),
    /// Why the last attempt did not work.
    Problem(String),
}

/// The login screen.
pub struct App {
    users: Vec<User>,
    selected: usize,
    password: String,
    /// In characters, never bytes: passwords contain `æøå`.
    caret: usize,
    /// What the line under the field says.
    pub status: Status,
    /// This machine's name, for the footer.
    pub hostname: String,
    /// The keyboard layout keys are read with, for the footer.
    pub keyboard: String,
    clock: String,
    date: String,
    authenticate: Authenticator,
    pending: Option<Receiver<(Outcome, Vec<String>)>>,
    /// Set once greetd has accepted a session: the greeter should exit.
    pub started: bool,
    /// Logging in, or unlocking.
    pub purpose: Purpose,
    power: Option<Power>,
    palette: Palette,
    scenery: Scenery,
    layout: Layout,
    size: (u32, u32),
    /// Fira Mono, or the built-in bitmap without it.
    pub face: Typeface,
    pointer: Cursor,
    theme: Theme,
    /// Where the pointer was drawn in the last frame, so the next one can say
    /// that that is one of the places the picture changed.
    painted_pointer: Option<Rect>,
    /// Whether the clock reads differently from the frame on screen.
    clock_moved: bool,
    /// Whether the scene was composed again, which changes every pixel.
    recomposed: bool,
}

impl App {
    /// A login screen for `users`, checking passwords with `authenticate`.
    #[must_use]
    pub fn new(users: Vec<User>, authenticate: Authenticator, width: u32, height: u32) -> Self {
        let palette = Palette::alpymist();
        let status = if users.is_empty() {
            Status::Problem("There is no account to log in to.".into())
        } else {
            Status::Idle
        };
        Self {
            users,
            selected: 0,
            password: String::new(),
            caret: 0,
            status,
            hostname: String::new(),
            keyboard: String::new(),
            clock: String::new(),
            date: String::new(),
            authenticate,
            pending: None,
            started: false,
            purpose: Purpose::Login,
            power: None,
            scenery: Scenery::compose(width, height, &palette, SCENE_SEED),
            layout: Layout::for_screen(width, height),
            palette,
            size: (width, height),
            face: typeface::load(),
            pointer: new_cursor(),
            theme: denise::theme::DARK,
            painted_pointer: None,
            clock_moved: true,
            recomposed: true,
        }
    }

    /// Put `user` first, when there is such an account: whoever logged in
    /// last is who is most likely to log in next.
    pub fn select(&mut self, user: &str) {
        if let Some(index) = self.users.iter().position(|u| u.name == user) {
            self.selected = index;
        }
    }

    /// The person the password is for.
    #[must_use]
    pub fn user(&self) -> Option<&User> {
        self.users.get(self.selected)
    }

    /// Whether a login is being checked.
    #[must_use]
    pub fn checking(&self) -> bool {
        self.pending.is_some()
    }

    /// Set what the clock says. Returns whether it changed, so the caller
    /// only redraws once a minute.
    pub fn set_time(&mut self, clock: String, date: String) -> bool {
        let changed = clock != self.clock || date != self.date;
        self.clock = clock;
        self.date = date;
        self.clock_moved |= changed;
        changed
    }

    /// A restart or power off that was asked for, once.
    pub fn take_power(&mut self) -> Option<Power> {
        self.power.take()
    }

    /// Whether this screen draws the pointer itself.
    ///
    /// The login screen does: it runs on DRM with no compositor, and nothing
    /// else would. The lock screen must not — the compositor draws one, and a
    /// second cursor chasing it would be both wrong to look at and expensive
    /// to keep up with, since following a pointer means a new frame for every
    /// motion event and a screen's worth of compositing behind each one.
    fn draws_pointer(&self) -> bool {
        self.purpose == Purpose::Login
    }

    /// Do what an action asks. Returns whether the screen needs redrawing.
    pub fn act(&mut self, action: Action) -> bool {
        if let Action::PointerTo(x, y) = action {
            if !self.draws_pointer() {
                return false;
            }
            self.pointer.position = Point::new(x, y);
            self.pointer.visible = true;
            return true;
        }
        if let Action::ClickAt(x, y) = action {
            let moved = self.draws_pointer();
            if moved {
                self.pointer.position = Point::new(x, y);
                self.pointer.visible = true;
            }
            return match self.hit(x, y) {
                Some(action) => self.act(action),
                None => moved,
            };
        }
        // Nothing changes under a login in progress: the password being
        // checked is the one on screen.
        if self.checking() {
            return false;
        }
        match action {
            Action::Type(ch) => {
                let at = byte_at(&self.password, self.caret);
                self.password.insert(at, ch);
                self.caret += 1;
                if matches!(self.status, Status::Problem(_)) && !self.users.is_empty() {
                    self.status = Status::Idle;
                }
            }
            Action::Backspace if self.caret > 0 => {
                let from = byte_at(&self.password, self.caret - 1);
                let to = byte_at(&self.password, self.caret);
                self.password.replace_range(from..to, "");
                self.caret -= 1;
            }
            Action::Delete if self.caret < self.password.chars().count() => {
                let from = byte_at(&self.password, self.caret);
                let to = byte_at(&self.password, self.caret + 1);
                self.password.replace_range(from..to, "");
            }
            Action::CaretLeft => self.caret = self.caret.saturating_sub(1),
            Action::CaretRight => {
                self.caret = (self.caret + 1).min(self.password.chars().count());
            }
            Action::CaretHome => self.caret = 0,
            Action::CaretEnd => self.caret = self.password.chars().count(),
            Action::PreviousUser | Action::NextUser if self.users.len() > 1 => {
                let n = self.users.len();
                self.selected = if action == Action::NextUser {
                    (self.selected + 1) % n
                } else {
                    (self.selected + n - 1) % n
                };
                self.clear();
            }
            Action::Clear => self.clear(),
            Action::Submit => self.submit(),
            // A locked machine offers neither, so neither can be asked for:
            // the keys are not bound and the buttons are not drawn.
            Action::Power(power) if self.purpose == Purpose::Login => self.power = Some(power),
            _ => return false,
        }
        true
    }

    fn clear(&mut self) {
        self.password.clear();
        self.caret = 0;
        if !self.users.is_empty() {
            self.status = Status::Idle;
        }
    }

    fn hit(&self, x: i32, y: i32) -> Option<Action> {
        let l = &self.layout;
        let powers = self.purpose == Purpose::Login;
        if inside(l.button, x, y) {
            Some(Action::Submit)
        } else if powers && inside(l.restart, x, y) {
            Some(Action::Power(Power::Restart))
        } else if powers && inside(l.power_off, x, y) {
            Some(Action::Power(Power::PowerOff))
        } else if self.users.len() > 1 && inside(l.previous, x, y) {
            Some(Action::PreviousUser)
        } else if self.users.len() > 1 && inside(l.next, x, y) {
            Some(Action::NextUser)
        } else {
            None
        }
    }

    fn submit(&mut self) {
        let Some(user) = self.user() else {
            return;
        };
        let name = user.name.clone();
        let password = self.password.clone();
        let authenticate = Arc::clone(&self.authenticate);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(authenticate(&name, &password));
        });
        self.pending = Some(rx);
        self.status = Status::Checking;
    }

    /// Collect the result of a login in progress. Returns whether anything
    /// changed.
    pub fn tick(&mut self) -> bool {
        let Some(rx) = self.pending.as_ref() else {
            return false;
        };
        let (outcome, notices) = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => (
                Outcome::Failed("The login check stopped without an answer.".into()),
                Vec::new(),
            ),
        };
        self.pending = None;
        match outcome {
            Outcome::Started => {
                self.started = true;
                self.status = Status::Notice("Starting your desktop".into());
            }
            Outcome::Rejected(why) | Outcome::Failed(why) => {
                self.password.clear();
                self.caret = 0;
                self.status = Status::Problem(why);
            }
        }
        if let (Status::Idle, Some(notice)) = (&self.status, notices.last()) {
            self.status = Status::Notice(notice.clone());
        }
        true
    }

    /// Recompose for a new screen size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            self.scenery = Scenery::compose(width, height, &self.palette, SCENE_SEED);
            self.layout = Layout::for_screen(width, height);
            self.size = (width, height);
            self.recomposed = true;
        }
    }

    /// Draw the whole screen, and say what part of it may differ from the
    /// frame before.
    ///
    /// Everything is painted every time — the whole picture costs a fraction
    /// of a millisecond, and the parts that never change are a copy. What the
    /// rectangle is for is the *compositor*: telling it that a screen's worth
    /// of pixels changed makes it upload and composite a screen's worth, which
    /// on a machine drawing in software is tens of milliseconds a
    /// keystroke — far more than the drawing it is reporting.
    ///
    /// Outside that rectangle this frame is the last frame, so a caller may
    /// pass it straight to `wl_surface.damage_buffer` — as long as the memory
    /// painted into holds the whole picture, which it does here. A caller
    /// drawing into a buffer that does *not* already hold the last frame must
    /// ignore this and take everything.
    pub fn draw(&mut self, canvas: &mut Canvas<'_>) -> Rect {
        let size = canvas.size();
        self.resize(size.width, size.height);
        let l = self.layout;
        let p = self.palette;

        self.scenery.paint_onto(canvas);

        // The card: the installer's panel, smaller.
        let radius = i32::from(l.text_px) / 2;
        canvas.fill_rounded_rect(rect(l.card), radius, Paint::new(alpha(p.sky_high, 216)));
        canvas.stroke_rounded_rect(rect(l.card), radius, 1, Paint::new(alpha(p.mist, 64)));

        // The field, ringed in the accent: there is only ever one place to type.
        let field_radius = l.field.3 / 4;
        canvas.fill_rounded_rect(
            rect(l.field),
            field_radius,
            Paint::new(alpha(p.ridge_near, 150)),
        );
        let ring = if self.checking() { p.mist } else { p.accent };
        canvas.stroke_rounded_rect(rect(l.field), field_radius, 1, Paint::new(alpha(ring, 170)));

        let button_style = if self.checking() || self.users.is_empty() {
            ButtonStyle::Disabled
        } else {
            ButtonStyle::Primary
        };
        paint_button(canvas, l.button, button_style, &p);
        if self.purpose == Purpose::Login {
            paint_button(canvas, l.restart, ButtonStyle::Quiet, &p);
            paint_button(canvas, l.power_off, ButtonStyle::Quiet, &p);
        }

        let mut pen = Pen::new(canvas);
        self.draw_sky_text(&mut pen);
        self.draw_card_text(&mut pen, button_style);
        self.draw_footer(&mut pen);
        paint_cursor(&mut pen, &self.pointer, &self.theme);

        self.damage(size)
    }

    /// The pointer's rectangle, when it is being drawn.
    fn pointer_rect(&self) -> Option<Rect> {
        if !self.pointer.visible {
            return None;
        }
        let sprite = self.pointer.image;
        let at = self.pointer.position;
        Some(Rect::new(
            at.x - sprite.hotspot.x,
            at.y - sprite.hotspot.y,
            sprite.width,
            sprite.height,
        ))
    }

    /// What the frame just painted may differ from the one before it.
    ///
    /// Three things move: the card, which holds everything anybody types or is
    /// told; the clock, once a minute; and the pointer, which has to take its
    /// old place with it or leave a copy of itself behind.
    fn damage(&mut self, size: denise::geom::Size) -> Rect {
        let l = self.layout;
        let whole = Rect::new(
            0,
            0,
            i32::try_from(size.width).unwrap_or(i32::MAX),
            i32::try_from(size.height).unwrap_or(i32::MAX),
        );
        let pointer = self.pointer_rect();
        let was = self.painted_pointer;
        self.painted_pointer = pointer;
        let clock_moved = std::mem::replace(&mut self.clock_moved, false);
        if std::mem::replace(&mut self.recomposed, false) {
            return whole;
        }
        let mut region = rect(l.card);
        if clock_moved {
            // The whole band the clock and the date are centred in: where
            // their text starts depends on how wide it is.
            let top = l.clock_at.1;
            let bottom = l.date_y + i32::from(l.text_px);
            region = region.union(&Rect::new(0, top, whole.width, bottom - top));
        }
        for sprite in [was, pointer].into_iter().flatten() {
            region = region.union(&sprite);
        }
        region.intersect(&whole).unwrap_or(whole)
    }

    fn centred(&mut self, pen: &mut Pen<'_>, centre_x: i32, y: i32, px: u16, text: &str, ink: Rgb) {
        let width = i32::try_from(self.face.measure(px, text).width).unwrap_or(0);
        self.face.draw(
            pen,
            Point::new(centre_x - width / 2, y),
            px,
            text,
            colour(ink),
        );
    }

    fn draw_sky_text(&mut self, pen: &mut Pen<'_>) {
        let l = self.layout;
        let p = self.palette;
        let clock = self.clock.clone();
        let date = self.date.clone();
        self.centred(pen, l.clock_at.0, l.clock_at.1, l.clock_px, &clock, p.ink);
        self.centred(pen, l.clock_at.0, l.date_y, l.text_px, &date, p.ink_dim);
    }

    fn draw_card_text(&mut self, pen: &mut Pen<'_>, button_style: ButtonStyle) {
        let l = self.layout;
        let p = self.palette;
        let centre = l.card.0 + l.card.2 / 2;

        let name = self
            .user()
            .map_or_else(|| "No accounts".to_string(), |u| u.display.clone());
        self.centred(pen, centre, l.name_y, l.name_px, &name, p.ink);
        if self.users.len() > 1 {
            for (r, arrow) in [(l.previous, "<"), (l.next, ">")] {
                let at = self.face.centre_in(r, l.name_px, arrow);
                self.face.draw(pen, at, l.name_px, arrow, colour(p.ink_dim));
            }
        }

        // The password, as dots, with a caret where the next character goes.
        let inset = i32::from(l.text_px);
        let text_y = l.field.1 + (l.field.3 - i32::from(l.text_px)) / 2;
        let dot = if self.face.status.is_loaded() {
            "•"
        } else {
            "*"
        };
        let count = self.password.chars().count();
        if count == 0 && !self.checking() {
            self.face.draw(
                pen,
                Point::new(l.field.0 + inset, text_y),
                l.text_px,
                "Password",
                colour(p.ink_dim.mix(p.sky_high, 40)),
            );
        }
        // Spaced dots, only as many as fit; the caret stays in view at the end.
        let one = i32::try_from(self.face.measure(l.text_px, dot).width)
            .unwrap_or(1)
            .max(1);
        let step = one + i32::from(l.text_px) / 4;
        let room = usize::try_from((l.field.2 - inset * 2) / step).unwrap_or(0);
        let shown = count.min(room);
        for i in 0..shown {
            let x = l.field.0 + inset + step * i32::try_from(i).unwrap_or(0);
            self.face
                .draw(pen, Point::new(x, text_y), l.text_px, dot, colour(p.ink));
        }
        if !self.checking() {
            let before = self.caret.min(count).saturating_sub(count - shown);
            let x = l.field.0 + inset + step * i32::try_from(before).unwrap_or(0) - step / 8;
            pen.fill_rect(
                Rect::new(x, text_y, 2, i32::from(l.text_px)),
                colour(p.accent),
            );
        }

        let (message, ink) = match &self.status {
            Status::Idle => (String::new(), p.ink_dim),
            Status::Checking => ("Checking".to_string(), p.ink_dim),
            Status::Notice(text) => (text.clone(), p.ink_dim),
            Status::Problem(text) => (text.clone(), p.accent),
        };
        if !message.is_empty() {
            let message = self.fit(&message, l.card.2 - inset * 2, l.text_px);
            self.centred(pen, centre, l.message_y, l.text_px, &message, ink);
        }

        let label = match (self.checking(), self.purpose) {
            (true, _) => "Checking",
            (false, Purpose::Login) => "Enter  Log in",
            (false, Purpose::Unlock) => "Enter  Unlock",
        };
        let at = self.face.centre_in(l.button, l.text_px, label);
        self.face.draw(
            pen,
            at,
            l.text_px,
            label,
            colour(button_ink(button_style, &p)),
        );
    }

    /// `text`, shortened with an ellipsis until it is no wider than `width`.
    fn fit(&mut self, text: &str, width: i32, px: u16) -> String {
        let fits = |face: &mut Typeface, s: &str| {
            i32::try_from(face.measure(px, s).width).unwrap_or(i32::MAX) <= width
        };
        if fits(&mut self.face, text) {
            return text.to_string();
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate = format!("{}...", chars.iter().collect::<String>().trim_end());
            if fits(&mut self.face, &candidate) {
                return candidate;
            }
        }
        String::new()
    }

    fn draw_footer(&mut self, pen: &mut Pen<'_>) {
        let l = self.layout;
        let p = self.palette;
        let mut left = self.hostname.clone();
        if !self.keyboard.is_empty() {
            left = format!("{left}   Keyboard: {}", self.keyboard);
        }
        self.face.draw(
            pen,
            Point::new(l.margin, l.footer_y),
            l.text_px,
            left.trim(),
            colour(p.ink_dim),
        );
        if self.purpose == Purpose::Login {
            for (r, label) in [(l.restart, "F11  Restart"), (l.power_off, "F12  Power off")] {
                let at = self.face.centre_in(r, l.text_px, label);
                self.face.draw(
                    pen,
                    at,
                    l.text_px,
                    label,
                    colour(button_ink(ButtonStyle::Quiet, &p)),
                );
            }
        }
    }
}

/// Where character `index` starts, in bytes; the end for an index past it.
fn byte_at(value: &str, index: usize) -> usize {
    value
        .char_indices()
        .nth(index)
        .map_or(value.len(), |(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::{Action, App, Authenticator, Layout, Power, Purpose, Status, action_for};
    use crate::login::Outcome;
    use crate::users::User;
    use denise::input::{ElementState, InputEvent, KeyCode, Modifiers};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn users(names: &[&str]) -> Vec<User> {
        names
            .iter()
            .zip(1000..)
            .map(|(n, uid)| User {
                name: (*n).to_string(),
                display: (*n).to_string(),
                uid,
            })
            .collect()
    }

    /// Accepts "right" and records who it was asked about.
    fn checker(asked: Asked) -> Authenticator {
        Arc::new(move |user: &str, password: &str| {
            asked
                .lock()
                .unwrap()
                .push((user.to_string(), password.to_string()));
            if password == "right" {
                (Outcome::Started, Vec::new())
            } else {
                (
                    Outcome::Rejected("That password is not right.".into()),
                    Vec::new(),
                )
            }
        })
    }

    type Asked = Arc<Mutex<Vec<(String, String)>>>;

    fn app(names: &[&str]) -> (App, Asked) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        (
            App::new(users(names), checker(Arc::clone(&asked)), 1280, 800),
            asked,
        )
    }

    fn type_text(app: &mut App, text: &str) {
        for ch in text.chars() {
            app.act(Action::Type(ch));
        }
    }

    fn settle(app: &mut App) {
        let until = Instant::now() + Duration::from_secs(5);
        while app.checking() && Instant::now() < until {
            app.tick();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn the_right_password_starts_the_session_for_the_chosen_person() {
        let (mut a, asked) = app(&["andre", "kid"]);
        a.act(Action::NextUser);
        type_text(&mut a, "right");
        a.act(Action::Submit);
        settle(&mut a);
        assert!(a.started);
        assert_eq!(*asked.lock().unwrap(), [("kid".into(), "right".into())]);
    }

    #[test]
    fn a_wrong_password_is_cleared_and_explained() {
        let (mut a, _) = app(&["andre"]);
        type_text(&mut a, "wrong");
        a.act(Action::Submit);
        settle(&mut a);
        assert!(!a.started);
        assert!(matches!(a.status, Status::Problem(_)));
        assert!(
            a.password.is_empty(),
            "the next try starts from an empty field"
        );
        type_text(&mut a, "r");
        assert_eq!(a.status, Status::Idle, "typing again dismisses the problem");
    }

    #[test]
    fn nothing_can_be_typed_while_a_login_is_being_checked() {
        let gate = Arc::new(Mutex::new(()));
        let held = gate.lock().unwrap();
        let waiting = Arc::clone(&gate);
        let slow: Authenticator = Arc::new(move |_: &str, _: &str| {
            let _wait = waiting.lock();
            (Outcome::Started, Vec::new())
        });
        let mut a = App::new(users(&["andre"]), slow, 1280, 800);
        type_text(&mut a, "right");
        a.act(Action::Submit);
        assert!(a.checking());
        assert!(!a.act(Action::Type('x')));
        assert!(!a.act(Action::Submit));
        assert_eq!(a.password, "right");
        drop(held);
        settle(&mut a);
        assert!(a.started);
    }

    /// A caret counted in bytes would split `ø` and panic on the next insert.
    #[test]
    fn passwords_with_non_ascii_letters_edit_by_character() {
        let (mut a, _) = app(&["andre"]);
        type_text(&mut a, "bløt");
        a.act(Action::CaretLeft);
        a.act(Action::Backspace);
        a.act(Action::Type('o'));
        assert_eq!(a.password, "blot");
        a.act(Action::CaretHome);
        a.act(Action::Delete);
        assert_eq!(a.password, "lot");
    }

    #[test]
    fn changing_person_clears_the_password() {
        let (mut a, _) = app(&["andre", "kid"]);
        type_text(&mut a, "secret");
        a.act(Action::PreviousUser);
        assert_eq!(a.user().unwrap().name, "kid", "wraps around");
        assert!(a.password.is_empty());
    }

    #[test]
    fn the_last_person_to_log_in_is_offered_first() {
        let (mut a, _) = app(&["andre", "kid"]);
        a.select("kid");
        assert_eq!(a.user().unwrap().name, "kid");
        a.select("nobody");
        assert_eq!(
            a.user().unwrap().name,
            "kid",
            "an unknown name changes nothing"
        );
    }

    #[test]
    fn a_machine_with_no_accounts_says_so_and_submits_nothing() {
        let (mut a, asked) = app(&[]);
        assert!(matches!(a.status, Status::Problem(_)));
        a.act(Action::Submit);
        assert!(!a.checking());
        assert!(asked.lock().unwrap().is_empty());
    }

    #[test]
    fn clicking_the_buttons_does_what_they_say() {
        let (mut a, _) = app(&["andre"]);
        let l = Layout::for_screen(1280, 800);
        let centre = |r: (i32, i32, i32, i32)| (r.0 + r.2 / 2, r.1 + r.3 / 2);
        let (x, y) = centre(l.power_off);
        a.act(Action::ClickAt(x, y));
        assert_eq!(a.take_power(), Some(Power::PowerOff));
        assert_eq!(a.take_power(), None, "asked for once, carried out once");
        let (x, y) = centre(l.button);
        a.act(Action::ClickAt(x, y));
        assert!(a.checking());
    }

    #[test]
    fn keys_map_to_what_the_footer_promises() {
        let key = |code| InputEvent::Key {
            code,
            state: ElementState::Down,
            modifiers: Modifiers::NONE,
            repeat: false,
        };
        assert_eq!(action_for(&key(KeyCode::Enter)), Some(Action::Submit));
        assert_eq!(
            action_for(&key(KeyCode::F11)),
            Some(Action::Power(Power::Restart))
        );
        assert_eq!(
            action_for(&key(KeyCode::F12)),
            Some(Action::Power(Power::PowerOff))
        );
        assert_eq!(
            action_for(&InputEvent::Text { ch: '\u{8}' }),
            None,
            "control characters are keys, not password text"
        );
    }

    #[test]
    fn a_locked_screen_does_not_offer_to_restart_or_switch_off() {
        let (mut a, _) = app(&["andre"]);
        a.purpose = Purpose::Unlock;
        let l = a.layout;
        for r in [l.restart, l.power_off] {
            a.act(Action::ClickAt(r.0 + r.2 / 2, r.1 + r.3 / 2));
            assert!(a.take_power().is_none(), "a footer button still there");
        }
        a.act(Action::Power(Power::PowerOff));
        assert!(a.take_power().is_none(), "not even when asked directly");
    }

    #[test]
    fn logging_in_still_does() {
        let (mut a, _) = app(&["andre"]);
        let l = a.layout;
        a.act(Action::ClickAt(
            l.power_off.0 + l.power_off.2 / 2,
            l.power_off.1 + l.power_off.3 / 2,
        ));
        assert_eq!(a.take_power(), Some(Power::PowerOff));
    }

    fn contains(outer: (i32, i32, i32, i32), inner: (i32, i32, i32, i32)) -> bool {
        inner.0 >= outer.0
            && inner.1 >= outer.1
            && inner.0 + inner.2 <= outer.0 + outer.2
            && inner.1 + inner.3 <= outer.1 + outer.3
    }

    fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn everything_fits_on_every_screen_size_we_support() {
        for (w, h) in [
            (640, 480),
            (800, 600),
            (1024, 600),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
        ] {
            let l = Layout::for_screen(w, h);
            let screen = (0, 0, i32::try_from(w).unwrap(), i32::try_from(h).unwrap());
            for (name, r) in [
                ("card", l.card),
                ("restart", l.restart),
                ("power off", l.power_off),
            ] {
                assert!(contains(screen, r), "{w}x{h}: {name} off screen: {r:?}");
            }
            for (name, r) in [("field", l.field), ("button", l.button)] {
                assert!(contains(l.card, r), "{w}x{h}: {name} outside the card");
            }
            assert!(!overlaps(l.field, l.button), "{w}x{h}: field and button");
            assert!(!overlaps(l.restart, l.power_off), "{w}x{h}: footer buttons");
            let date_bottom = l.date_y + i32::from(l.text_px);
            assert!(
                date_bottom < l.card.1,
                "{w}x{h}: the date runs into the card"
            );
            assert!(
                l.card.1 + l.card.3 < l.restart.1,
                "{w}x{h}: the card runs into the footer"
            );
        }
    }
}

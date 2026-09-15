//! The dialog's state, and what every key and click does to it. No pixels.
//!
//! The dialog shows what is to be done and whose password would do, and
//! asks for the password in whatever words PAM uses. Enter hands it to the
//! helper; while PAM thinks — a wrong password makes it wait a couple of
//! seconds on purpose — the dialog says so and takes no more typing. A wrong
//! password clears the field and asks again, up to [`ATTEMPTS`] times.

use crate::helper::Step;
use crate::request::Request;
use crate::secret::Secret;
use alpymist_widget::Key;

/// How many wrong passwords before the dialog gives up.
pub const ATTEMPTS: u32 = 3;

/// Where the conversation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The helper has not asked yet.
    Starting,
    /// The helper asked; the field takes typing.
    Asking,
    /// An answer is with PAM.
    Checking,
}

/// Which control has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The password field.
    Field,
    /// Cancel.
    Cancel,
    /// Authenticate.
    Authenticate,
}

/// Something the pointer can be over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The password field.
    Field,
    /// Cancel.
    Cancel,
    /// Authenticate.
    Authenticate,
    /// The account before, or after, the one asked for.
    Identity(i8),
}

/// What the dialog wants done.
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Hand this answer to the helper.
    Answer(Secret),
    /// Start a helper for the account at this index: the first time, after
    /// a failure, or when the account was changed.
    Start(usize),
    /// Close: authorised, or not.
    Close(bool),
}

/// The dialog.
pub struct Prompt {
    /// What is being authorised.
    pub request: Request,
    /// The account asked for, an index into the request's identities.
    pub identity: usize,
    /// What PAM asks for, without its colon: "Password".
    pub label: String,
    /// Whether the answer may be shown as typed.
    pub echo: bool,
    /// What has been typed.
    pub secret: Secret,
    /// Where the conversation is.
    pub phase: Phase,
    /// What went wrong last.
    pub error: Option<String>,
    /// What PAM last said besides.
    pub info: Option<String>,
    /// Wrong answers so far.
    pub failures: u32,
    /// Whether Caps Lock is on.
    pub caps_lock: bool,
    /// Which control has the keyboard.
    pub focus: Focus,
    /// What the pointer is over.
    pub hover: Option<Target>,
    /// Animation frames, while checking.
    pub frame: u32,
}

impl Prompt {
    /// The dialog for `request`, asking first for account `identity`.
    #[must_use]
    pub fn new(request: Request, identity: usize) -> Self {
        Self {
            request,
            identity,
            label: "Password".into(),
            echo: false,
            secret: Secret::new(),
            phase: Phase::Starting,
            error: None,
            info: None,
            failures: 0,
            caps_lock: false,
            focus: Focus::Field,
            hover: None,
            frame: 0,
        }
    }

    /// The helper said something.
    pub fn step(&mut self, step: Step) -> Outcome {
        match step {
            Step::Prompt { text, echo } => {
                let text = text.trim().trim_end_matches(':').trim();
                self.label = if text.is_empty() {
                    "Password".into()
                } else {
                    text.to_owned()
                };
                self.echo = echo;
                self.phase = Phase::Asking;
                self.secret.clear();
                self.focus = Focus::Field;
            }
            Step::Error(text) => self.error = Some(text),
            Step::Info(text) => self.info = Some(text),
            Step::Done(true) => return Outcome::Close(true),
            Step::Done(false) => {
                self.secret.clear();
                self.failures += 1;
                if self.failures >= ATTEMPTS {
                    return Outcome::Close(false);
                }
                self.error = Some(match ATTEMPTS - self.failures {
                    1 => "That password was not accepted. One more try.".into(),
                    _ => "That password was not accepted. Try again.".into(),
                });
                self.phase = Phase::Starting;
                return Outcome::Start(self.identity);
            }
        }
        Outcome::Redraw
    }

    /// The helper could not be run at all.
    pub fn broken(&mut self, why: String) -> Outcome {
        self.error = Some(why);
        self.phase = Phase::Starting;
        Outcome::Redraw
    }

    /// Whether anything is moving.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.phase == Phase::Checking
    }

    fn submit(&mut self) -> Outcome {
        if self.phase != Phase::Asking || self.secret.is_empty() {
            return Outcome::Unchanged;
        }
        self.phase = Phase::Checking;
        self.error = None;
        self.info = None;
        let answer = std::mem::take(&mut self.secret);
        Outcome::Answer(answer)
    }

    fn switch_identity(&mut self, by: i32) -> Outcome {
        let n = self.request.identities.len();
        if n < 2 || self.phase == Phase::Checking {
            return Outcome::Unchanged;
        }
        let n = i32::try_from(n).unwrap_or(1);
        let at = i32::try_from(self.identity).unwrap_or(0);
        self.identity = usize::try_from((at + by).rem_euclid(n)).unwrap_or(0);
        self.secret.clear();
        self.error = None;
        self.failures = 0;
        self.phase = Phase::Starting;
        Outcome::Start(self.identity)
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Escape => Outcome::Close(false),
            Key::Enter => match self.focus {
                Focus::Cancel => Outcome::Close(false),
                Focus::Field | Focus::Authenticate => self.submit(),
            },
            Key::Space if self.focus == Focus::Cancel => Outcome::Close(false),
            Key::Space if self.focus == Focus::Authenticate => self.submit(),
            Key::Space => self.text(' '),
            Key::Tab | Key::Down => self.move_focus(1),
            Key::BackTab | Key::Up => self.move_focus(-1),
            Key::Left => self.switch_identity(-1),
            Key::Right => self.switch_identity(1),
            Key::Backspace if self.phase == Phase::Asking => {
                self.secret.pop();
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            Key::Clear if self.phase == Phase::Asking => {
                self.secret.clear();
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            _ => Outcome::Unchanged,
        }
    }

    fn move_focus(&mut self, by: i32) -> Outcome {
        const ORDER: [Focus; 3] = [Focus::Field, Focus::Cancel, Focus::Authenticate];
        let at = ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        let at = i32::try_from(at).unwrap_or(0);
        self.focus = ORDER[usize::try_from((at + by).rem_euclid(3)).unwrap_or(0)];
        Outcome::Redraw
    }

    /// A character was typed: into the field, wherever the focus was.
    pub fn text(&mut self, ch: char) -> Outcome {
        if self.phase != Phase::Asking || ch.is_control() {
            return Outcome::Unchanged;
        }
        self.focus = Focus::Field;
        self.error = None;
        if self.secret.push(ch) {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// Caps Lock went on or off.
    pub fn set_caps_lock(&mut self, on: bool) -> Outcome {
        let changed = on != self.caps_lock;
        self.caps_lock = on;
        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// The pointer moved over `target`, or off everything.
    pub fn hover_over(&mut self, target: Option<Target>) -> Outcome {
        let changed = target != self.hover;
        self.hover = target;
        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// `target` was clicked.
    pub fn click(&mut self, target: Target) -> Outcome {
        match target {
            Target::Field => {
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            Target::Cancel => Outcome::Close(false),
            Target::Authenticate => self.submit(),
            Target::Identity(by) => self.switch_identity(i32::from(by)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ATTEMPTS, Focus, Outcome, Phase, Prompt};
    use crate::helper::Step;
    use crate::request::{Identity, Request};
    use alpymist_widget::Key;

    fn prompt() -> Prompt {
        let who = |uid, name: &str| Identity {
            uid,
            name: name.into(),
            full_name: String::new(),
        };
        Prompt::new(
            Request {
                identities: vec![who(1000, "andre"), who(1001, "kari")],
                ..Request::default()
            },
            0,
        )
    }

    fn asked(p: &mut Prompt) {
        p.step(Step::Prompt {
            text: "Password: ".into(),
            echo: false,
        });
    }

    #[test]
    fn nothing_is_typed_until_pam_asks_and_nothing_empty_is_sent() {
        let mut p = prompt();
        assert!(matches!(p.text('x'), Outcome::Unchanged));
        asked(&mut p);
        assert_eq!(p.label, "Password");
        assert!(matches!(p.key(Key::Enter), Outcome::Unchanged));
        p.text('o');
        p.text('k');
        let Outcome::Answer(secret) = p.key(Key::Enter) else {
            panic!("the answer goes to the helper");
        };
        assert_eq!(secret.expose(), b"ok");
        assert!(p.secret.is_empty());
        assert_eq!(p.phase, Phase::Checking);
        assert!(
            matches!(p.text('z'), Outcome::Unchanged),
            "no typing while PAM checks"
        );
    }

    #[test]
    fn wrong_passwords_ask_again_then_give_up() {
        let mut p = prompt();
        for n in 1..ATTEMPTS {
            asked(&mut p);
            p.text('x');
            p.key(Key::Enter);
            assert!(
                matches!(p.step(Step::Done(false)), Outcome::Start(0)),
                "attempt {n}"
            );
            assert!(p.error.is_some());
        }
        asked(&mut p);
        p.text('x');
        p.key(Key::Enter);
        assert!(matches!(p.step(Step::Done(false)), Outcome::Close(false)));
        assert!(matches!(
            prompt().step(Step::Done(true)),
            Outcome::Close(true)
        ));
    }

    #[test]
    fn escape_and_cancel_close_without_authorising() {
        let mut p = prompt();
        asked(&mut p);
        p.text('s');
        assert!(matches!(p.key(Key::Escape), Outcome::Close(false)));
        p.key(Key::Tab);
        assert_eq!(p.focus, Focus::Cancel);
        assert!(matches!(p.key(Key::Enter), Outcome::Close(false)));
    }

    #[test]
    fn another_account_starts_over() {
        let mut p = prompt();
        asked(&mut p);
        p.text('s');
        assert!(matches!(p.key(Key::Right), Outcome::Start(1)));
        assert!(p.secret.is_empty());
        assert!(matches!(p.key(Key::Right), Outcome::Start(0)));
    }
}

//! What ssh asks through `SSH_ASKPASS`, and what every key and click does to
//! the dialog. No pixels.
//!
//! ssh, ssh-add and git run the askpass program with one argument, the
//! question in their own words, and read the answer from its standard output.
//! `SSH_ASKPASS_PROMPT` says what kind of question it is: `confirm` wants a
//! yes or a no, given as the exit status (`ssh-add -c`, `AddKeysToAgent
//! confirm`); `none` is only something to see, such as "touch your security
//! key", and ssh ends the program when it no longer needs saying. Anything
//! else wants text typed: a key's passphrase, a PIN, a username, or "yes" to
//! a host key never seen before.
//!
//! The question is shown as ssh put it. Only its shape is read: a quoted
//! name at the end — the key's file, the site git signs in to — goes in a card
//! of its own, and a username or a yes-or-no is shown as it is typed, where a
//! passphrase is dots.

use crate::secret::Secret;
use alpymist_widget::Key;

/// What kind of question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Something to type: the answer is printed.
    Answer,
    /// Yes or no: the answer is the exit status.
    Confirm,
    /// Nothing to answer: shown until dismissed, or until ssh ends it.
    Notice,
}

impl Kind {
    /// The kind `SSH_ASKPASS_PROMPT` names.
    #[must_use]
    pub fn named(value: Option<&str>) -> Self {
        match value {
            Some("confirm") => Self::Confirm,
            Some("none") => Self::Notice,
            _ => Self::Answer,
        }
    }
}

/// Which control has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The field.
    Field,
    /// Cancel, or Deny.
    Cancel,
    /// Unlock, Allow or OK.
    Accept,
}

/// Something the pointer can be over.
pub type Target = Focus;

/// How the dialog closed.
pub enum Answer {
    /// This was typed, and is to be printed.
    Typed(Secret),
    /// Allowed, or seen.
    Yes,
    /// Cancelled or denied.
    No,
}

/// What the dialog wants done.
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Close with this answer.
    Close(Answer),
}

/// The dialog.
// Each flag is something the dialog says: echoing, Caps Lock, checkable,
// checked. They are independent, not a state machine in disguise.
#[allow(clippy::struct_excessive_bools)]
pub struct Askpass {
    /// What kind of question.
    pub kind: Kind,
    /// The title, from what is asked for.
    pub title: &'static str,
    /// The question, as ssh put it, less the name in [`Self::subject`].
    pub question: String,
    /// The key's file or the site, where the question ends with one quoted.
    pub subject: Option<String>,
    /// What the empty field says: "Passphrase", "PIN".
    pub hint: &'static str,
    /// Whether the answer is shown as typed.
    pub echo: bool,
    /// What has been typed.
    pub secret: Secret,
    /// Whether Caps Lock is on.
    pub caps_lock: bool,
    /// Which control has the keyboard.
    pub focus: Focus,
    /// What the pointer is over.
    pub hover: Option<Target>,
    /// Whether the desktop can check this prompt is genuine.
    pub checkable: bool,
    /// Whether Ctrl+Alt+Delete checked it since it opened.
    pub verified: bool,
}

impl Askpass {
    /// The dialog for `question`, of `kind`. `home` is the account's home,
    /// shown as `~`.
    #[must_use]
    pub fn new(kind: Kind, question: &str, home: Option<&str>) -> Self {
        let question = question.trim_end();
        let lower = question.to_lowercase();
        let yes_no = lower.contains("(yes/no");
        let username = lower.starts_with("username");
        let (hint, title) = if lower.contains("passphrase") {
            ("Passphrase", "Unlock an SSH key")
        } else if question.contains("PIN") {
            ("PIN", "Security key PIN")
        } else if username || lower.starts_with("password") {
            (if username { "Username" } else { "Password" }, "Sign in")
        } else if yes_no {
            ("yes or no", "Connect to a new host?")
        } else {
            ("Answer", "SSH is asking")
        };
        let title = match kind {
            Kind::Answer => title,
            Kind::Confirm => "Allow the key to be used?",
            Kind::Notice if lower.contains("touch") || lower.contains("presence") => {
                "Touch your security key"
            }
            Kind::Notice => "SSH says",
        };
        let (question, subject) = split_subject(question, home);
        Self {
            kind,
            title,
            question,
            subject,
            hint,
            echo: username || yes_no,
            secret: Secret::new(),
            caps_lock: false,
            focus: match kind {
                Kind::Answer => Focus::Field,
                // A keystroke meant for somewhere else must not allow a key.
                Kind::Confirm => Focus::Cancel,
                Kind::Notice => Focus::Accept,
            },
            hover: None,
            checkable: false,
            verified: false,
        }
    }

    /// Whether there is a field to type in.
    #[must_use]
    pub fn has_field(&self) -> bool {
        self.kind == Kind::Answer
    }

    /// Cancel's words, where there is a Cancel.
    #[must_use]
    pub fn cancel_label(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Answer => Some("Cancel"),
            Kind::Confirm => Some("Deny"),
            Kind::Notice => None,
        }
    }

    /// The main button's words.
    #[must_use]
    pub fn accept_label(&self) -> &'static str {
        match self.kind {
            Kind::Answer if self.hint == "Passphrase" => "Unlock",
            Kind::Confirm => "Allow",
            Kind::Answer | Kind::Notice => "OK",
        }
    }

    /// Whether the main button would do anything now.
    #[must_use]
    pub fn ready(&self) -> bool {
        !self.has_field() || !self.secret.is_empty()
    }

    /// The controls, in Tab order.
    fn order(&self) -> &'static [Focus] {
        match self.kind {
            Kind::Answer => &[Focus::Field, Focus::Cancel, Focus::Accept],
            Kind::Confirm => &[Focus::Cancel, Focus::Accept],
            Kind::Notice => &[Focus::Accept],
        }
    }

    fn accept(&mut self) -> Outcome {
        match self.kind {
            Kind::Answer if self.secret.is_empty() => Outcome::Unchanged,
            Kind::Answer => Outcome::Close(Answer::Typed(std::mem::take(&mut self.secret))),
            Kind::Confirm | Kind::Notice => Outcome::Close(Answer::Yes),
        }
    }

    fn activate(&mut self) -> Outcome {
        match self.focus {
            Focus::Cancel => Outcome::Close(Answer::No),
            Focus::Field | Focus::Accept => self.accept(),
        }
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Escape => Outcome::Close(Answer::No),
            Key::Enter => self.activate(),
            Key::Space if self.focus != Focus::Field => self.activate(),
            Key::Space => self.text(' '),
            Key::Tab | Key::Down => self.move_focus(1),
            Key::BackTab | Key::Up => self.move_focus(-1),
            Key::Backspace if self.has_field() => {
                self.secret.pop();
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            Key::Clear if self.has_field() => {
                self.secret.clear();
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            _ => Outcome::Unchanged,
        }
    }

    fn move_focus(&mut self, by: i32) -> Outcome {
        let order = self.order();
        let n = i32::try_from(order.len()).unwrap_or(1);
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let at = i32::try_from(at).unwrap_or(0);
        let next = order[usize::try_from((at + by).rem_euclid(n)).unwrap_or(0)];
        let changed = next != self.focus;
        self.focus = next;
        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// A character was typed: into the field, wherever the focus was.
    pub fn text(&mut self, ch: char) -> Outcome {
        if !self.has_field() || ch.is_control() {
            return Outcome::Unchanged;
        }
        self.focus = Focus::Field;
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

    /// Ctrl+Alt+Delete found this prompt genuine.
    pub fn verify(&mut self) -> Outcome {
        let changed = !self.verified;
        self.verified = true;
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
            Focus::Field => {
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            Focus::Cancel => Outcome::Close(Answer::No),
            Focus::Accept => self.accept(),
        }
    }
}

/// `Enter passphrase for key '/home/me/.ssh/id_ed25519':` is the question
/// "Enter passphrase for key" and the subject `~/.ssh/id_ed25519`.
fn split_subject(question: &str, home: Option<&str>) -> (String, Option<String>) {
    let trimmed = question.trim_end_matches(':').trim_end();
    let quoted = trimmed
        .strip_suffix('\'')
        .and_then(|rest| rest.rfind('\'').map(|at| (&rest[..at], &rest[at + 1..])));
    let Some((before, name)) = quoted.filter(|(b, n)| !n.is_empty() && !b.contains('\n')) else {
        return (question.trim_end_matches(':').trim_end().to_owned(), None);
    };
    let name = match home.filter(|h| !h.is_empty()) {
        Some(home) => match name.strip_prefix(home) {
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => name.to_owned(),
        },
        None => name.to_owned(),
    };
    (before.trim_end().to_owned(), Some(name))
}

#[cfg(test)]
mod tests {
    use super::{Answer, Askpass, Focus, Kind, Outcome};
    use alpymist_widget::Key;

    const KEY: &str = "Enter passphrase for key '/home/andre/.ssh/id_ed25519': ";

    #[test]
    fn a_passphrase_is_asked_for_the_key_named_and_printed() {
        let mut a = Askpass::new(Kind::Answer, KEY, Some("/home/andre"));
        assert_eq!(a.title, "Unlock an SSH key");
        assert_eq!(a.question, "Enter passphrase for key");
        assert_eq!(a.subject.as_deref(), Some("~/.ssh/id_ed25519"));
        assert!(!a.echo);
        assert_eq!(a.accept_label(), "Unlock");
        assert!(
            matches!(a.key(Key::Enter), Outcome::Unchanged),
            "nothing empty is sent"
        );
        for ch in "pæss word".chars() {
            a.text(ch);
        }
        let Outcome::Close(Answer::Typed(secret)) = a.key(Key::Enter) else {
            panic!("the passphrase is the answer");
        };
        assert_eq!(secret.expose(), "pæss word".as_bytes());
        assert!(a.secret.is_empty());
    }

    #[test]
    fn a_username_and_a_host_key_answer_are_shown_as_typed() {
        let a = Askpass::new(
            Kind::Answer,
            "Username for 'https://github.com': ",
            Some("/home/andre"),
        );
        assert!(a.echo);
        assert_eq!(a.title, "Sign in");
        assert_eq!(a.subject.as_deref(), Some("https://github.com"));
        assert!(
            !Askpass::new(
                Kind::Answer,
                "Password for 'https://andre@github.com': ",
                None
            )
            .echo
        );
        let host = "The authenticity of host 'vm (192.168.64.8)' can't be established.\n\
                    ED25519 key fingerprint is SHA256:abc.\n\
                    Are you sure you want to continue connecting (yes/no/[fingerprint])? ";
        let a = Askpass::new(Kind::Answer, host, None);
        assert!(a.echo);
        assert_eq!(a.subject, None, "a quote in the middle is not a subject");
        assert!(a.question.ends_with("(yes/no/[fingerprint])?"));
    }

    #[test]
    fn a_confirmation_is_denied_unless_allowed_on_purpose() {
        let mut a = Askpass::new(
            Kind::Confirm,
            "Allow use of key /home/andre/.ssh/id_ed25519?\nKey fingerprint SHA256:abc.",
            None,
        );
        assert!(!a.has_field());
        assert_eq!(a.focus, Focus::Cancel);
        assert!(matches!(a.text('y'), Outcome::Unchanged));
        assert!(matches!(a.key(Key::Enter), Outcome::Close(Answer::No)));
        a.key(Key::Tab);
        assert!(matches!(a.key(Key::Space), Outcome::Close(Answer::Yes)));
        assert!(matches!(a.click(Focus::Cancel), Outcome::Close(Answer::No)));
    }

    #[test]
    fn a_notice_has_only_ok() {
        let mut a = Askpass::new(
            Kind::Notice,
            "Confirm user presence for key ED25519-SK SHA256:abc",
            None,
        );
        assert_eq!(a.title, "Touch your security key");
        assert_eq!(a.cancel_label(), None);
        assert!(matches!(a.key(Key::Tab), Outcome::Unchanged));
        assert!(matches!(a.key(Key::Enter), Outcome::Close(Answer::Yes)));
        assert!(matches!(a.key(Key::Escape), Outcome::Close(Answer::No)));
    }

    #[test]
    fn prompt_kinds_come_from_ssh() {
        assert_eq!(Kind::named(Some("confirm")), Kind::Confirm);
        assert_eq!(Kind::named(Some("none")), Kind::Notice);
        assert_eq!(Kind::named(Some("entry")), Kind::Answer);
        assert_eq!(Kind::named(None), Kind::Answer);
    }
}

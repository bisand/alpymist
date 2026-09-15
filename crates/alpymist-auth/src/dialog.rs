//! The dialog on screen: a layer surface over the whole output, dimmed, with
//! the prompt in the middle, and a thread holding the conversation with
//! polkit's helper.
//!
//! The layer takes the keyboard exclusively, so while the dialog is up no
//! other window receives a keystroke — a password typed a moment after the
//! dialog closes cannot land somewhere it should not. Should the focus be
//! taken anyway, or a click land outside, the dialog cancels rather than
//! wait behind a window for a password nobody is typing.

use alpymist_auth::attention;
use alpymist_auth::helper::{self, Attempt, Step};
use alpymist_auth::prompt::{Outcome, Prompt};
use alpymist_auth::request::{Request, own_uid};
use alpymist_auth::secret::Secret;
use alpymist_auth::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Placement, Sender};
use alpymist_widget::{Appearance, Key, Outcome as AnyOutcome, Widget};
use denise::Frame;
use denise::geom::{Point, Size};
use std::io::Read;
use std::sync::mpsc;

/// What the dialog tells the conversation.
enum Order {
    /// Start an attempt for this account.
    Start(String),
    /// Answer the helper's prompt.
    Answer(Secret),
}

/// What the conversation tells the dialog.
enum Event {
    Step(Step),
    Broken(String),
    /// Ctrl+Alt+Delete found this prompt genuine.
    Verified,
}

struct Dialog {
    appearance: Appearance,
    fonts: Fonts,
    prompt: Prompt,
    layout: Option<Layout>,
    orders: mpsc::Sender<Order>,
    authorised: std::rc::Rc<std::cell::Cell<bool>>,
}

impl Dialog {
    fn apply(&mut self, outcome: Outcome) -> AnyOutcome {
        match outcome {
            Outcome::Unchanged => AnyOutcome::Unchanged,
            Outcome::Redraw => AnyOutcome::Redraw,
            Outcome::Answer(secret) => {
                let _ = self.orders.send(Order::Answer(secret));
                AnyOutcome::Redraw
            }
            Outcome::Start(index) => {
                if let Some(who) = self.prompt.request.identities.get(index) {
                    let _ = self.orders.send(Order::Start(who.name.clone()));
                }
                AnyOutcome::Redraw
            }
            Outcome::Close(authorised) => {
                self.authorised.set(authorised);
                AnyOutcome::Close
            }
        }
    }
}

impl Widget for Dialog {
    type Event = Event;

    fn layout(&mut self, scale: u32) -> Size {
        let layout = Layout::new(&self.appearance, &mut self.fonts, &self.prompt, scale);
        let size = layout.size;
        self.layout = Some(layout);
        size
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if let Some(layout) = &self.layout {
            view::paint(
                frame,
                layout,
                &self.appearance,
                &mut self.fonts,
                &self.prompt,
            );
        }
    }

    fn key(&mut self, key: Key) -> AnyOutcome {
        let outcome = self.prompt.key(key);
        self.apply(outcome)
    }

    fn text(&mut self, ch: char) -> AnyOutcome {
        let outcome = self.prompt.text(ch);
        self.apply(outcome)
    }

    fn pointer(&mut self, at: Option<Point>) -> AnyOutcome {
        let target = at.and_then(|p| self.layout.as_ref().and_then(|l| l.hit(p)));
        let outcome = self.prompt.hover_over(target);
        self.apply(outcome)
    }

    fn press(&mut self, at: Point) -> AnyOutcome {
        let outcome = match self.layout.as_ref().and_then(|l| l.hit(at)) {
            Some(target) => self.prompt.click(target),
            None => Outcome::Unchanged,
        };
        self.apply(outcome)
    }

    fn caps_lock(&mut self, on: bool) -> AnyOutcome {
        let outcome = self.prompt.set_caps_lock(on);
        self.apply(outcome)
    }

    fn animating(&self) -> bool {
        self.prompt.animating()
    }

    fn tick(&mut self) -> AnyOutcome {
        self.prompt.frame = self.prompt.frame.wrapping_add(1);
        AnyOutcome::Redraw
    }

    fn event(&mut self, event: Event) -> AnyOutcome {
        let outcome = match event {
            Event::Step(step) => self.prompt.step(step),
            Event::Broken(why) => self.prompt.broken(why),
            Event::Verified => self.prompt.verify(),
        };
        self.apply(outcome)
    }
}

/// The conversation: one helper at a time, each step posted to the dialog.
fn converse(cookie: &str, orders: &mpsc::Receiver<Order>, events: &Sender<Event>) {
    let Some(path) = helper::find() else {
        let _ = events.send(Event::Broken("polkit's helper is not installed".into()));
        return;
    };
    let mut attempt: Option<Attempt> = None;
    for order in orders {
        match order {
            Order::Start(user) => {
                attempt = None;
                match Attempt::start(&path, &user, cookie) {
                    Ok(a) => attempt = Some(a),
                    Err(e) => {
                        let _ = events.send(Event::Broken(format!("polkit's helper: {e}")));
                        continue;
                    }
                }
            }
            Order::Answer(mut secret) => {
                let Some(a) = attempt.as_mut() else { continue };
                if a.answer(&mut secret).is_err() {
                    let _ = events.send(Event::Step(Step::Done(false)));
                    attempt = None;
                    continue;
                }
            }
        }
        // Relay what the helper says until it asks again or is done.
        while let Some(a) = attempt.as_mut() {
            let said = a.step().unwrap_or(Step::Done(false));
            let asks = matches!(said, Step::Prompt { .. });
            let done = matches!(said, Step::Done(_));
            if events.send(Event::Step(said)).is_err() {
                return;
            }
            if done {
                attempt = None;
            }
            if asks || done {
                break;
            }
        }
    }
}

/// Ask. `Ok(true)` when the password was accepted.
pub fn ask(request: Request) -> Result<bool, String> {
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    let first = request.first_identity(own_uid());
    let user = request
        .identities
        .get(first)
        .map(|i| i.name.clone())
        .ok_or("no account may authorise this")?;
    let (orders, received) = mpsc::channel();
    let (sender, events) = host::events();
    let verified = sender.clone();
    let cookie = request.cookie.clone();
    std::thread::spawn(move || converse(&cookie, &received, &sender));
    let _ = orders.send(Order::Start(user));

    // Under Hyprland, Ctrl+Alt+Delete checks the prompt and tells it so
    // here. Anyone of the user's could connect and say so too; that makes a
    // genuine prompt look checked, which gains them nothing.
    let mut prompt = Prompt::new(request, first);
    let socket = std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|d| d.split(':').any(|d| d == "Hyprland"))
        .then(|| attention::socket(std::process::id()))
        .flatten();
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
        if let Ok(listener) = std::os::unix::net::UnixListener::bind(path) {
            prompt.checkable = true;
            let events = verified;
            std::thread::spawn(move || {
                for stream in listener.incoming().map_while(Result::ok) {
                    let mut line = String::new();
                    let _ = std::io::Read::take(stream, 64).read_to_string(&mut line);
                    if line.trim() == "verified" && events.send(Event::Verified).is_err() {
                        return;
                    }
                }
            });
        }
    }

    let authorised = std::rc::Rc::new(std::cell::Cell::new(false));
    let dialog = Dialog {
        appearance,
        fonts,
        prompt,
        layout: None,
        orders,
        authorised: std::rc::Rc::clone(&authorised),
    };
    let options = host::Options {
        placement: Placement::Centre,
        backdrop: 0x9900_0000,
        ..host::Options::new("alpymist-auth")
    };
    let result = host::run(dialog, &options, events, None);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result?;
    Ok(authorised.get())
}

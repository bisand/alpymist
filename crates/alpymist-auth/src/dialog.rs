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
use alpymist_auth::helper::{self, Running, Step};
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

/// What the conversation is told: by the dialog, and by the helpers it runs.
enum Order {
    /// Start an attempt for this account.
    Start(String),
    /// Answer the helper's prompt.
    Answer(Secret),
    /// Check this password with a second helper, beside the one waiting for
    /// a finger.
    Beside(Secret),
    /// Helper number so-and-so said this.
    Heard(u64, Step),
    /// The dialog is gone: end every helper.
    Quit,
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
            Outcome::Beside(secret) => {
                let _ = self.orders.send(Order::Beside(secret));
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

/// A helper the conversation runs, and its number, which is how what it says
/// is told from what an earlier one said.
struct Helper {
    number: u64,
    running: Running,
}

/// The conversation: the helper the dialog is answering, and at times one
/// beside it checking a password typed while the first waits for a finger.
/// Every helper's lines come back as [`Order::Heard`], so none waits on
/// another.
fn converse(
    cookie: &str,
    orders: &mpsc::Receiver<Order>,
    back: &mpsc::Sender<Order>,
    events: &Sender<Event>,
) {
    let Some(path) = helper::find() else {
        let _ = events.send(Event::Broken("polkit's helper is not installed".into()));
        return;
    };
    let mut numbers = 0u64;
    let mut start = |user: &str| -> Result<Helper, String> {
        numbers += 1;
        let number = numbers;
        let back = back.clone();
        Running::start(&path, user, cookie, move |step| {
            back.send(Order::Heard(number, step)).is_ok()
        })
        .map(|running| Helper { number, running })
        .map_err(|e| format!("polkit's helper: {e}"))
    };
    let mut user = String::new();
    let mut main: Option<Helper> = None;
    // The helper beside, and the password it is to be given when it asks.
    let mut beside: Option<(Helper, Option<Secret>)> = None;
    let is = |h: &Option<Helper>, n: u64| h.as_ref().is_some_and(|h| h.number == n);

    for order in orders {
        let tell = match order {
            Order::Quit => return,
            Order::Start(who) => {
                beside = None;
                main = None;
                user = who;
                match start(&user) {
                    Ok(h) => main = Some(h),
                    Err(why) => {
                        let _ = events.send(Event::Broken(why));
                    }
                }
                None
            }
            Order::Answer(mut secret) => {
                let failed = main
                    .as_mut()
                    .is_some_and(|h| h.running.answer(&mut secret).is_err());
                if failed {
                    main = None;
                }
                failed.then_some(Step::Done(false))
            }
            Order::Beside(secret) => match start(&user) {
                Ok(h) => {
                    beside = Some((h, Some(secret)));
                    None
                }
                Err(_) => Some(Step::Done(false)),
            },
            Order::Heard(n, step) if is(&main, n) => {
                if let Step::Done(ok) = step {
                    main = None;
                    if ok {
                        beside = None;
                    }
                }
                Some(step)
            }
            Order::Heard(n, step) if beside.as_ref().is_some_and(|(h, _)| h.number == n) => {
                match step {
                    // Its pam_fprintd found the reader held by the first, and
                    // this is the password. Anything asked after that is not
                    // what was typed, so the attempt ends there.
                    Step::Prompt { echo: false, .. } => {
                        let answered = beside.as_mut().and_then(|(h, secret)| {
                            let mut secret = secret.take()?;
                            h.running.answer(&mut secret).ok()
                        });
                        if answered.is_some() {
                            None
                        } else {
                            beside = None;
                            Some(Step::Done(false))
                        }
                    }
                    Step::Prompt { echo: true, .. } => {
                        beside = None;
                        Some(Step::Done(false))
                    }
                    Step::Done(ok) => {
                        beside = None;
                        if ok {
                            main = None;
                        }
                        Some(Step::Done(ok))
                    }
                    // What the dialog says about a wrong password is its own.
                    Step::Info(_) | Step::Error(_) => None,
                }
            }
            // From a helper already ended.
            Order::Heard(..) => None,
        };
        if let Some(step) = tell
            && events.send(Event::Step(step)).is_err()
        {
            return;
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
    let back = orders.clone();
    std::thread::spawn(move || converse(&cookie, &received, &back, &sender));
    let _ = orders.send(Order::Start(user));
    // The helpers hold the way back too, so the conversation is told when
    // the dialog is gone rather than left to notice.
    let quit = orders.clone();

    let mut prompt = Prompt::new(request, first);
    let socket = listen(verified, || Event::Verified);
    prompt.checkable = socket.is_some();

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
    let _ = quit.send(Order::Quit);
    if let Some(path) = &socket {
        std::fs::remove_file(path).ok();
    }
    result?;
    Ok(authorised.get())
}

/// Under Hyprland, Ctrl+Alt+Delete checks the prompt and tells it so on a
/// socket named for this process; `verified` is what the dialog is sent then.
/// The socket, for removing afterwards, when there is one to listen on.
///
/// Anyone of the user's could connect and say so too; that makes a genuine
/// prompt look checked, which gains them nothing.
pub fn listen<E: Send + 'static>(
    events: Sender<E>,
    verified: fn() -> E,
) -> Option<std::path::PathBuf> {
    let path = std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|d| d.split(':').any(|d| d == "Hyprland"))
        .then(|| attention::socket(std::process::id()))
        .flatten()?;
    std::fs::remove_file(&path).ok();
    let listener = std::os::unix::net::UnixListener::bind(&path).ok()?;
    std::thread::spawn(move || {
        for stream in listener.incoming().map_while(Result::ok) {
            let mut line = String::new();
            let _ = std::io::Read::take(stream, 64).read_to_string(&mut line);
            if line.trim() == "verified" && events.send(verified()).is_err() {
                return;
            }
        }
    });
    Some(path)
}

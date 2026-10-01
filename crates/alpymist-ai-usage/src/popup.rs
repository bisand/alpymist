//! The popup's state, and what every key and click does to it.
//!
//! No pixels, no files and no keyring. A [`Reading`] is everything shown,
//! already in words: every provider installed, whether it is on, its meters,
//! and what there is to say about how old its answer is. The host turns
//! input into calls here and does what the [`Outcome`] says — a [`Command`]
//! goes to the worker — and the worker's answers come back through
//! [`Popup::update`] and [`Popup::finished`]. So everything a person can do
//! in the popup is testable as a sequence of calls.
//!
//! Turning a provider on is where it is set up, as joining a network is
//! where its passphrase is asked. The worker says what the provider still
//! needs: a key is typed in a field under its row and goes to the keyring;
//! a vendor's tool is installed in a terminal, where its installer can be
//! watched.
//!
//! Everything a click can do, a key can do. Tab and Shift+Tab, or Up and
//! Down, walk the controls in the order drawn; Enter or Space presses the
//! one with the focus; Left and Right step the warning.

use crate::bar::{self, Entry};
use crate::config::Config;
use crate::definition::Definition;
use crate::store::Kept;
use alpymist_widget::Key;
use alpymist_widget::focus::FocusRing;

/// How many characters of small text fit across the popup: a note longer
/// than this is carried over to another line.
const NOTE_WIDTH: usize = 50;

/// The longest key taken.
const LONGEST_KEY: usize = 512;

/// The warning's least, most and step, as Settings has them.
const WARN: (u8, u8, u8) = (50, 95, 5);

/// One meter of a provider's, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// What it is: "5-hour limit".
    pub label: String,
    /// What it says: "42% used, resets in 2 h 10 min".
    pub says: String,
    /// How close to its limit, from 0 to 1, where it has one.
    pub used: Option<f64>,
}

/// A key a provider takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    /// Its name in the provider's file.
    pub key: String,
    /// What it is called: "API key".
    pub title: String,
    /// Whether the provider works without it.
    pub optional: bool,
}

/// A provider installed, as shown.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Provider {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Whether it is turned on.
    pub on: bool,
    /// Whether it reads what its vendor does not document.
    pub unofficial: bool,
    /// The vendor's tool it is read through, where it has one: "Codex,
    /// `OpenAI`'s command-line tool".
    pub tool: Option<String>,
    /// The keys it takes.
    pub credentials: Vec<Credential>,
    /// Its plan, where it says.
    pub plan: Option<String>,
    /// Its meters.
    pub lines: Vec<Line>,
    /// What to say under them: how old the answer is, and why.
    pub notes: Vec<String>,
    /// Whether the last asking failed.
    pub failed: bool,
}

/// Everything the popup shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    /// The providers installed, in the order they are listed.
    pub providers: Vec<Provider>,
    /// From what percentage used a meter is drawn as a warning.
    pub warn_at: u8,
    /// Whether a notification is sent when a limit is near.
    pub notify: bool,
}

impl Reading {
    /// Every provider installed and what is kept for it, in words, as of
    /// `now`. `every` is how many seconds apart a provider is asked.
    #[must_use]
    pub fn of(
        installed: &[(Definition, Kept)],
        config: &Config,
        now: i64,
        every: impl Fn(&Definition) -> i64,
    ) -> Self {
        let providers = installed
            .iter()
            .map(|(definition, kept)| {
                let on = config.is_enabled(&definition.id);
                let report = kept.report.as_ref().filter(|_| on);
                let entry = Entry {
                    definition,
                    kept,
                    every: every(definition),
                };
                Provider {
                    id: definition.id.clone(),
                    name: definition.name.clone(),
                    on,
                    unofficial: definition.unofficial,
                    tool: definition.requires.as_ref().map(|r| r.about.clone()),
                    credentials: definition
                        .credentials
                        .iter()
                        .map(|c| Credential {
                            key: c.key.clone(),
                            title: c.title.clone(),
                            optional: c.optional,
                        })
                        .collect(),
                    plan: report.and_then(|r| r.plan.clone()),
                    lines: report
                        .iter()
                        .flat_map(|r| &r.meters)
                        .map(|m| Line {
                            label: m.label().to_owned(),
                            says: m.says(now),
                            used: m.used(),
                        })
                        .collect(),
                    notes: if on {
                        bar::notes(&entry, now)
                            .iter()
                            .flat_map(|note| wrap(note, NOTE_WIDTH))
                            .collect()
                    } else {
                        Vec::new()
                    },
                    failed: on && kept.error.is_some(),
                }
            })
            .collect();
        Self {
            providers,
            warn_at: config.warn_at,
            notify: config.notify,
        }
    }

    /// The provider closest to its limit, which of its meters that is, and
    /// how close, from 0 to 1.
    #[must_use]
    pub fn worst(&self) -> Option<(&str, &str, f64)> {
        self.providers
            .iter()
            .flat_map(|p| {
                p.lines
                    .iter()
                    .filter_map(|l| Some((p.name.as_str(), l.label.as_str(), l.used?)))
            })
            .max_by(|a, b| a.2.total_cmp(&b.2))
    }

    /// Whether `used` is past the warning.
    #[must_use]
    pub fn warns(&self, used: f64) -> bool {
        used * 100.0 >= f64::from(self.warn_at)
    }

    fn at(&self, id: &str) -> Option<usize> {
        self.providers.iter().position(|p| p.id == id)
    }
}

/// `text` in lines of at most `width` characters, broken between words. A
/// word longer than a line has one to itself, and is cut where it is drawn.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// Something for the worker to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Ask every provider turned on again, now.
    Refresh,
    /// Turn a provider on, or say what it still needs.
    TurnOn(String),
    /// Turn a provider off.
    TurnOff(String),
    /// Keep a key in the keyring.
    Keep {
        /// The provider.
        id: String,
        /// The key's name in the provider's file.
        key: String,
        /// What was typed.
        secret: String,
    },
    /// Send a notification when a limit is near, or do not.
    Notify(bool),
    /// Warn from this percentage.
    WarnAt(u8),
}

/// What the worker says of a [`Command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// It is done.
    Done,
    /// The provider's vendor's tool is not installed.
    NeedsTool,
    /// The provider has no key of these names yet, and cannot do without.
    NeedsKeys(Vec<String>),
    /// It could not be done, and why.
    Failed(String),
}

/// What the host should do after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Hand this to the worker, and paint again.
    Run(Command),
    /// Set this provider up in a terminal — its vendor's installer, where it
    /// can be watched — and close the popup.
    Install(String),
    /// Open Settings at AI usage, and close the popup.
    Settings,
    /// Close the popup.
    Close,
}

/// A control: what can be clicked, and what the keyboard stops at.
/// Providers are by their place in the reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A provider's switch.
    Switch(usize),
    /// Give a provider its keys again.
    Keys(usize),
    /// The key field.
    Field,
    /// Show or hide the key.
    Reveal,
    /// Keep the key.
    Save,
    /// Install a provider's tool.
    Install,
    /// Notify when a limit is near.
    Notify,
    /// Warn from.
    Warn,
    /// Ask again.
    Refresh,
    /// Open Settings.
    Settings,
}

/// A key being typed for a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// The provider.
    pub id: String,
    /// The keys still to ask for, the first being the one typed now.
    pub queue: Vec<Credential>,
    /// What has been typed.
    pub typed: String,
    /// Draw it as typed rather than as dots.
    pub reveal: bool,
    /// Whether the provider already works, and its keys are being changed:
    /// nothing typed then leaves a key as it is.
    pub change: bool,
}

impl Ask {
    /// The key being typed.
    #[must_use]
    pub fn current(&self) -> Option<&Credential> {
        self.queue.first()
    }

    /// Whether nothing typed moves on rather than being a mistake.
    #[must_use]
    pub fn skippable(&self) -> bool {
        self.change || self.current().is_some_and(|c| c.optional)
    }
}

/// What a provider being turned on is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setup {
    /// A key.
    Ask(Ask),
    /// Its vendor's tool, which is not installed.
    Tool(String),
}

impl Setup {
    /// The provider it is for.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Ask(ask) => &ask.id,
            Self::Tool(id) => id,
        }
    }
}

/// A line above the buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What it says, in lines that fit.
    pub lines: Vec<String>,
    /// Whether it is a failure.
    pub error: bool,
}

/// The popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    reading: Reading,
    loaded: bool,
    asking: bool,
    /// A provider being switched, and to what.
    switching: Option<(String, bool)>,
    setup: Option<Setup>,
    /// A provider to turn on as soon as it is read: what Settings opens the
    /// popup for.
    begin: Option<String>,
    message: Option<Message>,
    focus: FocusRing<Target>,
    hover: Option<Target>,
}

impl Default for Popup {
    fn default() -> Self {
        Self::new()
    }
}

impl Popup {
    /// A popup before anything is read.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reading: Reading::default(),
            loaded: false,
            asking: false,
            switching: None,
            setup: None,
            begin: None,
            message: None,
            focus: FocusRing::new(Target::Refresh),
            hover: None,
        }
    }

    /// A popup that turns `id` on as soon as it is read, asking for whatever
    /// it needs.
    #[must_use]
    pub fn setting_up(id: &str) -> Self {
        Self {
            begin: Some(id.to_owned()),
            ..Self::new()
        }
    }

    /// The last reading.
    #[must_use]
    pub fn reading(&self) -> &Reading {
        &self.reading
    }

    /// Whether a reading has arrived.
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// Whether the providers are being asked now.
    #[must_use]
    pub fn asking(&self) -> bool {
        self.asking
    }

    /// Whether a provider's switch shows on: what was asked for, until it is
    /// read back.
    #[must_use]
    pub fn shown_on(&self, index: usize) -> bool {
        let Some(provider) = self.reading.providers.get(index) else {
            return false;
        };
        match &self.switching {
            Some((id, on)) if *id == provider.id => *on,
            _ => provider.on || self.setup_at() == Some(index),
        }
    }

    /// What a provider being turned on is waiting for.
    #[must_use]
    pub fn setup(&self) -> Option<&Setup> {
        self.setup.as_ref()
    }

    /// The provider the setup is under, by its place.
    #[must_use]
    pub fn setup_at(&self) -> Option<usize> {
        self.reading.at(self.setup.as_ref()?.id())
    }

    /// The line above the buttons.
    #[must_use]
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// Which control has the keyboard.
    #[must_use]
    pub fn focus(&self) -> Target {
        self.focus.get()
    }

    /// Whether to draw the focus ring.
    #[must_use]
    pub fn focus_visible(&self) -> bool {
        self.focus.visible()
    }

    /// What the pointer is over.
    #[must_use]
    pub fn hover(&self) -> Option<Target> {
        self.hover
    }

    /// The line under the title.
    #[must_use]
    pub fn status(&self) -> String {
        if !self.loaded {
            return "Reading…".into();
        }
        if self.asking {
            return "Asking…".into();
        }
        let on = self.reading.providers.iter().filter(|p| p.on).count();
        if self.reading.providers.is_empty() {
            return "No provider is installed".into();
        }
        if on == 0 {
            return "Turn a provider on to see its limits".into();
        }
        match self.reading.worst() {
            Some((name, _, _)) if on > 1 => format!("Closest to its limit: {name}"),
            Some((_, label, _)) => label.to_owned(),
            None => "Nothing with a limit".into(),
        }
    }

    /// Whether a provider's keys can be given again: it is on and takes any.
    #[must_use]
    pub fn has_keys_button(&self, index: usize) -> bool {
        self.reading
            .providers
            .get(index)
            .is_some_and(|p| p.on && !p.credentials.is_empty())
    }

    /// The controls Tab stops at, in the order drawn.
    #[must_use]
    pub fn focus_order(&self) -> Vec<Target> {
        let mut order = Vec::new();
        let setup_at = self.setup_at();
        for index in 0..self.reading.providers.len() {
            if self.has_keys_button(index) {
                order.push(Target::Keys(index));
            }
            order.push(Target::Switch(index));
            if setup_at == Some(index) {
                match &self.setup {
                    Some(Setup::Ask(_)) => {
                        order.extend([Target::Field, Target::Reveal, Target::Save]);
                    }
                    Some(Setup::Tool(_)) => order.push(Target::Install),
                    None => {}
                }
            }
        }
        if !self.reading.providers.is_empty() {
            order.extend([Target::Notify, Target::Warn, Target::Refresh]);
        }
        order.push(Target::Settings);
        order
    }

    /// A new reading.
    pub fn update(&mut self, reading: Reading) -> Outcome {
        if self.loaded && reading == self.reading {
            return Outcome::Unchanged;
        }
        // The focus stays on its provider by name, not by place.
        let held = match self.focus.get() {
            Target::Switch(i) | Target::Keys(i) => {
                self.reading.providers.get(i).map(|p| p.id.clone())
            }
            _ => None,
        };
        let first = !self.loaded;
        self.reading = reading;
        self.loaded = true;
        if let Some((id, on)) = &self.switching
            && self
                .reading
                .at(id)
                .is_some_and(|i| self.reading.providers[i].on == *on)
        {
            self.switching = None;
        }
        if self.setup_at().is_none() {
            self.setup = None;
        }
        if let Some(at) = held.as_deref().and_then(|id| self.reading.at(id)) {
            let moved = match self.focus.get() {
                Target::Keys(_) if self.has_keys_button(at) => Target::Keys(at),
                _ => Target::Switch(at),
            };
            if self.focus.visible() {
                self.focus.set(moved);
            } else {
                self.focus.click(moved);
            }
        }
        if first
            && !self.reading.providers.is_empty()
            && self.reading.providers.iter().all(|p| !p.on)
        {
            // Nothing to read yet: the first thing to do is turn one on.
            self.focus.click(Target::Switch(0));
        }
        self.focus.settle(&self.focus_order());
        if let Some(id) = self.begin.take()
            && let Some(at) = self.reading.at(&id)
        {
            self.focus.click(Target::Switch(at));
            if !self.reading.providers[at].on {
                return self.turn_on(id);
            }
        }
        Outcome::Redraw
    }

    fn say(&mut self, text: &str, error: bool) {
        self.message = Some(Message {
            lines: wrap(text, NOTE_WIDTH),
            error,
        });
    }

    fn turn_on(&mut self, id: String) -> Outcome {
        self.switching = Some((id.clone(), true));
        self.message = None;
        Outcome::Run(Command::TurnOn(id))
    }

    /// The worker finished `command`.
    pub fn finished(&mut self, command: &Command, reply: Reply) -> Outcome {
        match (command, reply) {
            (Command::Refresh, _) => self.asking = false,
            (Command::TurnOn(id), reply) => {
                self.switching = None;
                match reply {
                    Reply::Done => {
                        self.setup = None;
                        self.message = None;
                    }
                    Reply::NeedsTool => {
                        self.setup = Some(Setup::Tool(id.clone()));
                        self.focus.click(Target::Install);
                    }
                    Reply::NeedsKeys(keys) => {
                        let queue: Vec<Credential> = self
                            .reading
                            .at(id)
                            .map(|i| &self.reading.providers[i].credentials)
                            .into_iter()
                            .flatten()
                            .filter(|c| keys.contains(&c.key))
                            .cloned()
                            .collect();
                        if queue.is_empty() {
                            self.say("It needs a key its file does not name.", true);
                        } else {
                            self.setup = Some(Setup::Ask(Ask {
                                id: id.clone(),
                                queue,
                                typed: String::new(),
                                reveal: false,
                                change: false,
                            }));
                            self.focus.set(Target::Field);
                        }
                    }
                    Reply::Failed(why) => self.say(&why, true),
                }
            }
            (Command::Keep { .. }, Reply::Done) => return self.next_key(),
            (_, Reply::Failed(why)) => {
                self.switching = None;
                self.say(&why, true);
            }
            (Command::TurnOff(_), _) => self.switching = None,
            _ => {}
        }
        self.focus.settle(&self.focus_order());
        Outcome::Redraw
    }

    /// The key typed is kept, or passed over: on to the next, and after the
    /// last the provider is turned on.
    fn next_key(&mut self) -> Outcome {
        let Some(Setup::Ask(ask)) = &mut self.setup else {
            return Outcome::Redraw;
        };
        if !ask.queue.is_empty() {
            ask.queue.remove(0);
        }
        ask.typed.clear();
        ask.reveal = false;
        if !ask.queue.is_empty() {
            self.focus.set(Target::Field);
            return Outcome::Redraw;
        }
        let id = ask.id.clone();
        let at = self.reading.at(&id);
        self.setup = None;
        if let Some(at) = at {
            self.focus.click(Target::Switch(at));
        }
        self.turn_on(id)
    }

    fn submit(&mut self) -> Outcome {
        let Some(Setup::Ask(ask)) = &self.setup else {
            return Outcome::Unchanged;
        };
        let Some(credential) = ask.current() else {
            return Outcome::Unchanged;
        };
        if ask.typed.trim().is_empty() {
            if ask.skippable() {
                return self.next_key();
            }
            self.say("Nothing was typed.", true);
            return Outcome::Redraw;
        }
        let command = Command::Keep {
            id: ask.id.clone(),
            key: credential.key.clone(),
            secret: ask.typed.trim().to_owned(),
        };
        self.message = None;
        Outcome::Run(command)
    }

    fn cancel_setup(&mut self) -> Outcome {
        let at = self.setup_at();
        self.setup = None;
        self.message = None;
        if let Some(at) = at {
            self.focus.click(Target::Switch(at));
        }
        self.focus.settle(&self.focus_order());
        Outcome::Redraw
    }

    fn step_warn(&mut self, by: i16) -> Outcome {
        let (least, most, step) = WARN;
        let (least, step) = (i16::from(least), i16::from(step));
        let now = i16::from(self.reading.warn_at).clamp(least, i16::from(most));
        let span = i16::from(most) - least + step;
        let next = least + (now - least + by * step).rem_euclid(span);
        let next = u8::try_from(next).unwrap_or(WARN.0);
        self.reading.warn_at = next;
        Outcome::Run(Command::WarnAt(next))
    }

    fn press(&mut self, target: Target) -> Outcome {
        match target {
            Target::Switch(index) => {
                let Some(provider) = self.reading.providers.get(index) else {
                    return Outcome::Unchanged;
                };
                // One switch at a time.
                if self.switching.is_some() {
                    return Outcome::Unchanged;
                }
                let (id, on) = (provider.id.clone(), provider.on);
                if self.setup_at() == Some(index) {
                    return self.cancel_setup();
                }
                self.setup = None;
                if on {
                    self.switching = Some((id.clone(), false));
                    self.message = None;
                    Outcome::Run(Command::TurnOff(id))
                } else {
                    self.turn_on(id)
                }
            }
            Target::Keys(index) => {
                let Some(provider) = self.reading.providers.get(index) else {
                    return Outcome::Unchanged;
                };
                if self.setup_at() == Some(index) {
                    return self.cancel_setup();
                }
                self.setup = Some(Setup::Ask(Ask {
                    id: provider.id.clone(),
                    queue: provider.credentials.clone(),
                    typed: String::new(),
                    reveal: false,
                    change: true,
                }));
                self.message = None;
                Outcome::Redraw
            }
            Target::Field => Outcome::Redraw,
            Target::Reveal => match &mut self.setup {
                Some(Setup::Ask(ask)) => {
                    ask.reveal = !ask.reveal;
                    Outcome::Redraw
                }
                _ => Outcome::Unchanged,
            },
            Target::Save => self.submit(),
            Target::Install => match &self.setup {
                Some(Setup::Tool(id)) => Outcome::Install(id.clone()),
                _ => Outcome::Unchanged,
            },
            Target::Notify => {
                self.reading.notify = !self.reading.notify;
                Outcome::Run(Command::Notify(self.reading.notify))
            }
            Target::Warn => self.step_warn(1),
            // One asking at a time.
            Target::Refresh if self.asking => Outcome::Unchanged,
            Target::Refresh => {
                self.asking = true;
                Outcome::Run(Command::Refresh)
            }
            Target::Settings => Outcome::Settings,
        }
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        let before = self.focus;
        let outcome = self.key_inner(key);
        if outcome == Outcome::Unchanged && before != self.focus {
            Outcome::Redraw
        } else {
            outcome
        }
    }

    fn key_inner(&mut self, key: Key) -> Outcome {
        let order = self.focus_order();
        let focus = self.focus.get();
        match key {
            Key::Escape if self.setup.is_some() => self.cancel_setup(),
            Key::Escape => Outcome::Close,
            Key::Tab | Key::Down => self.step(&order, 1),
            Key::BackTab | Key::Up => self.step(&order, -1),
            Key::Home | Key::End => {
                let at = if key == Key::Home {
                    order.first()
                } else {
                    order.last()
                };
                match at {
                    Some(t) if !self.focus.shows(*t) => {
                        self.focus.set(*t);
                        Outcome::Redraw
                    }
                    _ => Outcome::Unchanged,
                }
            }
            Key::Left | Key::Right if focus == Target::Warn => {
                self.focus.set(focus);
                self.step_warn(if key == Key::Left { -1 } else { 1 })
            }
            // A key has no spaces in it.
            Key::Left | Key::Right => Outcome::Unchanged,
            Key::Space if focus == Target::Field => Outcome::Unchanged,
            Key::Enter if focus == Target::Field => self.submit(),
            Key::Enter | Key::Space => {
                self.focus.set(focus);
                let outcome = self.press(focus);
                self.land_in_field(focus);
                outcome
            }
            Key::Backspace => {
                let popped = match &mut self.setup {
                    Some(Setup::Ask(ask)) => ask.typed.pop().is_some(),
                    _ => false,
                };
                if popped {
                    self.focus.set(Target::Field);
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Clear => match &mut self.setup {
                Some(Setup::Ask(ask)) => {
                    ask.typed.clear();
                    self.focus.set(Target::Field);
                    Outcome::Redraw
                }
                _ => Outcome::Unchanged,
            },
        }
    }

    /// After the keys button or the field was pressed, typing goes on where
    /// it shows.
    fn land_in_field(&mut self, pressed: Target) {
        if matches!(self.setup, Some(Setup::Ask(_)))
            && matches!(pressed, Target::Field | Target::Keys(_))
        {
            self.focus.set(Target::Field);
        }
    }

    fn step(&mut self, order: &[Target], by: isize) -> Outcome {
        if self.focus.step(order, by) {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// Text was typed. It goes into the key field wherever the focus is,
    /// since there is nowhere else text could mean anything.
    pub fn text(&mut self, ch: char) -> Outcome {
        let Some(Setup::Ask(ask)) = &mut self.setup else {
            return Outcome::Unchanged;
        };
        if ch.is_control() || ch.is_whitespace() || ask.typed.chars().count() >= LONGEST_KEY {
            return Outcome::Unchanged;
        }
        ask.typed.push(ch);
        self.focus.set(Target::Field);
        if self.message.as_ref().is_some_and(|m| m.error) {
            self.message = None;
        }
        Outcome::Redraw
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

    /// `target` was clicked. The focus goes where the click did.
    pub fn click(&mut self, target: Target) -> Outcome {
        let before = self.focus;
        self.focus.click(target);
        let outcome = self.press(target);
        self.land_in_field(target);
        match outcome {
            Outcome::Unchanged if before != self.focus => Outcome::Redraw,
            outcome => outcome,
        }
    }
}

/// A reading to draw and test with: a subscription near a limit, spend last
/// known from a while ago, and two turned off.
#[must_use]
pub fn sample() -> Reading {
    let line = |label: &str, says: &str, used: Option<f64>| Line {
        label: label.into(),
        says: says.into(),
        used,
    };
    let key = |key: &str, title: &str, optional: bool| Credential {
        key: key.into(),
        title: title.into(),
        optional,
    };
    Reading {
        providers: vec![
            Provider {
                id: "claude".into(),
                name: "Claude".into(),
                on: true,
                tool: Some("Claude Code, Anthropic's command-line tool".into()),
                plan: Some("Max".into()),
                lines: vec![
                    line("5-hour limit", "42% used, resets in 2 h 10 min", Some(0.42)),
                    line("Weekly limit", "86% used, resets in 3 days", Some(0.86)),
                ],
                ..Provider::default()
            },
            Provider {
                id: "codex".into(),
                name: "ChatGPT (Codex)".into(),
                unofficial: true,
                tool: Some("Codex, OpenAI's command-line tool".into()),
                ..Provider::default()
            },
            Provider {
                id: "openai-api".into(),
                name: "OpenAI API".into(),
                on: true,
                credentials: vec![key("admin-key", "admin key", false)],
                lines: vec![line("This month", "$12.34 spent", None)],
                notes: vec![
                    "Last known, 40 min old.".into(),
                    "Could not reach it: no network.".into(),
                ],
                failed: true,
                ..Provider::default()
            },
            Provider {
                id: "openrouter".into(),
                name: "OpenRouter".into(),
                credentials: vec![
                    key("api-key", "API key", false),
                    key("management-key", "management key", true),
                ],
                ..Provider::default()
            },
        ],
        warn_at: 80,
        notify: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Outcome, Popup, Reading, Reply, Setup, Target, sample, wrap};
    use crate::config::Config;
    use crate::definition::Definition;
    use crate::report::{Meter, Report};
    use crate::store::Kept;
    use alpymist_widget::Key;

    const CODEX: usize = 1;
    const ROUTER: usize = 3;

    fn popup() -> Popup {
        let mut p = Popup::new();
        p.update(sample());
        p
    }

    fn type_in(p: &mut Popup, text: &str) {
        for ch in text.chars() {
            p.text(ch);
        }
    }

    #[test]
    fn a_reading_is_every_provider_installed_and_what_is_kept_in_words() {
        let claude = Definition::parse(
            "claude",
            "name = \"Claude\"\ndescription = \"\"\nexec = \"x\"\n",
        )
        .unwrap();
        let router = Definition::parse(
            "openrouter",
            "name = \"OpenRouter\"\ndescription = \"\"\nexec = \"x\"\n\
             [[credential]]\nkey = \"api-key\"\ntitle = \"API key\"\n",
        )
        .unwrap();
        let report = Report {
            plan: Some("Max".into()),
            meters: vec![Meter::Window {
                label: "5-hour limit".into(),
                used: 0.9,
                resets_at: Some(4600),
            }],
        };
        let kept = Kept::default()
            .after(400, Ok(report.clone()))
            .after(1000, Err("could not reach it: no network".into()));
        let mut config = Config::default();
        config.set_enabled("claude", true);
        // Turned off with a report still kept: none of it shows.
        let stale = Kept::default().after(400, Ok(report));
        let installed = [(claude, kept), (router, stale)];
        let reading = Reading::of(&installed, &config, 1000, |_| 600);

        let p = &reading.providers[0];
        assert!(p.on);
        assert_eq!(p.plan.as_deref(), Some("Max"));
        assert_eq!(p.lines[0].says, "90% used, resets in 1 h");
        assert_eq!(
            p.notes,
            ["Last known, 10 min old.", "could not reach it: no network"]
        );
        assert!(p.failed);
        let off = &reading.providers[1];
        assert!(!off.on && off.lines.is_empty() && off.notes.is_empty());
        assert_eq!(off.credentials[0].title, "API key");
        assert_eq!(reading.worst(), Some(("Claude", "5-hour limit", 0.9)));
        assert!(reading.warns(0.9) && !reading.warns(0.5));
    }

    #[test]
    fn turning_one_on_asks_for_its_key_and_keeps_it() {
        let mut p = popup();
        let on = Command::TurnOn("openrouter".into());
        assert_eq!(p.click(Target::Switch(ROUTER)), Outcome::Run(on.clone()));
        assert!(p.shown_on(ROUTER), "shown on at once");
        assert_eq!(
            p.click(Target::Switch(0)),
            Outcome::Redraw,
            "one switch at a time: only the focus moved"
        );
        p.finished(&on, Reply::NeedsKeys(vec!["api-key".into()]));
        assert_eq!(p.setup_at(), Some(ROUTER));
        assert_eq!(p.focus(), Target::Field);

        assert_eq!(p.key(Key::Enter), Outcome::Redraw);
        assert_eq!(p.message().unwrap().lines, ["Nothing was typed."]);
        type_in(&mut p, "sk-or 1");
        let keep = Command::Keep {
            id: "openrouter".into(),
            key: "api-key".into(),
            secret: "sk-or1".into(),
        };
        assert_eq!(p.key(Key::Enter), Outcome::Run(keep.clone()));
        assert!(p.message().is_none());
        // Kept: nothing more to ask, so it is turned on.
        assert_eq!(p.finished(&keep, Reply::Done), Outcome::Run(on.clone()));
        assert!(p.setup().is_none());
        assert_eq!(p.finished(&on, Reply::Done), Outcome::Redraw);
    }

    #[test]
    fn a_key_the_keyring_refuses_stays_typed_and_says_why() {
        let mut p = popup();
        let on = Command::TurnOn("openrouter".into());
        p.click(Target::Switch(ROUTER));
        p.finished(&on, Reply::NeedsKeys(vec!["api-key".into()]));
        type_in(&mut p, "sk");
        let Outcome::Run(keep) = p.click(Target::Save) else {
            panic!("expected the key to be kept");
        };
        p.finished(&keep, Reply::Failed("The keyring is locked.".into()));
        assert!(p.message().unwrap().error);
        let Some(Setup::Ask(ask)) = p.setup() else {
            panic!("still asking");
        };
        assert_eq!(ask.typed, "sk");
        assert_eq!(p.key(Key::Escape), Outcome::Redraw, "escape stops asking");
        assert!(p.setup().is_none() && !p.shown_on(ROUTER));
        assert_eq!(p.key(Key::Escape), Outcome::Close);
    }

    #[test]
    fn a_missing_tool_is_installed_in_a_terminal() {
        let mut p = popup();
        let on = Command::TurnOn("codex".into());
        p.click(Target::Switch(CODEX));
        p.finished(&on, Reply::NeedsTool);
        assert_eq!(p.setup(), Some(&Setup::Tool("codex".into())));
        assert!(p.focus_order().contains(&Target::Install));
        assert_eq!(p.click(Target::Install), Outcome::Install("codex".into()));
    }

    #[test]
    fn keys_are_changed_one_after_another_and_nothing_typed_leaves_one() {
        let mut p = popup();
        let mut reading = sample();
        reading.providers[ROUTER].on = true;
        p.update(reading);
        assert!(p.has_keys_button(ROUTER) && !p.has_keys_button(0));
        assert_eq!(p.click(Target::Keys(ROUTER)), Outcome::Redraw);
        assert_eq!(p.focus(), Target::Field);
        // The first is left as it is; the second is given.
        assert_eq!(p.key(Key::Enter), Outcome::Redraw);
        let Some(Setup::Ask(ask)) = p.setup() else {
            panic!("still asking");
        };
        assert_eq!(ask.current().unwrap().key, "management-key");
        type_in(&mut p, "mk");
        let Outcome::Run(keep) = p.key(Key::Enter) else {
            panic!("expected the key to be kept");
        };
        assert_eq!(
            p.finished(&keep, Reply::Done),
            Outcome::Run(Command::TurnOn("openrouter".into())),
            "asked again with the new key"
        );
    }

    #[test]
    fn turning_one_off_and_the_settings_at_hand() {
        let mut p = popup();
        let off = Command::TurnOff("claude".into());
        assert_eq!(p.click(Target::Switch(0)), Outcome::Run(off.clone()));
        assert!(!p.shown_on(0));
        p.finished(&off, Reply::Failed("could not undo its setup".into()));
        assert!(p.shown_on(0), "back as it was");
        assert!(p.message().unwrap().error);

        assert_eq!(
            p.click(Target::Notify),
            Outcome::Run(Command::Notify(false))
        );
        assert!(!p.reading().notify);
        assert_eq!(p.click(Target::Warn), Outcome::Run(Command::WarnAt(85)));
        p.key(Key::Left);
        assert_eq!(p.key(Key::Left), Outcome::Run(Command::WarnAt(75)));
        for _ in 0..5 {
            p.key(Key::Left);
        }
        assert_eq!(
            p.key(Key::Left),
            Outcome::Run(Command::WarnAt(95)),
            "round to the other end"
        );
    }

    #[test]
    fn opened_for_a_provider_it_is_turned_on_at_the_first_reading() {
        let mut p = Popup::setting_up("openrouter");
        assert_eq!(
            p.update(sample()),
            Outcome::Run(Command::TurnOn("openrouter".into()))
        );
        assert_eq!(p.focus(), Target::Switch(ROUTER));
        // One already on is only pointed at.
        let mut p = Popup::setting_up("claude");
        assert_eq!(p.update(sample()), Outcome::Redraw);
        assert_eq!(p.focus(), Target::Switch(0));
    }

    #[test]
    fn asking_again_is_asked_once_until_it_is_done() {
        let mut p = popup();
        assert_eq!(p.click(Target::Refresh), Outcome::Run(Command::Refresh));
        assert_eq!(p.status(), "Asking…");
        assert_eq!(p.click(Target::Refresh), Outcome::Unchanged);
        assert_eq!(p.finished(&Command::Refresh, Reply::Done), Outcome::Redraw);
        assert_eq!(p.status(), "Closest to its limit: Claude");
        assert!(!p.focus_visible(), "a click shows no ring");
    }

    #[test]
    fn tab_walks_everything_in_the_order_drawn() {
        let mut p = popup();
        assert_eq!(
            p.focus_order(),
            [
                Target::Switch(0),
                Target::Switch(1),
                Target::Keys(2),
                Target::Switch(2),
                Target::Switch(3),
                Target::Notify,
                Target::Warn,
                Target::Refresh,
                Target::Settings
            ]
        );
        p.key(Key::End);
        assert_eq!(p.key(Key::Enter), Outcome::Settings);
    }

    #[test]
    fn nothing_installed_and_nothing_on() {
        let mut p = Popup::new();
        assert_eq!(p.status(), "Reading…");
        p.update(Reading::default());
        assert_eq!(p.focus_order(), [Target::Settings]);
        assert_eq!(p.status(), "No provider is installed");
        assert_eq!(p.update(Reading::default()), Outcome::Unchanged);

        let mut off = sample();
        for provider in &mut off.providers {
            provider.on = false;
            provider.lines.clear();
        }
        let mut p = Popup::new();
        p.update(off);
        assert_eq!(p.focus(), Target::Switch(0), "the first thing to do");
        assert!(p.status().starts_with("Turn a provider on"));
    }

    #[test]
    fn a_long_note_is_carried_over_between_words() {
        assert_eq!(
            wrap("could not reach it: the name did not resolve", 20),
            ["could not reach it:", "the name did not", "resolve"]
        );
        assert!(wrap("", 20).is_empty());
    }
}

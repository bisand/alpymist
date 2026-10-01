//! The popup's state, and what every key and click does to it.
//!
//! No pixels and no files. A [`Reading`] is everything shown, already in
//! words: each provider turned on, its meters, and what there is to say
//! about how old its answer is. The host turns input into calls here and
//! does what the [`Outcome`] says, and fresh readings come back through
//! [`Popup::update`].
//!
//! Tab and Shift+Tab, or the arrows, walk the two buttons; Enter and Space
//! press the one with the focus.

use crate::bar::{self, Entry};
use crate::config::Config;
use alpymist_widget::Key;
use alpymist_widget::focus::FocusRing;

/// How many characters of small text fit across the popup: a note longer
/// than this is carried over to another line.
const NOTE_WIDTH: usize = 52;

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

/// A provider turned on, as shown.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Provider {
    /// Its name.
    pub name: String,
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
    /// The providers turned on, in the order they are listed.
    pub providers: Vec<Provider>,
    /// From what percentage used a meter is drawn as a warning.
    pub warn_at: u8,
}

impl Reading {
    /// What is kept for `entries`, in words, as of `now`.
    #[must_use]
    pub fn of(entries: &[Entry<'_>], config: &Config, now: i64) -> Self {
        let providers = entries
            .iter()
            .map(|entry| {
                let report = entry.kept.report.as_ref();
                Provider {
                    name: entry.definition.name.clone(),
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
                    notes: bar::notes(entry, now)
                        .iter()
                        .flat_map(|note| wrap(note, NOTE_WIDTH))
                        .collect(),
                    failed: entry.kept.error.is_some(),
                }
            })
            .collect();
        Self {
            providers,
            warn_at: config.warn_at,
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
}

/// `text` in lines of at most `width` characters, broken between words. A
/// word longer than a line has one to itself, and is cut where it is drawn.
fn wrap(text: &str, width: usize) -> Vec<String> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Ask every provider turned on again, now.
    Refresh,
}

/// What the host should do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Hand this to the worker, and paint again.
    Run(Command),
    /// Open Settings at AI usage, and close the popup.
    Settings,
    /// Close the popup.
    Close,
}

/// A button: what can be clicked, and what the keyboard stops at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Ask again.
    Refresh,
    /// Open Settings.
    Settings,
}

/// The popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    reading: Reading,
    loaded: bool,
    asking: bool,
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
            focus: FocusRing::new(Target::Refresh),
            hover: None,
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

    /// Which button has the keyboard.
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
        if self.reading.providers.is_empty() {
            return "Nothing is turned on".into();
        }
        match self.reading.worst() {
            Some((name, _, _)) if self.reading.providers.len() > 1 => {
                format!("Closest to its limit: {name}")
            }
            Some((_, label, _)) => label.to_owned(),
            None => "Nothing with a limit".into(),
        }
    }

    /// The buttons shown, in the order drawn: nothing to ask with nothing
    /// turned on.
    #[must_use]
    pub fn buttons(&self) -> Vec<Target> {
        if self.reading.providers.is_empty() {
            vec![Target::Settings]
        } else {
            vec![Target::Refresh, Target::Settings]
        }
    }

    /// A new reading.
    pub fn update(&mut self, reading: Reading) -> Outcome {
        if self.loaded && reading == self.reading {
            return Outcome::Unchanged;
        }
        self.reading = reading;
        self.loaded = true;
        self.focus.settle(&self.buttons());
        Outcome::Redraw
    }

    /// The worker finished `command`.
    pub fn finished(&mut self, command: Command) -> Outcome {
        match command {
            Command::Refresh => self.asking = false,
        }
        Outcome::Redraw
    }

    fn press(&mut self, target: Target) -> Outcome {
        match target {
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
        let order = self.buttons();
        match key {
            Key::Escape => Outcome::Close,
            Key::Tab | Key::Down | Key::Right => self.step(&order, 1),
            Key::BackTab | Key::Up | Key::Left => self.step(&order, -1),
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
            Key::Enter | Key::Space => {
                let target = self.focus.get();
                let shown = self.focus.visible();
                self.focus.set(target);
                match self.press(target) {
                    Outcome::Unchanged if !shown => Outcome::Redraw,
                    outcome => outcome,
                }
            }
            Key::Backspace | Key::Clear => Outcome::Unchanged,
        }
    }

    fn step(&mut self, order: &[Target], by: isize) -> Outcome {
        if self.focus.step(order, by) {
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

    /// `target` was clicked. The focus goes where the click did.
    pub fn click(&mut self, target: Target) -> Outcome {
        let before = self.focus;
        self.focus.click(target);
        match self.press(target) {
            Outcome::Unchanged if before != self.focus => Outcome::Redraw,
            outcome => outcome,
        }
    }
}

/// A reading to draw and test with: a subscription near a limit, credits
/// with a balance, and spend last known from a while ago.
#[must_use]
pub fn sample() -> Reading {
    let line = |label: &str, says: &str, used: Option<f64>| Line {
        label: label.into(),
        says: says.into(),
        used,
    };
    Reading {
        providers: vec![
            Provider {
                name: "Claude".into(),
                plan: Some("Max".into()),
                lines: vec![
                    line("5-hour limit", "42% used, resets in 2 h 10 min", Some(0.42)),
                    line("Weekly limit", "86% used, resets in 3 days", Some(0.86)),
                ],
                ..Provider::default()
            },
            Provider {
                name: "OpenRouter".into(),
                plan: None,
                lines: vec![line("Credits", "$3.20 left of $10.00", Some(0.68))],
                ..Provider::default()
            },
            Provider {
                name: "OpenAI API".into(),
                plan: None,
                lines: vec![line("This month", "$12.34 spent", None)],
                notes: vec![
                    "Last known, 40 min old.".into(),
                    "Could not reach it: no network.".into(),
                ],
                failed: true,
            },
        ],
        warn_at: 80,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Outcome, Popup, Reading, Target, sample, wrap};
    use crate::bar::Entry;
    use crate::config::Config;
    use crate::definition::Definition;
    use crate::report::{Meter, Report};
    use crate::store::Kept;
    use alpymist_widget::Key;

    fn popup() -> Popup {
        let mut p = Popup::new();
        p.update(sample());
        p
    }

    #[test]
    fn a_reading_is_what_is_kept_in_words() {
        let def = Definition::parse(
            "claude",
            "name = \"Claude\"\ndescription = \"\"\nexec = \"x\"\n",
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
            .after(400, Ok(report))
            .after(1000, Err("could not reach it: no network".into()));
        let entries = [Entry {
            definition: &def,
            kept: &kept,
            every: 600,
        }];
        let reading = Reading::of(&entries, &Config::default(), 1000);
        let p = &reading.providers[0];
        assert_eq!(p.plan.as_deref(), Some("Max"));
        assert_eq!(p.lines[0].says, "90% used, resets in 1 h");
        assert_eq!(
            p.notes,
            ["Last known, 10 min old.", "could not reach it: no network"]
        );
        assert!(p.failed);
        assert_eq!(reading.worst(), Some(("Claude", "5-hour limit", 0.9)));
        assert!(reading.warns(0.9));
        assert!(!reading.warns(0.5));
    }

    #[test]
    fn asking_again_is_asked_once_until_it_is_done() {
        let mut p = popup();
        assert_eq!(p.click(Target::Refresh), Outcome::Run(Command::Refresh));
        assert_eq!(p.status(), "Asking…");
        assert_eq!(p.click(Target::Refresh), Outcome::Unchanged);
        assert_eq!(p.finished(Command::Refresh), Outcome::Redraw);
        assert_eq!(p.status(), "Closest to its limit: Claude");
        assert!(!p.focus_visible(), "a click shows no ring");
    }

    #[test]
    fn the_keyboard_does_what_a_click_does() {
        let mut p = popup();
        // The first Tab shows the ring where the focus is; the next moves it.
        assert_eq!(p.key(Key::Tab), Outcome::Redraw);
        assert_eq!(p.focus(), Target::Refresh);
        assert_eq!(p.key(Key::Tab), Outcome::Redraw);
        assert_eq!(p.focus(), Target::Settings);
        assert!(p.focus_visible());
        assert_eq!(p.key(Key::Enter), Outcome::Settings);
        p.key(Key::BackTab);
        assert_eq!(p.key(Key::Space), Outcome::Run(Command::Refresh));
        assert_eq!(p.key(Key::Escape), Outcome::Close);
    }

    #[test]
    fn nothing_turned_on_leaves_only_settings() {
        let mut p = Popup::new();
        assert_eq!(p.status(), "Reading…");
        p.update(Reading::default());
        assert_eq!(p.buttons(), [Target::Settings]);
        assert_eq!(p.focus(), Target::Settings);
        assert_eq!(p.status(), "Nothing is turned on");
        assert_eq!(p.update(Reading::default()), Outcome::Unchanged);
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

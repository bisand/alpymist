//! The popup's state, and what every key and click does to it.
//!
//! No pixels and no iwd. The host turns input into calls here and does what
//! the [`Outcome`] says — a [`Command`] goes to the worker driving iwd — and
//! the worker's answers come back through [`Popup::update`] and
//! [`Popup::finished`]. So everything a person can do in the popup is testable
//! as a sequence of calls.
//!
//! Everything a click can do, a key can do. Tab and Shift+Tab move a focus
//! ring through the controls in the order they are drawn — the switch, the
//! joined network's buttons, rescan, the list, and the passphrase field with
//! its buttons — and Enter or Space presses whatever has it. The list is one
//! stop, walked with the arrows, as lists are everywhere else.

use crate::model::{Radio, Security, State, Station, validate_passphrase};

/// Something for the worker to ask of iwd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Look for networks.
    Scan,
    /// Join a network.
    Join {
        /// iwd's object for it.
        path: String,
        /// Its name, for messages.
        name: String,
        /// For a network iwd has no profile for.
        passphrase: Option<String>,
    },
    /// Leave the joined network.
    Disconnect,
    /// Forget a saved network.
    Forget {
        /// iwd's object for the profile.
        known_path: String,
        /// Its name, for messages.
        name: String,
    },
    /// Switch the radio.
    SetPowered(bool),
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
    /// Close the popup.
    Close,
}

/// A key, as far as the popup cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Previous network.
    Up,
    /// Next network.
    Down,
    /// First network.
    Home,
    /// Last network.
    End,
    /// Focus the next control.
    Tab,
    /// Focus the previous control.
    BackTab,
    /// Press the focused control: join the chosen network, send the
    /// passphrase, flip the switch.
    Enter,
    /// As Enter, except in the passphrase field, where it is a space.
    Space,
    /// Stop asking for a passphrase, or close.
    Escape,
    /// Delete a character of the passphrase.
    Backspace,
    /// Clear the passphrase.
    Clear,
}

/// Which control has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The on/off switch.
    Switch,
    /// Forget the joined network.
    Forget,
    /// Leave the joined network.
    Disconnect,
    /// Look for networks again.
    Rescan,
    /// The list of networks; the arrows choose within it.
    List,
    /// The passphrase field.
    Field,
    /// Show or hide the passphrase.
    Reveal,
    /// Send the passphrase.
    Join,
}

/// Something on the popup that can be clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The on/off switch.
    Switch,
    /// Leave the joined network.
    Disconnect,
    /// Forget the joined network.
    Forget,
    /// Look for networks again.
    Rescan,
    /// A network in the list, by its place among [`State::others`].
    Row(usize),
    /// The passphrase field.
    Field,
    /// Show or hide the passphrase.
    Reveal,
    /// Send the passphrase.
    Join,
}

/// A passphrase being typed for a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// iwd's object for the network.
    pub path: String,
    /// Its name.
    pub name: String,
    /// What has been typed.
    pub passphrase: String,
    /// Draw it as typed rather than as dots.
    pub reveal: bool,
}

/// What the worker is busy with, as the popup shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Busy {
    /// Joining a network.
    Joining(String),
    /// Leaving the joined network.
    Disconnecting,
    /// Forgetting a network.
    Forgetting(String),
    /// Switching the radio.
    Switching(bool),
}

/// A line under the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What it says.
    pub text: String,
    /// Whether it is a failure, drawn in the warning colour.
    pub error: bool,
}

/// The popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    state: State,
    selected: Option<usize>,
    scroll: usize,
    rows: usize,
    ask: Option<Ask>,
    busy: Option<Busy>,
    message: Option<Message>,
    hover: Option<Target>,
    focus: Focus,
    /// The focus ring shows once the keyboard has moved it, and hides again
    /// at a click, so a pointer user never sees a ring they did not ask for.
    focus_visible: bool,
    /// Whether a first state has arrived; until then there is nothing to say.
    loaded: bool,
}

impl Popup {
    /// A popup showing up to `rows` networks at once, before anything is
    /// known.
    #[must_use]
    pub fn new(rows: usize) -> Self {
        Self {
            state: State::default(),
            selected: None,
            scroll: 0,
            rows: rows.max(1),
            ask: None,
            busy: None,
            message: None,
            hover: None,
            focus: Focus::List,
            focus_visible: false,
            loaded: false,
        }
    }

    /// Which control has the keyboard.
    #[must_use]
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// Whether to draw the focus ring.
    #[must_use]
    pub fn focus_visible(&self) -> bool {
        self.focus_visible
    }

    /// The controls Tab stops at now, in the order they are drawn.
    #[must_use]
    pub fn focus_order(&self) -> Vec<Focus> {
        let mut order = Vec::new();
        let state = &self.state;
        if matches!(state.radio, Radio::On | Radio::Off) {
            order.push(Focus::Switch);
        }
        if state.radio != Radio::On {
            return order;
        }
        if matches!(state.station, Station::Connected | Station::Roaming) && state.link.is_some() {
            if self.current_known_path().is_some() {
                order.push(Focus::Forget);
            }
            order.push(Focus::Disconnect);
        }
        order.push(Focus::Rescan);
        if self.count() > 0 {
            order.push(Focus::List);
        }
        if self.ask.is_some() {
            order.extend([Focus::Field, Focus::Reveal, Focus::Join]);
        }
        order
    }

    fn current_known_path(&self) -> Option<String> {
        self.state
            .current
            .as_deref()
            .and_then(|c| self.state.network(c))
            .and_then(|n| n.known_path.clone())
    }

    /// Keep the focus on something that is still there.
    fn settle_focus(&mut self) {
        let order = self.focus_order();
        if order.contains(&self.focus) {
            return;
        }
        // The list is where the keyboard most likely wants to be; failing
        // that, the first thing there is.
        self.focus = if order.contains(&Focus::List) {
            Focus::List
        } else {
            order.first().copied().unwrap_or(Focus::List)
        };
    }

    fn move_focus(&mut self, by: isize) -> Outcome {
        let order = self.focus_order();
        if order.is_empty() {
            return Outcome::Unchanged;
        }
        let len = order.len().cast_signed();
        let next = match order.iter().position(|f| *f == self.focus) {
            Some(i) => (i.cast_signed() + by).rem_euclid(len),
            None if by > 0 => 0,
            None => len - 1,
        };
        self.focus = order[next.cast_unsigned()];
        self.focus_visible = true;
        if self.focus == Focus::List && self.selected.is_none() {
            self.selected = Some(self.scroll);
        }
        if let Some(i) = self.selected {
            self.reveal(i);
        }
        Outcome::Redraw
    }

    /// Wi-Fi as last read.
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Whether a state has arrived yet.
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// The network chosen in the list, by its place among [`State::others`].
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The first network shown.
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// How many networks show at once.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The passphrase being asked for, if one is.
    #[must_use]
    pub fn ask(&self) -> Option<&Ask> {
        self.ask.as_ref()
    }

    /// What the worker is doing, if anything.
    #[must_use]
    pub fn busy(&self) -> Option<&Busy> {
        self.busy.as_ref()
    }

    /// The line under the list.
    #[must_use]
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// What the pointer is over.
    #[must_use]
    pub fn hover(&self) -> Option<Target> {
        self.hover
    }

    /// The row the passphrase is asked under, by its place among the others.
    #[must_use]
    pub fn ask_row(&self) -> Option<usize> {
        let ask = self.ask.as_ref()?;
        self.state.others().position(|n| n.path == ask.path)
    }

    fn count(&self) -> usize {
        self.state.others().count()
    }

    fn path_of(&self, index: usize) -> Option<String> {
        self.state.others().nth(index).map(|n| n.path.clone())
    }

    /// A new reading from iwd. The chosen network stays chosen by identity,
    /// not by place, since the list reorders as signals change.
    pub fn update(&mut self, state: State) -> Outcome {
        if self.loaded && state == self.state {
            return Outcome::Unchanged;
        }
        let chosen = self.selected.and_then(|i| self.path_of(i));
        self.state = state;
        self.loaded = true;
        self.selected = chosen
            .and_then(|path| self.state.others().position(|n| n.path == path))
            .or_else(|| {
                (self.count() > 0)
                    .then_some(0)
                    .filter(|_| self.selected.is_some())
            });
        if self.state.radio != Radio::On {
            self.ask = None;
        }
        // A network joined by other means is no longer one to ask about.
        if let Some(ask) = &self.ask
            && self
                .state
                .network_at(&ask.path)
                .is_some_and(|n| n.connected)
        {
            self.ask = None;
        }
        if let Some(Busy::Switching(on)) = self.busy
            && (self.state.radio == Radio::On) == on
        {
            self.busy = None;
        }
        self.clamp_scroll();
        self.settle_focus();
        Outcome::Redraw
    }

    /// The worker finished `command`.
    pub fn finished(&mut self, command: &Command, result: Result<(), String>) -> Outcome {
        self.busy = None;
        match (command, result) {
            (Command::Scan, Ok(())) => return Outcome::Unchanged,
            (Command::Join { name, .. }, Ok(())) => {
                self.ask = None;
                self.message = Some(Message {
                    text: format!("Joined {name}"),
                    error: false,
                });
            }
            (Command::Forget { name, .. }, Ok(())) => {
                self.message = Some(Message {
                    text: format!("Forgot {name}"),
                    error: false,
                });
            }
            (_, Ok(())) => self.message = None,
            (Command::Join { path, name, .. }, Err(why)) => {
                // Asked again, with what was typed kept to correct.
                let needs_passphrase = self
                    .state
                    .network_at(path)
                    .is_some_and(|n| n.security == Security::Psk);
                if needs_passphrase && self.ask.is_none() {
                    self.ask = Some(Ask {
                        path: path.clone(),
                        name: name.clone(),
                        passphrase: String::new(),
                        reveal: false,
                    });
                }
                if self.ask.is_some() {
                    self.focus = Focus::Field;
                }
                self.message = Some(Message {
                    text: why,
                    error: true,
                });
            }
            (_, Err(why)) => {
                self.message = Some(Message {
                    text: why,
                    error: true,
                });
            }
        }
        self.settle_focus();
        Outcome::Redraw
    }

    fn run(&mut self, command: Command) -> Outcome {
        self.busy = match &command {
            Command::Scan => None,
            Command::Join { name, .. } => Some(Busy::Joining(name.clone())),
            Command::Disconnect => Some(Busy::Disconnecting),
            Command::Forget { name, .. } => Some(Busy::Forgetting(name.clone())),
            Command::SetPowered(on) => Some(Busy::Switching(*on)),
        };
        if command != Command::Scan {
            self.message = None;
        }
        Outcome::Run(command)
    }

    fn working(&self) -> bool {
        self.busy.is_some()
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        let in_ask = matches!(self.focus, Focus::Field | Focus::Reveal | Focus::Join);
        match key {
            Key::Tab => self.move_focus(1),
            Key::BackTab => self.move_focus(-1),
            Key::Escape if self.ask.is_some() => self.cancel_ask(),
            Key::Escape => Outcome::Close,
            Key::Up | Key::Down | Key::Home | Key::End if in_ask => Outcome::Unchanged,
            Key::Up => self.step(-1),
            Key::Down => self.step(1),
            Key::Home => self.step_to(0),
            Key::End => self.step_to(self.count().saturating_sub(1)),
            Key::Space if self.focus == Focus::Field => self.text(' '),
            Key::Enter | Key::Space => self.press(),
            Key::Backspace => {
                if self.ask.as_mut().and_then(|a| a.passphrase.pop()).is_some() {
                    self.focus = Focus::Field;
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Clear => match self.ask.as_mut() {
                Some(ask) => {
                    ask.passphrase.clear();
                    self.focus = Focus::Field;
                    Outcome::Redraw
                }
                None => Outcome::Unchanged,
            },
        }
    }

    /// Enter or Space on whatever has the focus.
    fn press(&mut self) -> Outcome {
        match self.focus {
            Focus::List => self.selected.map_or(Outcome::Unchanged, |i| {
                if self.ask_row() == Some(i) {
                    self.focus = Focus::Field;
                    return Outcome::Redraw;
                }
                self.ask = None;
                let outcome = self.activate(i);
                if outcome == Outcome::Unchanged {
                    Outcome::Redraw
                } else {
                    outcome
                }
            }),
            Focus::Field | Focus::Join => self.submit(),
            Focus::Reveal => self.toggle_reveal(),
            Focus::Switch => self.target(Target::Switch),
            Focus::Forget => self.target(Target::Forget),
            Focus::Disconnect => self.target(Target::Disconnect),
            Focus::Rescan => self.target(Target::Rescan),
        }
    }

    fn cancel_ask(&mut self) -> Outcome {
        self.ask = None;
        self.message = None;
        self.focus = Focus::List;
        self.settle_focus();
        Outcome::Redraw
    }

    fn toggle_reveal(&mut self) -> Outcome {
        let Some(ask) = self.ask.as_mut() else {
            return Outcome::Unchanged;
        };
        ask.reveal = !ask.reveal;
        Outcome::Redraw
    }

    /// Text was typed. It goes into the passphrase field wherever the focus
    /// is, since there is nowhere else text could mean anything.
    pub fn text(&mut self, ch: char) -> Outcome {
        let Some(ask) = self.ask.as_mut() else {
            return Outcome::Unchanged;
        };
        if ch.is_control() || ask.passphrase.chars().count() >= 63 {
            return Outcome::Unchanged;
        }
        ask.passphrase.push(ch);
        self.focus = Focus::Field;
        if self.message.as_ref().is_some_and(|m| m.error) {
            self.message = None;
        }
        Outcome::Redraw
    }

    fn submit(&mut self) -> Outcome {
        if self.working() {
            return Outcome::Unchanged;
        }
        let Some(ask) = &self.ask else {
            return Outcome::Unchanged;
        };
        if let Err(why) = validate_passphrase(&ask.passphrase) {
            self.message = Some(Message {
                text: why,
                error: true,
            });
            return Outcome::Redraw;
        }
        let command = Command::Join {
            path: ask.path.clone(),
            name: ask.name.clone(),
            passphrase: Some(ask.passphrase.clone()),
        };
        self.run(command)
    }

    fn step_to(&mut self, index: usize) -> Outcome {
        if self.count() == 0 {
            return Outcome::Unchanged;
        }
        self.focus = Focus::List;
        self.focus_visible = true;
        self.selected = Some(index);
        self.reveal(index);
        Outcome::Redraw
    }

    fn step(&mut self, by: isize) -> Outcome {
        let count = self.count();
        if count == 0 {
            return Outcome::Unchanged;
        }
        self.focus = Focus::List;
        self.focus_visible = true;
        let next = match self.selected {
            None if by > 0 => 0,
            None => count - 1,
            Some(i) => (i.cast_signed() + by)
                .rem_euclid(count.cast_signed())
                .cast_unsigned(),
        };
        self.selected = Some(next);
        self.reveal(next);
        Outcome::Redraw
    }

    /// Scroll so `index` is in view.
    fn reveal(&mut self, index: usize) {
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + self.rows {
            self.scroll = index + 1 - self.rows;
        }
    }

    fn clamp_scroll(&mut self) {
        let max = self.count().saturating_sub(self.rows);
        self.scroll = self.scroll.min(max);
        if let Some(i) = self.selected {
            self.reveal(i);
        }
    }

    /// Scroll the list by `rows`.
    pub fn scroll_by(&mut self, rows: i32) -> Outcome {
        let max = self.count().saturating_sub(self.rows);
        let next = self
            .scroll
            .saturating_add_signed(isize::try_from(rows).unwrap_or(0))
            .min(max);
        if next == self.scroll {
            return Outcome::Unchanged;
        }
        self.scroll = next;
        Outcome::Redraw
    }

    /// Join the network at `index` among the others, asking for a passphrase
    /// first if iwd will need one.
    fn activate(&mut self, index: usize) -> Outcome {
        if self.working() {
            return Outcome::Unchanged;
        }
        let Some(network) = self.state.others().nth(index).cloned() else {
            return Outcome::Unchanged;
        };
        match network.security {
            Security::Enterprise | Security::Wep if !network.known => {
                self.message = Some(Message {
                    text: format!("{} needs setting up by hand.", network.name),
                    error: true,
                });
                Outcome::Redraw
            }
            Security::Psk if !network.known => {
                if self.ask.as_ref().is_some_and(|a| a.path == network.path) {
                    return Outcome::Unchanged;
                }
                self.ask = Some(Ask {
                    path: network.path,
                    name: network.name,
                    passphrase: String::new(),
                    reveal: false,
                });
                self.message = None;
                self.focus = Focus::Field;
                Outcome::Redraw
            }
            _ => {
                self.ask = None;
                self.run(Command::Join {
                    path: network.path,
                    name: network.name,
                    passphrase: None,
                })
            }
        }
    }

    /// The pointer moved over `target`, or off everything.
    pub fn hover_over(&mut self, target: Option<Target>) -> Outcome {
        let mut changed = target != self.hover;
        self.hover = target;
        if let Some(Target::Row(i)) = target
            && self.ask.is_none()
            && self.selected != Some(i)
        {
            self.selected = Some(i);
            changed = true;
        }
        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// `target` was clicked. The focus goes where the click did.
    pub fn click(&mut self, target: Target) -> Outcome {
        let before = (self.focus, self.focus_visible, self.selected);
        self.focus_visible = false;
        self.focus = match target {
            Target::Switch => Focus::Switch,
            Target::Disconnect => Focus::Disconnect,
            Target::Forget => Focus::Forget,
            Target::Rescan => Focus::Rescan,
            Target::Row(_) => Focus::List,
            Target::Field => Focus::Field,
            Target::Reveal => Focus::Reveal,
            Target::Join => Focus::Join,
        };
        let outcome = self.target(target);
        if outcome == Outcome::Unchanged
            && before != (self.focus, self.focus_visible, self.selected)
        {
            Outcome::Redraw
        } else {
            outcome
        }
    }

    /// What pressing `target` does, by pointer or by key.
    fn target(&mut self, target: Target) -> Outcome {
        match target {
            Target::Switch => {
                if matches!(self.busy, Some(Busy::Switching(_)))
                    || matches!(self.state.radio, Radio::NoDaemon | Radio::NoAdapter)
                {
                    return Outcome::Unchanged;
                }
                self.ask = None;
                self.run(Command::SetPowered(self.state.radio != Radio::On))
            }
            Target::Disconnect => {
                if self.working() || self.state.station != Station::Connected {
                    return Outcome::Unchanged;
                }
                self.run(Command::Disconnect)
            }
            Target::Forget => {
                if self.working() {
                    return Outcome::Unchanged;
                }
                let (Some(known_path), Some(name)) =
                    (self.current_known_path(), self.state.current.clone())
                else {
                    return Outcome::Unchanged;
                };
                self.run(Command::Forget { known_path, name })
            }
            Target::Rescan => {
                if self.state.scanning || self.state.radio != Radio::On {
                    return Outcome::Unchanged;
                }
                self.run(Command::Scan)
            }
            Target::Row(i) => {
                self.selected = Some(i);
                if self.ask_row() == Some(i) {
                    return Outcome::Unchanged;
                }
                self.ask = None;
                let outcome = self.activate(i);
                if outcome == Outcome::Unchanged {
                    Outcome::Redraw
                } else {
                    outcome
                }
            }
            Target::Field => Outcome::Unchanged,
            Target::Reveal => self.toggle_reveal(),
            Target::Join => self.submit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Busy, Command, Focus, Key, Outcome, Popup, Target};
    use crate::model::{Radio, State, Station, sample};

    fn popup() -> Popup {
        let mut p = Popup::new(4);
        p.update(sample());
        p
    }

    fn select(p: &mut Popup, name: &str) -> usize {
        let i = p.state().others().position(|n| n.name == name).unwrap();
        p.hover_over(Some(Target::Row(i)));
        i
    }

    #[test]
    fn an_open_network_is_joined_straight_away() {
        let mut p = popup();
        let i = select(&mut p, "Kaffebar Gjest");
        let Outcome::Run(Command::Join { passphrase, .. }) = p.click(Target::Row(i)) else {
            panic!("expected a join");
        };
        assert_eq!(passphrase, None);
        assert_eq!(p.busy(), Some(&Busy::Joining("Kaffebar Gjest".into())));
    }

    #[test]
    fn a_new_secured_network_asks_for_its_passphrase_first() {
        let mut p = popup();
        let i = select(&mut p, "Naboen sitt nett");
        assert_eq!(p.key(Key::Enter), Outcome::Redraw);
        assert_eq!(p.ask_row(), Some(i));
        for ch in "short".chars() {
            p.text(ch);
        }
        assert_eq!(p.key(Key::Enter), Outcome::Redraw);
        assert!(
            p.message().unwrap().error,
            "a short passphrase is refused here"
        );
        for ch in " enough".chars() {
            p.text(ch);
        }
        assert!(p.message().is_none(), "typing clears the complaint");
        let Outcome::Run(Command::Join { passphrase, .. }) = p.key(Key::Enter) else {
            panic!("expected a join");
        };
        assert_eq!(passphrase.as_deref(), Some("short enough"));
    }

    #[test]
    fn a_wrong_passphrase_asks_again_with_the_reason() {
        let mut p = popup();
        select(&mut p, "Naboen sitt nett");
        p.key(Key::Enter);
        "wrong passphrase".chars().for_each(|c| {
            p.text(c);
        });
        let Outcome::Run(command) = p.key(Key::Enter) else {
            panic!("expected a join");
        };
        p.finished(
            &command,
            Err("Could not join — check the passphrase.".into()),
        );
        assert!(p.busy().is_none());
        assert!(p.ask().is_some());
        assert!(p.message().unwrap().text.contains("passphrase"));
    }

    #[test]
    fn escape_stops_asking_before_it_closes() {
        let mut p = popup();
        select(&mut p, "Naboen sitt nett");
        p.key(Key::Enter);
        assert_eq!(p.key(Key::Escape), Outcome::Redraw);
        assert!(p.ask().is_none());
        assert_eq!(p.key(Key::Escape), Outcome::Close);
    }

    #[test]
    fn enterprise_networks_are_explained_not_attempted() {
        let mut p = popup();
        let i = select(&mut p, "eduroam");
        assert_eq!(p.click(Target::Row(i)), Outcome::Redraw);
        assert!(p.message().unwrap().text.contains("by hand"));
    }

    #[test]
    fn the_switch_says_the_opposite_of_the_radio() {
        let mut p = popup();
        assert_eq!(
            p.click(Target::Switch),
            Outcome::Run(Command::SetPowered(false))
        );
        assert_eq!(
            p.click(Target::Switch),
            Outcome::Unchanged,
            "once is enough"
        );
        p.update(State {
            radio: Radio::Off,
            ..State::default()
        });
        assert!(p.busy().is_none(), "the radio going off ends the wait");
        assert_eq!(
            p.click(Target::Switch),
            Outcome::Run(Command::SetPowered(true))
        );
    }

    #[test]
    fn the_selection_follows_its_network_when_the_list_reorders() {
        let mut p = popup();
        select(&mut p, "DIRECT-printer");
        let mut state = sample();
        state.networks.swap(1, 4);
        p.update(state);
        let chosen = p.state().others().nth(p.selected().unwrap()).unwrap();
        assert_eq!(chosen.name, "DIRECT-printer");
    }

    #[test]
    fn moving_past_the_end_scrolls_and_wraps() {
        let mut p = popup();
        for _ in 0..4 {
            p.key(Key::Down);
        }
        assert_eq!(p.selected(), Some(3));
        assert_eq!(p.scroll(), 0);
        p.key(Key::Down);
        assert_eq!(
            p.selected(),
            Some(0),
            "four networks besides the joined one"
        );
        p.key(Key::Up);
        assert_eq!(p.selected(), Some(3));
    }

    fn tab_to(p: &mut Popup, focus: Focus) {
        for _ in 0..12 {
            if p.focus() == focus && p.focus_visible() {
                return;
            }
            p.key(Key::Tab);
        }
        panic!(
            "Tab never reached {focus:?}; order is {:?}",
            p.focus_order()
        );
    }

    #[test]
    fn tab_visits_every_control_in_the_order_drawn() {
        let p = popup();
        assert_eq!(
            p.focus_order(),
            [
                Focus::Switch,
                Focus::Forget,
                Focus::Disconnect,
                Focus::Rescan,
                Focus::List
            ]
        );
        let mut p = popup();
        assert!(!p.focus_visible(), "no ring until the keyboard is used");
        p.key(Key::Tab);
        assert_eq!(
            p.focus(),
            Focus::Switch,
            "from the list, Tab wraps to the top"
        );
        p.key(Key::BackTab);
        assert_eq!(p.focus(), Focus::List);
        assert_eq!(
            p.selected(),
            Some(0),
            "arriving at the list chooses a network"
        );
    }

    #[test]
    fn every_button_can_be_pressed_from_the_keyboard() {
        let mut p = popup();
        tab_to(&mut p, Focus::Switch);
        assert_eq!(p.key(Key::Space), Outcome::Run(Command::SetPowered(false)));
        let mut p = popup();
        tab_to(&mut p, Focus::Disconnect);
        assert_eq!(p.key(Key::Enter), Outcome::Run(Command::Disconnect));
        let mut p = popup();
        tab_to(&mut p, Focus::Forget);
        assert!(matches!(
            p.key(Key::Enter),
            Outcome::Run(Command::Forget { .. })
        ));
        let mut p = popup();
        tab_to(&mut p, Focus::Rescan);
        assert_eq!(p.key(Key::Enter), Outcome::Run(Command::Scan));
    }

    #[test]
    fn a_passphrase_can_be_typed_revealed_and_sent_without_a_pointer() {
        let mut p = popup();
        p.key(Key::Down);
        p.key(Key::Down);
        p.key(Key::Enter);
        assert_eq!(
            p.focus(),
            Focus::Field,
            "asking puts the cursor in the field"
        );
        "two words".chars().for_each(|c| {
            p.text(c);
        });
        p.key(Key::Space);
        assert_eq!(
            p.ask().unwrap().passphrase,
            "two words ",
            "space types in the field"
        );
        p.key(Key::Tab);
        assert_eq!(p.focus(), Focus::Reveal);
        p.key(Key::Space);
        assert!(p.ask().unwrap().reveal);
        p.key(Key::Tab);
        assert_eq!(p.focus(), Focus::Join);
        assert!(matches!(
            p.key(Key::Enter),
            Outcome::Run(Command::Join { .. })
        ));
    }

    #[test]
    fn arrows_stay_out_of_the_list_while_in_the_field() {
        let mut p = popup();
        p.key(Key::Down);
        p.key(Key::Down);
        p.key(Key::Enter);
        let chosen = p.selected();
        assert_eq!(p.key(Key::Down), Outcome::Unchanged);
        assert_eq!(p.selected(), chosen);
        p.key(Key::Escape);
        assert_eq!(p.focus(), Focus::List, "cancelling returns to the list");
    }

    #[test]
    fn the_focus_leaves_a_control_that_goes_away() {
        let mut p = popup();
        tab_to(&mut p, Focus::Disconnect);
        let mut state = sample();
        state.station = Station::Disconnected;
        state.current = None;
        state.link = None;
        for n in &mut state.networks {
            n.connected = false;
        }
        p.update(state);
        assert!(p.focus_order().contains(&p.focus()), "{:?}", p.focus());
    }

    #[test]
    fn disconnect_needs_a_connection() {
        let mut p = popup();
        assert_eq!(
            p.click(Target::Disconnect),
            Outcome::Run(Command::Disconnect)
        );
        let mut p = popup();
        let mut state = sample();
        state.station = Station::Disconnected;
        p.update(state);
        assert!(!matches!(p.click(Target::Disconnect), Outcome::Run(_)));
    }
}

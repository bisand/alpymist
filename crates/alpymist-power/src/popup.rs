//! The popup's state, and what every key and click does to it.
//!
//! No pixels and no files. The host turns input into calls here and does what
//! the [`Outcome`] says — a [`Command`] goes to the worker, which runs the
//! helper or saves the configuration — and fresh readings come back through
//! [`Popup::update`] and [`Popup::finished`]. Everything a person can do in
//! the popup is testable as a sequence of calls.
//!
//! Tab and Shift+Tab, or Up and Down, walk the controls in the order drawn:
//! the power modes, each setting, each check box. Left and Right choose a
//! mode or step a setting; Enter and Space step a setting forwards or tick a
//! box.

use crate::battery::Power;
use crate::config::{Action, Config};
use crate::profile::Profile;
use crate::system::CHARGE_LIMITS;
use alpymist_widget::Key;
use alpymist_widget::focus::FocusRing;

/// A reading of everything the popup shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    /// Batteries and charger.
    pub power: Power,
    /// The power mode now.
    pub profile: Option<Profile>,
    /// The power modes this machine offers.
    pub profiles: Vec<Profile>,
    /// Whether hibernating is possible here.
    pub can_hibernate: bool,
}

/// Something for the worker to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Switch the power mode.
    SetProfile(Profile),
    /// Stop charging at a percentage.
    SetChargeLimit(u8),
    /// Save the configuration.
    Save(Config),
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

/// A setting stepped through with arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Closing the lid on battery.
    Lid,
    /// Closing the lid on the charger.
    LidOnPower,
    /// Pressing the power button.
    Button,
    /// Where charging stops.
    ChargeLimit,
}

impl Setting {
    /// The name shown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Lid => "Lid closed on battery",
            Self::LidOnPower => "Lid closed, plugged in",
            Self::Button => "Power button",
            Self::ChargeLimit => "Stop charging at",
        }
    }
}

/// A check box for what the bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// The percentage.
    Percentage,
    /// Time left.
    Time,
    /// Power draw.
    Power,
    /// The power mode.
    Profile,
}

impl Show {
    /// Every box, in the order drawn.
    pub const ALL: [Self; 4] = [Self::Percentage, Self::Time, Self::Power, Self::Profile];

    /// The name shown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Percentage => "Percentage",
            Self::Time => "Time left",
            Self::Power => "Power draw",
            Self::Profile => "Power mode",
        }
    }
}

/// Which control has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The power modes, one stop.
    Profiles,
    /// A setting.
    Setting(Setting),
    /// A check box.
    Show(Show),
}

/// Something on the popup that can be clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A power mode.
    Profile(Profile),
    /// A setting.
    Setting(Setting),
    /// A check box.
    Show(Show),
}

/// A line under the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What it says.
    pub text: String,
    /// Whether it is a failure.
    pub error: bool,
}

/// The popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    reading: Reading,
    config: Config,
    loaded: bool,
    /// A mode asked for and not yet read back.
    pending_profile: Option<Profile>,
    /// A limit asked for and not yet read back.
    pending_limit: Option<u8>,
    focus: FocusRing<Focus>,
    hover: Option<Target>,
    message: Option<Message>,
}

impl Popup {
    /// A popup with the user's configuration, before anything is read.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            reading: Reading::default(),
            config,
            loaded: false,
            pending_profile: None,
            pending_limit: None,
            focus: FocusRing::new(Focus::Profiles),
            hover: None,
            message: None,
        }
    }

    /// The last reading.
    #[must_use]
    pub fn reading(&self) -> &Reading {
        &self.reading
    }

    /// The configuration as chosen.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Whether a reading has arrived.
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// The mode to show as chosen: the one asked for, until it is read back.
    #[must_use]
    pub fn profile(&self) -> Option<Profile> {
        self.pending_profile.or(self.reading.profile)
    }

    /// Whether a mode change is under way.
    #[must_use]
    pub fn switching(&self) -> bool {
        self.pending_profile.is_some()
    }

    /// The charge limit to show, if the battery has one.
    #[must_use]
    pub fn charge_limit(&self) -> Option<u8> {
        let now = self.reading.power.main()?.charge_limit?;
        Some(self.pending_limit.unwrap_or(now))
    }

    /// Which control has the keyboard.
    #[must_use]
    pub fn focus(&self) -> Focus {
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

    /// The line under the settings.
    #[must_use]
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// The settings shown, in order.
    #[must_use]
    pub fn settings(&self) -> Vec<Setting> {
        let mut out = Vec::new();
        if self.reading.power.has_battery() {
            out.extend([Setting::Lid, Setting::LidOnPower]);
        }
        out.push(Setting::Button);
        if self.charge_limit().is_some() {
            out.push(Setting::ChargeLimit);
        }
        out
    }

    /// The check boxes shown: none without a battery, whose bar module is
    /// hidden anyway.
    #[must_use]
    pub fn shows(&self) -> Vec<Show> {
        if self.reading.power.has_battery() {
            Show::ALL.to_vec()
        } else {
            Vec::new()
        }
    }

    /// Whether a box is ticked.
    #[must_use]
    pub fn shown(&self, show: Show) -> bool {
        let b = &self.config.bar;
        match show {
            Show::Percentage => b.percentage,
            Show::Time => b.time,
            Show::Power => b.power,
            Show::Profile => b.profile,
        }
    }

    /// A setting's value, as shown.
    #[must_use]
    pub fn value(&self, setting: Setting) -> String {
        let a = &self.config.actions;
        match setting {
            Setting::Lid => a.lid.label().into(),
            Setting::LidOnPower => a.lid_on_power.label().into(),
            Setting::Button => a.power_button.label().into(),
            Setting::ChargeLimit => match self.charge_limit() {
                Some(100) | None => "No limit".into(),
                Some(n) => format!("{n}%"),
            },
        }
    }

    /// The controls Tab stops at, in the order drawn.
    #[must_use]
    pub fn focus_order(&self) -> Vec<Focus> {
        let mut order = Vec::new();
        if !self.reading.profiles.is_empty() {
            order.push(Focus::Profiles);
        }
        order.extend(self.settings().into_iter().map(Focus::Setting));
        order.extend(self.shows().into_iter().map(Focus::Show));
        order
    }

    /// A new reading.
    pub fn update(&mut self, reading: Reading) -> Outcome {
        if self.loaded && reading == self.reading {
            return Outcome::Unchanged;
        }
        if self.pending_profile.is_some() && reading.profile == self.pending_profile {
            self.pending_profile = None;
        }
        if let Some(limit) = self.pending_limit
            && reading.power.main().and_then(|b| b.charge_limit) == Some(limit)
        {
            self.pending_limit = None;
        }
        self.reading = reading;
        self.loaded = true;
        self.focus.settle(&self.focus_order());
        Outcome::Redraw
    }

    /// The worker finished `command`.
    pub fn finished(&mut self, command: &Command, result: Result<(), String>) -> Outcome {
        match (command, result) {
            (Command::SetProfile(p), Ok(())) => {
                // The reading that follows confirms it; a firmware that maps
                // the mode to one of its own may never read back the same.
                self.pending_profile = None;
                self.reading.profile = Some(*p);
                self.message = None;
            }
            (Command::SetChargeLimit(n), Ok(())) => {
                self.pending_limit = None;
                if let Some(b) = self.reading.power.batteries.first_mut() {
                    b.charge_limit = Some(*n);
                }
                self.message = None;
            }
            (Command::Save(_), Ok(())) => return Outcome::Unchanged,
            (command, Err(why)) => {
                match command {
                    Command::SetProfile(_) => self.pending_profile = None,
                    Command::SetChargeLimit(_) => self.pending_limit = None,
                    Command::Save(_) => {}
                }
                self.message = Some(Message {
                    text: why,
                    error: true,
                });
            }
        }
        Outcome::Redraw
    }

    fn set_profile(&mut self, profile: Profile) -> Outcome {
        if self.profile() == Some(profile)
            || self.switching()
            || !self.reading.profiles.contains(&profile)
        {
            return Outcome::Unchanged;
        }
        self.pending_profile = Some(profile);
        self.message = None;
        Outcome::Run(Command::SetProfile(profile))
    }

    fn step_profile(&mut self, by: isize) -> Outcome {
        let profiles = &self.reading.profiles;
        if profiles.is_empty() {
            return Outcome::Unchanged;
        }
        let len = profiles.len().cast_signed();
        let next = match self
            .profile()
            .and_then(|p| profiles.iter().position(|q| *q == p))
        {
            // Stops at the ends: a mode is not a carousel.
            Some(i) => (i.cast_signed() + by).clamp(0, len - 1),
            None if by > 0 => 0,
            None => len - 1,
        };
        let target = profiles[next.cast_unsigned()];
        self.set_profile(target)
    }

    fn step_setting(&mut self, setting: Setting, by: isize) -> Outcome {
        let hibernate = self.reading.can_hibernate;
        let lid_order: Vec<Action> = Action::LID
            .into_iter()
            .filter(|a| *a != Action::Hibernate || hibernate)
            .collect();
        let a = &mut self.config.actions;
        match setting {
            Setting::Lid => a.lid = a.lid.step(&lid_order, by),
            Setting::LidOnPower => a.lid_on_power = a.lid_on_power.step(&lid_order, by),
            Setting::Button => a.power_button = a.power_button.step(&Action::BUTTON, by),
            Setting::ChargeLimit => {
                if self.pending_limit.is_some() {
                    return Outcome::Unchanged;
                }
                let Some(now) = self.charge_limit() else {
                    return Outcome::Unchanged;
                };
                let len = CHARGE_LIMITS.len().cast_signed();
                let at = CHARGE_LIMITS
                    .iter()
                    .position(|n| *n == now)
                    .map_or(0, |i| (i.cast_signed() + by).rem_euclid(len));
                let next = CHARGE_LIMITS[at.cast_unsigned()];
                self.pending_limit = Some(next);
                self.message = None;
                return Outcome::Run(Command::SetChargeLimit(next));
            }
        }
        self.message = None;
        Outcome::Run(Command::Save(self.config.clone()))
    }

    fn toggle(&mut self, show: Show) -> Outcome {
        let b = &mut self.config.bar;
        let flag = match show {
            Show::Percentage => &mut b.percentage,
            Show::Time => &mut b.time,
            Show::Power => &mut b.power,
            Show::Profile => &mut b.profile,
        };
        *flag = !*flag;
        Outcome::Run(Command::Save(self.config.clone()))
    }

    fn move_focus(&mut self, by: isize) -> Outcome {
        if self.focus.step(&self.focus_order(), by) {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
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
        let focus = self.focus.get();
        match key {
            Key::Escape => Outcome::Close,
            Key::Tab | Key::Down => self.move_focus(1),
            Key::BackTab | Key::Up => self.move_focus(-1),
            Key::Home | Key::End => {
                let order = self.focus_order();
                let at = if key == Key::Home {
                    order.first()
                } else {
                    order.last()
                };
                match at {
                    Some(f) if !self.focus.shows(*f) => {
                        self.focus.set(*f);
                        Outcome::Redraw
                    }
                    _ => Outcome::Unchanged,
                }
            }
            Key::Left | Key::Right => {
                let by = if key == Key::Left { -1 } else { 1 };
                self.focus.set(focus);
                match focus {
                    Focus::Profiles => self.step_profile(by),
                    Focus::Setting(s) => self.step_setting(s, by),
                    Focus::Show(_) => Outcome::Redraw,
                }
            }
            Key::Enter | Key::Space => {
                self.focus.set(focus);
                match focus {
                    Focus::Profiles => Outcome::Redraw,
                    Focus::Setting(s) => self.step_setting(s, 1),
                    Focus::Show(s) => self.toggle(s),
                }
            }
            Key::Backspace | Key::Clear => Outcome::Unchanged,
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
        let outcome = match target {
            Target::Profile(p) => {
                self.focus.click(Focus::Profiles);
                self.set_profile(p)
            }
            Target::Setting(s) => {
                self.focus.click(Focus::Setting(s));
                self.step_setting(s, 1)
            }
            Target::Show(s) => {
                self.focus.click(Focus::Show(s));
                self.toggle(s)
            }
        };
        if outcome == Outcome::Unchanged && before != self.focus {
            Outcome::Redraw
        } else {
            outcome
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Focus, Outcome, Popup, Reading, Setting, Show, Target};
    use crate::battery::{Power, sample};
    use crate::config::{Action, Config};
    use crate::profile::Profile;
    use alpymist_widget::Key;

    fn reading() -> Reading {
        Reading {
            power: sample(),
            profile: Some(Profile::Balanced),
            profiles: Profile::ALL.to_vec(),
            can_hibernate: false,
        }
    }

    fn popup() -> Popup {
        let mut p = Popup::new(Config::default());
        p.update(reading());
        p
    }

    #[test]
    fn a_mode_is_shown_chosen_at_once_and_asked_for_once() {
        let mut p = popup();
        assert_eq!(
            p.click(Target::Profile(Profile::Performance)),
            Outcome::Run(Command::SetProfile(Profile::Performance))
        );
        assert_eq!(p.profile(), Some(Profile::Performance));
        assert_eq!(
            p.click(Target::Profile(Profile::PowerSaver)),
            Outcome::Unchanged,
            "one change at a time"
        );
        let mut r = reading();
        r.profile = Some(Profile::Performance);
        p.update(r);
        assert!(!p.switching());
    }

    #[test]
    fn a_refused_mode_goes_back_and_says_why() {
        let mut p = popup();
        let Outcome::Run(command) = p.key(Key::Left) else {
            panic!("expected a change");
        };
        assert_eq!(command, Command::SetProfile(Profile::PowerSaver));
        p.finished(&command, Err("Not allowed.".into()));
        assert_eq!(p.profile(), Some(Profile::Balanced));
        assert!(p.message().unwrap().error);
    }

    #[test]
    fn arrows_stop_at_the_ends_of_the_modes() {
        let mut p = popup();
        let mut r = reading();
        r.profile = Some(Profile::Performance);
        p.update(r);
        assert_eq!(p.key(Key::Right), Outcome::Redraw, "already the last");
    }

    #[test]
    fn settings_step_and_save() {
        let mut p = popup();
        p.key(Key::Tab);
        p.key(Key::Tab);
        assert_eq!(p.focus(), Focus::Setting(Setting::Lid));
        let Outcome::Run(Command::Save(config)) = p.key(Key::Right) else {
            panic!("expected a save");
        };
        assert_eq!(config.actions.lid, Action::Lock);
        assert_eq!(p.value(Setting::Lid), "Lock");
        p.key(Key::Left);
        let Outcome::Run(Command::Save(config)) = p.key(Key::Left) else {
            panic!("expected a save");
        };
        assert_eq!(
            config.actions.lid,
            Action::Nothing,
            "hibernate is skipped where it cannot happen"
        );
    }

    #[test]
    fn ticking_a_box_saves_what_the_bar_shows() {
        let mut p = popup();
        let Outcome::Run(Command::Save(config)) = p.click(Target::Show(Show::Time)) else {
            panic!("expected a save");
        };
        assert!(config.bar.time);
        assert!(p.shown(Show::Time));
        assert!(!p.focus_visible(), "a click shows no ring");
    }

    #[test]
    fn the_charge_limit_cycles_through_what_firmware_accepts() {
        let mut p = popup();
        assert_eq!(p.value(Setting::ChargeLimit), "80%");
        let outcome = p.click(Target::Setting(Setting::ChargeLimit));
        assert_eq!(outcome, Outcome::Run(Command::SetChargeLimit(60)));
        p.finished(&Command::SetChargeLimit(60), Ok(()));
        assert_eq!(p.value(Setting::ChargeLimit), "60%");
        let outcome = p.click(Target::Setting(Setting::ChargeLimit));
        assert_eq!(outcome, Outcome::Run(Command::SetChargeLimit(100)));
    }

    #[test]
    fn a_desktop_has_only_the_modes_and_the_button() {
        let mut p = Popup::new(Config::default());
        p.update(Reading {
            power: Power::default(),
            profile: Some(Profile::Balanced),
            profiles: vec![Profile::PowerSaver, Profile::Balanced],
            can_hibernate: false,
        });
        assert_eq!(
            p.focus_order(),
            [Focus::Profiles, Focus::Setting(Setting::Button)]
        );
        assert_eq!(p.key(Key::Escape), Outcome::Close);
    }

    #[test]
    fn tab_walks_everything_in_order() {
        let p = popup();
        let order = p.focus_order();
        assert_eq!(order.first(), Some(&Focus::Profiles));
        assert_eq!(order.last(), Some(&Focus::Show(Show::Profile)));
        assert_eq!(order.len(), 1 + 4 + 4);
    }
}

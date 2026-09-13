//! The wizard: which screen we are on, and whether we may leave it.
//!
//! A pure state machine over [`Answers`]. It performs no installation and
//! touches nothing outside itself, so every route through the wizard — and
//! every reason it refuses to advance — can be tested directly.

use crate::answers::{
    Answers, Field, Issue, MIN_PASSWORD, Network, validate_hostname, validate_ipv4,
    validate_username,
};

/// The screens, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Step {
    /// Title screen, continuing from the splash.
    #[default]
    Welcome,
    /// Keyboard layout.
    Keyboard,
    /// Region and timezone.
    Region,
    /// Network.
    Network,
    /// Disk.
    Disk,
    /// User account and hostname.
    Account,
    /// Detected desktop tier, with an override.
    Desktop,
    /// Review everything before anything is written.
    Confirm,
    /// Doing the work.
    Install,
    /// Finished.
    Done,
}

impl Step {
    /// Every step in order.
    pub const ALL: [Self; 10] = [
        Self::Welcome,
        Self::Keyboard,
        Self::Region,
        Self::Network,
        Self::Disk,
        Self::Account,
        Self::Desktop,
        Self::Confirm,
        Self::Install,
        Self::Done,
    ];

    /// The heading shown at the top of the screen.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Keyboard => "Keyboard",
            Self::Region => "Region and time",
            Self::Network => "Network",
            Self::Disk => "Disk",
            Self::Account => "Your account",
            Self::Desktop => "Desktop",
            Self::Confirm => "Ready to install",
            Self::Install => "Installing",
            Self::Done => "Finished",
        }
    }

    /// One line under the heading saying what this screen is for.
    #[must_use]
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Welcome => "This will set up Alpymist on this machine.",
            Self::Keyboard => "Pick the layout your keyboard actually has.",
            Self::Region => "Used for the clock and for regional defaults.",
            Self::Network => "Alpymist can install without a network, but updates need one.",
            Self::Disk => "Nothing is written to the disk until you confirm on the last screen.",
            Self::Account => "The first account, which will be able to use doas.",
            Self::Desktop => "Chosen from what this machine can actually drive.",
            Self::Confirm => {
                "Check this over. This is the last point at which nothing has changed."
            }
            Self::Install => "Writing the system to disk.",
            Self::Done => "Alpymist is installed.",
        }
    }

    /// Whether this step is part of the numbered progress through the wizard.
    ///
    /// Welcome, Install and Done are not: they are not questions.
    #[must_use]
    pub fn is_question(self) -> bool {
        !matches!(self, Self::Welcome | Self::Install | Self::Done)
    }

    /// Steps that are questions, in order.
    pub fn questions() -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(|s| s.is_question())
    }

    /// This step's position among the questions, 1-based.
    #[must_use]
    pub fn question_number(self) -> Option<usize> {
        Self::questions().position(|s| s == self).map(|i| i + 1)
    }

    /// The next step, or `None` at the end.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        let i = Self::ALL.iter().position(|s| *s == self)?;
        Self::ALL.get(i + 1).copied()
    }

    /// The previous step, or `None` at the start.
    #[must_use]
    pub fn previous(self) -> Option<Self> {
        let i = Self::ALL.iter().position(|s| *s == self)?;
        i.checked_sub(1).and_then(|i| Self::ALL.get(i)).copied()
    }
}

/// The wizard's position and collected answers.
#[derive(Debug, Clone, Default)]
pub struct Wizard {
    step: Step,
    /// What the user has told us.
    pub answers: Answers,
}

impl Wizard {
    /// A fresh wizard at the welcome screen.
    #[must_use]
    pub fn new(answers: Answers) -> Self {
        Self {
            step: Step::Welcome,
            answers,
        }
    }

    /// The current screen.
    #[must_use]
    pub fn step(&self) -> Step {
        self.step
    }

    /// Whether going back is possible.
    ///
    /// Once installation starts there is nothing to go back to, so the answer
    /// is no from `Install` onwards.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.step.previous().is_some() && self.step < Step::Install
    }

    /// Go back one screen, if allowed.
    pub fn back(&mut self) -> bool {
        if !self.can_go_back() {
            return false;
        }
        if let Some(previous) = self.step.previous() {
            self.step = previous;
            return true;
        }
        false
    }

    /// What is stopping this screen from being completed.
    ///
    /// Empty means it is ready. Checked on every keystroke so the screen can
    /// show the reason next to the field rather than only on a failed Next.
    #[must_use]
    pub fn blockers(&self) -> Vec<Issue> {
        let a = &self.answers;
        let issue = |field, message: &str| Issue {
            field,
            message: message.to_string(),
        };
        let mut issues = Vec::new();

        match self.step {
            Step::Keyboard => {
                if a.keyboard.is_none() {
                    issues.push(issue(Field::Keyboard, "Choose a keyboard layout."));
                }
            }
            Step::Region => {
                if a.timezone.is_none() {
                    issues.push(issue(Field::Timezone, "Choose a timezone."));
                }
            }
            Step::Network => match &a.network {
                None => issues.push(issue(Field::Network, "Choose how this machine connects.")),
                Some(Network::Static {
                    address,
                    gateway,
                    dns,
                }) => {
                    for (value, with_prefix, what) in [
                        (address, true, "address"),
                        (gateway, false, "gateway"),
                        (dns, false, "name server"),
                    ] {
                        if let Err(why) = validate_ipv4(value, with_prefix) {
                            issues.push(issue(Field::Network, &format!("The {what}: {why}")));
                        }
                    }
                }
                Some(Network::Dhcp | Network::Offline) => {}
            },
            Step::Disk => match &a.disk {
                None => issues.push(issue(Field::Disk, "Choose a disk to install to.")),
                Some(plan) => {
                    if plan.is_destructive() && !a.disk_confirmed {
                        issues.push(issue(
                            Field::DiskConfirm,
                            "Confirm that everything on this disk may be erased.",
                        ));
                    }
                }
            },
            Step::Account => {
                if let Err(why) = validate_username(&a.username) {
                    issues.push(issue(Field::Username, &why));
                }
                if let Err(why) = validate_hostname(&a.hostname) {
                    issues.push(issue(Field::Hostname, &why));
                }
                if a.password.is_empty() {
                    issues.push(issue(Field::Password, "Choose a password."));
                } else if a.password != a.password_confirm {
                    issues.push(issue(Field::PasswordConfirm, "The passwords do not match."));
                }
            }
            Step::Desktop => {
                if a.effective_tier().is_none() {
                    issues.push(issue(Field::Disk, "Choose which desktop to install."));
                }
            }
            Step::Welcome | Step::Confirm | Step::Install | Step::Done => {}
        }
        issues
    }

    /// Things worth saying but not worth blocking on.
    ///
    /// Kept apart from [`blockers`](Self::blockers) deliberately. A short
    /// password on a machine in someone's spare room is their call, and an
    /// installer that refuses to continue over it is being paternalistic. An
    /// installer that says nothing at all is being negligent. This is the
    /// middle.
    #[must_use]
    pub fn advisories(&self) -> Vec<Issue> {
        let a = &self.answers;
        let note = |field, message: String| Issue { field, message };
        let mut notes = Vec::new();

        match self.step {
            Step::Network => {
                if matches!(a.network, Some(Network::Offline)) {
                    notes.push(note(
                        Field::Network,
                        "Without a network the installer uses only what is on this image. \
                         You can connect later."
                            .into(),
                    ));
                }
            }
            Step::Account => {
                if !a.password.is_empty() && a.password.chars().count() < MIN_PASSWORD {
                    notes.push(note(
                        Field::Password,
                        format!("Shorter than {MIN_PASSWORD} characters. That is your call."),
                    ));
                }
            }
            Step::Disk => {
                if matches!(
                    &a.disk,
                    Some(crate::answers::DiskPlan::WholeDisk { encrypt: false, .. })
                ) {
                    notes.push(note(
                        Field::Disk,
                        "Without encryption, anyone holding this disk can read it.".into(),
                    ));
                }
            }
            Step::Desktop if a.tier_was_overridden() => {
                notes.push(note(
                    Field::Disk,
                    "You have overridden what this machine reported it can drive. \
                         If the desktop will not start, this screen is why."
                        .into(),
                ));
            }
            _ => {}
        }
        notes
    }

    /// Whether the current screen is complete.
    #[must_use]
    pub fn can_advance(&self) -> bool {
        self.blockers().is_empty() && self.step.next().is_some()
    }

    /// Move to the next screen if this one is complete.
    ///
    /// Returns the blockers instead of advancing when it is not, so the caller
    /// can show them rather than having to ask again.
    ///
    /// # Errors
    /// Returns the reasons this screen cannot be left.
    pub fn advance(&mut self) -> Result<Step, Vec<Issue>> {
        let blockers = self.blockers();
        if !blockers.is_empty() {
            return Err(blockers);
        }
        match self.step.next() {
            Some(next) => {
                self.step = next;
                Ok(next)
            }
            None => Err(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Step, Wizard};
    use crate::answers::{Answers, DiskPlan, Field, Network};
    use alpymist_core::Tier;

    /// A wizard with every question answered acceptably.
    fn complete() -> Wizard {
        Wizard::new(Answers {
            keyboard: Some("no".into()),
            keyboard_variant: None,
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sda".into(),
                encrypt: true,
            }),
            disk_confirmed: true,
            username: "andre".into(),
            full_name: "André Biseth".into(),
            password: "correct horse battery".into(),
            password_confirm: "correct horse battery".into(),
            hostname: "alpymist".into(),
            disks: crate::disks::sample(),
            detected_tier: Some(Tier::Lite),
            tier_override: None,
        })
    }

    fn at(step: Step) -> Wizard {
        let mut w = complete();
        while w.step() != step {
            w.advance().expect("a complete wizard should advance");
        }
        w
    }

    #[test]
    fn a_complete_wizard_walks_from_welcome_to_done() {
        let mut w = complete();
        let mut seen = vec![w.step()];
        while w.advance().is_ok() && w.step() != Step::Done {
            seen.push(w.step());
        }
        seen.push(Step::Done);
        assert_eq!(
            seen,
            Step::ALL.to_vec(),
            "the wizard skipped or repeated a screen"
        );
    }

    #[test]
    fn every_step_has_a_title_and_a_subtitle() {
        for step in Step::ALL {
            assert!(!step.title().is_empty(), "{step:?} has no title");
            assert!(!step.subtitle().is_empty(), "{step:?} has no subtitle");
        }
    }

    #[test]
    fn only_the_question_screens_are_numbered() {
        assert_eq!(Step::Welcome.question_number(), None);
        assert_eq!(Step::Install.question_number(), None);
        assert_eq!(Step::Done.question_number(), None);
        assert_eq!(Step::Keyboard.question_number(), Some(1));
        assert_eq!(Step::Confirm.question_number(), Some(7));
        assert_eq!(Step::questions().count(), 7);
    }

    #[test]
    fn an_empty_wizard_cannot_leave_the_keyboard_screen() {
        let mut w = Wizard::new(Answers::default());
        assert!(w.advance().is_ok(), "welcome asks nothing");
        assert_eq!(w.step(), Step::Keyboard);
        let blockers = w.advance().expect_err("no layout chosen");
        assert_eq!(blockers[0].field, Field::Keyboard);
        assert_eq!(w.step(), Step::Keyboard, "a refused advance must not move");
    }

    #[test]
    fn a_destructive_disk_plan_needs_explicit_confirmation() {
        let mut w = at(Step::Disk);
        w.answers.disk_confirmed = false;
        let blockers = w.advance().expect_err("unconfirmed erase must not proceed");
        assert!(blockers.iter().any(|i| i.field == Field::DiskConfirm));
    }

    /// Using an existing partition destroys nothing by itself, so it does not
    /// demand the same acknowledgement.
    #[test]
    fn a_manual_disk_plan_needs_no_erase_confirmation() {
        let mut w = at(Step::Disk);
        w.answers.disk = Some(DiskPlan::Manual {
            root: "/dev/sda3".into(),
        });
        w.answers.disk_confirmed = false;
        assert!(w.advance().is_ok());
    }

    #[test]
    fn mismatched_passwords_are_caught() {
        let mut w = at(Step::Account);
        w.answers.password_confirm = "something else".into();
        let blockers = w.advance().expect_err("mismatch must block");
        assert!(blockers.iter().any(|i| i.field == Field::PasswordConfirm));
    }

    #[test]
    fn every_bad_account_field_is_reported_at_once_not_one_at_a_time() {
        let mut w = at(Step::Account);
        w.answers.username = "Andre".into();
        w.answers.hostname = "-nope".into();
        w.answers.password = String::new();
        let blockers = w.advance().expect_err("all three are invalid");
        let fields: Vec<_> = blockers.iter().map(|i| i.field).collect();
        assert!(fields.contains(&Field::Username));
        assert!(fields.contains(&Field::Hostname));
        assert!(fields.contains(&Field::Password));
    }

    #[test]
    fn a_malformed_static_address_blocks_the_network_screen() {
        let mut w = at(Step::Network);
        w.answers.network = Some(Network::Static {
            address: "192.168.1.10".into(), // missing the prefix
            gateway: "192.168.1.1".into(),
            dns: "1.1.1.1".into(),
        });
        assert!(w.advance().is_err());
    }

    #[test]
    fn a_well_formed_static_address_is_accepted() {
        let mut w = at(Step::Network);
        w.answers.network = Some(Network::Static {
            address: "192.168.1.10/24".into(),
            gateway: "192.168.1.1".into(),
            dns: "1.1.1.1".into(),
        });
        assert!(w.advance().is_ok());
    }

    #[test]
    fn going_back_returns_to_the_previous_screen() {
        let mut w = at(Step::Account);
        assert!(w.back());
        assert_eq!(w.step(), Step::Disk);
    }

    /// Once the disk is being written there is nothing to go back to.
    #[test]
    fn there_is_no_going_back_once_installation_starts() {
        let mut w = at(Step::Install);
        assert!(!w.can_go_back());
        assert!(!w.back());
        assert_eq!(w.step(), Step::Install);
    }

    #[test]
    fn the_welcome_screen_has_nothing_before_it() {
        let mut w = complete();
        assert!(!w.can_go_back());
        assert!(!w.back());
    }

    #[test]
    fn a_short_password_advises_but_does_not_block() {
        let mut w = at(Step::Account);
        w.answers.password = "short".into();
        w.answers.password_confirm = "short".into();
        assert!(w.blockers().is_empty(), "a short password must not block");
        assert!(
            w.advisories().iter().any(|i| i.field == Field::Password),
            "but it should say something"
        );
        assert!(w.advance().is_ok());
    }

    #[test]
    fn an_unencrypted_whole_disk_install_advises_but_does_not_block() {
        let mut w = at(Step::Disk);
        w.answers.disk = Some(DiskPlan::WholeDisk {
            device: "/dev/sda".into(),
            encrypt: false,
        });
        assert!(w.blockers().is_empty());
        assert!(!w.advisories().is_empty());
    }

    #[test]
    fn overriding_the_detected_tier_advises_why_the_desktop_might_not_start() {
        let mut w = at(Step::Desktop);
        w.answers.tier_override = Some(Tier::Full);
        assert!(w.blockers().is_empty(), "the override is allowed");
        assert!(!w.advisories().is_empty(), "but it should be flagged");
    }

    #[test]
    fn agreeing_with_the_probe_produces_no_advisory() {
        let mut w = at(Step::Desktop);
        w.answers.tier_override = Some(Tier::Lite); // same as detected
        assert!(w.advisories().is_empty());
    }

    #[test]
    fn installing_offline_is_allowed_but_explained() {
        let mut w = at(Step::Network);
        w.answers.network = Some(Network::Offline);
        assert!(w.blockers().is_empty());
        assert!(!w.advisories().is_empty());
        assert!(w.advance().is_ok());
    }

    #[test]
    fn no_screen_before_confirm_can_be_reached_with_unanswered_questions() {
        // Walk a default wizard forward; it must stall at the first question.
        let mut w = Wizard::new(Answers::default());
        w.advance().expect("welcome");
        for _ in 0..20 {
            if w.advance().is_err() {
                break;
            }
        }
        assert!(
            w.step() < Step::Confirm,
            "reached {:?} with nothing filled in",
            w.step()
        );
    }
}

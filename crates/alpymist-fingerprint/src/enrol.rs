//! What the fingerprint window is doing, and what each thing the daemon says
//! means for it. No drawing and no D-Bus: [`crate::fprint`] reports, this
//! decides, and the view shows.
//!
//! The daemon is fprintd, or validity-fprintd in its place; both speak
//! fprintd's words, which are the strings matched here.

/// The ten fingers, as fprintd names them, left hand thumb first.
pub const FINGERS: [&str; 10] = [
    "left-thumb",
    "left-index-finger",
    "left-middle-finger",
    "left-ring-finger",
    "left-little-finger",
    "right-thumb",
    "right-index-finger",
    "right-middle-finger",
    "right-ring-finger",
    "right-little-finger",
];

/// A finger as a person says it: "Right index finger".
#[must_use]
pub fn spoken(finger: &str) -> String {
    let mut words = finger.split('-');
    let hand = match words.next() {
        Some("left") => "Left",
        Some("right") => "Right",
        _ => return finger.to_owned(),
    };
    let rest: Vec<&str> = words.collect();
    format!("{hand} {}", rest.join(" "))
}

/// What is going on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doing {
    /// Waiting to be asked for something.
    Idle,
    /// Adding the chosen finger: scans taken so far, of how many.
    Enrolling {
        /// Scans the sensor has accepted.
        taken: u32,
        /// How many it wants, as the daemon says.
        of: u32,
    },
    /// Checking a finger against the ones enrolled.
    Testing,
    /// Removing every enrolled finger.
    Removing,
}

/// How the last thing ended, or what the sensor wants now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// Nothing to say.
    Nothing,
    /// Good news: "Right index finger added."
    Done(String),
    /// Try again, and how: "Put your finger in the middle of the sensor."
    Again(String),
    /// It did not work: "The sensor could not take this finger."
    Failed(String),
}

/// The window's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrol {
    /// Fingers enrolled for this account.
    pub enrolled: Vec<String>,
    /// The finger chosen, by index into [`FINGERS`].
    pub chosen: usize,
    /// What is going on.
    pub doing: Doing,
    /// What was said last.
    pub said: Said,
    /// The reader's name, when there is one; `None` when none was found.
    pub reader: Option<String>,
}

/// What the window wants done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Nothing.
    Nothing,
    /// Start enrolling this finger.
    Enrol(String),
    /// Start checking a finger.
    Test,
    /// Stop what is going on.
    Stop,
    /// Remove every enrolled finger.
    RemoveAll,
}

impl Enrol {
    /// The window for a reader called `reader` (or none), with `enrolled`
    /// fingers, the first enrolled one chosen, or the right index finger.
    #[must_use]
    pub fn new(reader: Option<String>, enrolled: Vec<String>) -> Self {
        let chosen = enrolled
            .first()
            .and_then(|f| FINGERS.iter().position(|g| g == f))
            .unwrap_or(6);
        Self {
            enrolled,
            chosen,
            doing: Doing::Idle,
            said: Said::Nothing,
            reader,
        }
    }

    /// The chosen finger's name.
    #[must_use]
    pub fn finger(&self) -> &'static str {
        FINGERS[self.chosen.min(FINGERS.len() - 1)]
    }

    /// Whether the chosen finger is enrolled.
    #[must_use]
    pub fn chosen_enrolled(&self) -> bool {
        self.enrolled.iter().any(|f| f == self.finger())
    }

    /// Whether nothing is going on, so a finger can be chosen and asked for.
    #[must_use]
    pub fn idle(&self) -> bool {
        self.doing == Doing::Idle && self.reader.is_some()
    }

    /// Choose finger `index`.
    pub fn choose(&mut self, index: usize) -> bool {
        if !self.idle() || index >= FINGERS.len() || index == self.chosen {
            return false;
        }
        self.chosen = index;
        self.said = Said::Nothing;
        true
    }

    /// Add the chosen finger. Asking for one already enrolled adds it again,
    /// which is how a finger that reads badly is fixed.
    pub fn enrol(&mut self, stages: u32) -> Ask {
        if !self.idle() {
            return Ask::Nothing;
        }
        self.doing = Doing::Enrolling {
            taken: 0,
            of: stages.max(1),
        };
        self.said = Said::Again("Put your finger on the sensor, and lift it again.".into());
        Ask::Enrol(self.finger().to_owned())
    }

    /// Check a finger against those enrolled.
    pub fn test(&mut self) -> Ask {
        if !self.idle() || self.enrolled.is_empty() {
            return Ask::Nothing;
        }
        self.doing = Doing::Testing;
        self.said = Said::Again("Touch the sensor with an enrolled finger.".into());
        Ask::Test
    }

    /// Remove every enrolled finger: fprintd removes them all at once.
    pub fn remove(&mut self) -> Ask {
        if !self.idle() || self.enrolled.is_empty() {
            return Ask::Nothing;
        }
        self.doing = Doing::Removing;
        self.said = Said::Nothing;
        Ask::RemoveAll
    }

    /// Give up on what is going on.
    pub fn cancel(&mut self) -> Ask {
        if matches!(self.doing, Doing::Enrolling { .. } | Doing::Testing) {
            self.doing = Doing::Idle;
            self.said = Said::Nothing;
            return Ask::Stop;
        }
        Ask::Nothing
    }

    /// The daemon's `EnrollStatus`: `result`, and whether that is the end.
    pub fn enroll_status(&mut self, result: &str, done: bool) {
        let Doing::Enrolling { taken, of } = self.doing else {
            return;
        };
        match result {
            "enroll-stage-passed" => {
                let taken = (taken + 1).min(of);
                self.doing = Doing::Enrolling { taken, of };
                self.said = Said::Again("Again: lift your finger and put it back.".into());
            }
            "enroll-completed" => {
                let finger = self.finger().to_owned();
                if !self.enrolled.contains(&finger) {
                    self.enrolled.push(finger.clone());
                }
                self.said = Said::Done(format!("{} added.", spoken(&finger)));
            }
            other => self.said = retry_or_fail(other, "enroll"),
        }
        if done {
            self.doing = Doing::Idle;
        }
    }

    /// The daemon's `VerifyStatus`.
    pub fn verify_status(&mut self, result: &str, done: bool) {
        if self.doing != Doing::Testing {
            return;
        }
        self.said = match result {
            "verify-match" => Said::Done("That finger is recognised.".into()),
            "verify-no-match" => Said::Failed("That finger is not one that is enrolled.".into()),
            other => retry_or_fail(other, "verify"),
        };
        if done {
            self.doing = Doing::Idle;
        }
    }

    /// Removing finished: `Ok` with none left, or why not.
    pub fn removed(&mut self, result: Result<(), String>) {
        if self.doing != Doing::Removing {
            return;
        }
        self.doing = Doing::Idle;
        self.said = match result {
            Ok(()) => {
                self.enrolled.clear();
                Said::Done("Every fingerprint is removed.".into())
            }
            Err(why) => Said::Failed(why),
        };
    }

    /// Something could not be asked for at all: the daemon said no, or
    /// polkit did.
    pub fn refused(&mut self, why: &str) {
        self.doing = Doing::Idle;
        self.said = Said::Failed(why.to_owned());
    }
}

/// What a retry or a failure means, in words, for results shared by
/// enrolling and checking: `enroll-swipe-too-short`, `verify-retry-scan`.
fn retry_or_fail(result: &str, verb: &str) -> Said {
    let what = result
        .strip_prefix(verb)
        .unwrap_or(result)
        .trim_start_matches('-');
    match what {
        "retry-scan" => Said::Again("That did not read well. Try again.".into()),
        "swipe-too-short" => {
            Said::Again("Too quick. Hold your finger on the sensor a little longer.".into())
        }
        "finger-not-centered" => Said::Again("Put your finger in the middle of the sensor.".into()),
        "remove-and-retry" => Said::Again("Lift your finger, then try again.".into()),
        "duplicate" => Said::Failed("That finger is already enrolled, as another one.".into()),
        "data-full" => Said::Failed("The sensor has no room for another fingerprint.".into()),
        "disconnected" => Said::Failed("The sensor went away.".into()),
        _ => Said::Failed("The sensor could not do that. Try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Ask, Doing, Enrol, Said, spoken};

    #[test]
    fn fingers_are_spoken_as_people_say_them() {
        assert_eq!(spoken("right-index-finger"), "Right index finger");
        assert_eq!(spoken("left-thumb"), "Left thumb");
    }

    #[test]
    fn enrolling_counts_the_scans_and_adds_the_finger() {
        let mut e = Enrol::new(Some("Synaptics".into()), vec![]);
        assert_eq!(e.finger(), "right-index-finger", "the usual one first");
        assert!(e.choose(1));
        assert_eq!(e.enrol(5), Ask::Enrol("left-index-finger".into()));
        assert!(!e.choose(2), "no other finger while one is being added");
        e.enroll_status("enroll-stage-passed", false);
        e.enroll_status("enroll-finger-not-centered", false);
        assert_eq!(e.doing, Doing::Enrolling { taken: 1, of: 5 });
        assert_eq!(
            e.said,
            Said::Again("Put your finger in the middle of the sensor.".into())
        );
        for _ in 0..4 {
            e.enroll_status("enroll-stage-passed", false);
        }
        e.enroll_status("enroll-completed", true);
        assert_eq!(e.doing, Doing::Idle);
        assert_eq!(e.enrolled, ["left-index-finger"]);
        assert!(e.chosen_enrolled());
        assert_eq!(e.said, Said::Done("Left index finger added.".into()));
    }

    #[test]
    fn testing_needs_a_finger_and_says_whether_it_matched() {
        let mut e = Enrol::new(Some("Synaptics".into()), vec![]);
        assert_eq!(e.test(), Ask::Nothing, "nothing to test against");
        let mut e = Enrol::new(Some("Synaptics".into()), vec!["right-index-finger".into()]);
        assert_eq!(e.test(), Ask::Test);
        e.verify_status("verify-no-match", true);
        assert!(matches!(e.said, Said::Failed(_)));
        assert_eq!(e.test(), Ask::Test);
        e.verify_status("verify-match", true);
        assert_eq!(e.said, Said::Done("That finger is recognised.".into()));
    }

    #[test]
    fn nothing_can_be_asked_without_a_reader_and_a_refusal_is_said() {
        let mut e = Enrol::new(None, vec![]);
        assert_eq!(e.enrol(5), Ask::Nothing);
        let mut e = Enrol::new(Some("Synaptics".into()), vec!["left-thumb".into()]);
        assert_eq!(e.chosen, 0, "the first enrolled finger is chosen");
        assert_eq!(e.remove(), Ask::RemoveAll);
        e.removed(Err("not authorized".into()));
        assert_eq!(e.enrolled, ["left-thumb"]);
        assert_eq!(e.enrol(5), Ask::Enrol("left-thumb".into()));
        e.refused("not authorized for net.reactivated.fprint.device.enroll");
        assert_eq!(e.doing, Doing::Idle);
        assert!(matches!(e.said, Said::Failed(_)));
    }
}

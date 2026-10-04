//! A finger on the reader, beside the password.
//!
//! When Settings › System lets a fingerprint unlock the screen, it writes
//! [`CONFIG`]: a PAM service of its own, `pam_fprintd` and nothing else, so a
//! finger is checked by the fingerprint daemon and never stands in for the
//! password in [`crate::pam`]'s service. The lock asks it on a thread of its
//! own for as long as the screen is locked, and the password field works as
//! it always did; whichever answers first unlocks.
//!
//! `pam_fprintd` gives up when nobody touches the reader for a while, when the
//! reader is held by something else, or when this account has no finger
//! enrolled; each of those is asked again, less often the more they repeat.
//! Three fingers that are not this account's is different: that is somebody
//! trying, and the lock stops listening to the reader until it is unlocked
//! with the password.

use alpymist_pam::{Conversation, Error};
use std::path::Path;
use std::time::{Duration, Instant};

/// The PAM service a finger is checked with.
pub const SERVICE: &str = "alpymist-lock-fingerprint";

/// Where it is configured. Settings writes it and removes it; its being there
/// is the switch.
pub const CONFIG: &str = "/etc/pam.d/alpymist-lock-fingerprint";

/// Whether a fingerprint may unlock the screen.
#[must_use]
pub fn enabled() -> bool {
    Path::new(CONFIG).exists()
}

/// What the reader had to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// The reader waits for a finger.
    Waiting,
    /// It does not, for now: no reader, no finger enrolled, or the reader
    /// held by something else. It is asked again later.
    Idle,
    /// A finger that is not one of this account's, or one that read badly.
    NotThatFinger,
    /// This account's finger: unlock.
    Matched,
    /// No more listening to the reader, and why.
    GaveUp(String),
}

/// What the lock says while the reader waits. These fit the card's line,
/// about 35 characters, or they are cut short.
pub const WAITING: &str = "Or touch the fingerprint reader";
/// What it says after a finger that did not do.
pub const NOT_THAT_FINGER: &str = "Finger not recognised. Try again.";
/// What it says after too many of those.
pub const TOO_MANY: &str = "Too many tries. Use the password.";

/// The longest wait before asking the reader again, after it said no quickly
/// several times: no reader, no finger enrolled.
const MOST: Duration = Duration::from_mins(1);

/// Listen to the reader for `user` until a finger matches, too many do not,
/// or `heard` returns false. Blocks: run it on a thread of its own.
pub fn listen(user: &str, heard: &(impl Fn(Heard) -> bool + Sync)) {
    let mut pause = Duration::ZERO;
    loop {
        let began = Instant::now();
        match check(user, heard) {
            Ok(()) => {
                heard(Heard::Matched);
                return;
            }
            Err(Error::MaxTries) => {
                heard(Heard::GaveUp(TOO_MANY.into()));
                return;
            }
            // Timed out with nobody touching it, the reader held elsewhere,
            // no finger enrolled, no daemon: ask again. What gave up at once
            // will likely do so again, so it is asked less and less often; a
            // wait that lasted is the ordinary kind, and asked again at once.
            Err(_) if began.elapsed() >= Duration::from_secs(5) => pause = Duration::ZERO,
            Err(_) => {
                pause = (pause * 2).clamp(Duration::from_secs(2), MOST);
                // Also how this learns the screen was unlocked meanwhile.
                if !heard(Heard::Idle) {
                    return;
                }
                std::thread::sleep(pause);
            }
        }
    }
}

/// One `pam_fprintd` attempt.
fn check(user: &str, heard: &(impl Fn(Heard) -> bool + Sync)) -> Result<(), Error> {
    alpymist_pam::authenticate(SERVICE, user, &Answers { heard })
}

/// This end of `pam_fprintd`'s conversation: it only ever says things.
struct Answers<'a, F> {
    heard: &'a F,
}

impl<F: Fn(Heard) -> bool + Sync> Conversation for Answers<'_, F> {
    // Nothing is asked for here: a password belongs to the other service,
    // and a stack that asks for one in this service is not what Settings
    // wrote.
    fn prompt(&self, _request: &str) -> Result<String, Error> {
        Err(Error::Conversation)
    }

    fn masked_prompt(&self, _request: &str) -> Result<String, Error> {
        Err(Error::Conversation)
    }

    /// "Failed to match fingerprint", or a scan to try again.
    fn error(&self, _message: &str) {
        (self.heard)(Heard::NotThatFinger);
    }

    /// "Place your finger on …", "Verification timed out".
    fn info(&self, message: &str) {
        let message = message.to_ascii_lowercase();
        if message.contains("finger") {
            (self.heard)(Heard::Waiting);
        }
    }
}

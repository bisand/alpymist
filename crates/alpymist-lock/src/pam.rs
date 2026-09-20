//! Is this the person?
//!
//! greetd runs PAM for the login screen and relays its conversation. There is
//! no greetd here, so the lock speaks to PAM itself, as its own service:
//! `/etc/pam.d/alpymist-lock`, which on Alpine is the `base-auth` stack a
//! login goes through. What unlocks the screen is what logs the account in,
//! and nothing else.
//!
//! This runs as the account itself, not as root. That is what PAM's own
//! setuid helper is for: `pam_unix` hands the check to `unix_chkpwd`, which
//! reads `/etc/shadow` so this never has to.
//!
//! **Only `pam_authenticate`.** Account management — expiry, allowed hours —
//! is deliberately not asked. A lock screen's question is "is this the
//! person", and refusing to let somebody back into a session that is already
//! running, over a password that expired while they were at lunch, answers a
//! question nobody asked.

use alpymist_greeter::login::Outcome;
use nonstick::{
    AuthnFlags, ConversationAdapter, ErrorCode, Result as PamResult, Transaction,
    TransactionBuilder,
};
use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The PAM service the lock authenticates as.
pub const SERVICE: &str = "alpymist-lock";

/// Where that service is configured: the package's file, and the vendor
/// directory linux-pam 1.7 falls back to — which is where Alpine 3.24 keeps
/// its own `base-auth` and the rest, `/etc/pam.d` being the administrator's.
pub const CONFIG: &str = "/etc/pam.d/alpymist-lock";
const VENDOR: &str = "/usr/lib/pam.d/alpymist-lock";

/// Whether there is a service for this to use.
///
/// Checked before the screen is locked, never after: a lock nobody can answer
/// is a machine that has to be rescued from a text console, and the one moment
/// this can be noticed cheaply is before the compositor covers the screen.
///
/// Note what this does *not* accept: PAM falls back to the `other` service for
/// a service it has no file for, and on Alpine that authenticates. Relying on
/// it would mean a lock whose behaviour is whatever `other` happens to say —
/// somewhere else, `other` is `pam_deny`, and the screen would never open.
#[must_use]
pub fn configured() -> bool {
    Path::new(CONFIG).exists() || Path::new(VENDOR).exists()
}

/// What this end of the conversation has to say.
struct Answers {
    user: String,
    password: String,
    /// Whether the password has been given already: PAM asking a second time
    /// is asking for something else.
    asked: Cell<bool>,
    /// What PAM said along the way, for the screen to show. Shared, because
    /// the conversation is PAM's for as long as the attempt lasts.
    said: Arc<Mutex<Vec<String>>>,
}

impl ConversationAdapter for Answers {
    fn prompt(&self, _request: impl AsRef<OsStr>) -> PamResult<OsString> {
        // Anything shown as it is typed is the username; the password is the
        // masked one below, and there is no field here for a third thing.
        Ok(OsString::from(&self.user))
    }

    fn masked_prompt(&self, _request: impl AsRef<OsStr>) -> PamResult<OsString> {
        if self.asked.replace(true) {
            // A second hidden prompt is a one-time code or a second factor.
            // Sending the password again would answer a different question
            // with it, so the attempt ends here instead.
            return Err(ErrorCode::ConversationError);
        }
        Ok(OsString::from(&self.password))
    }

    fn error_msg(&self, message: impl AsRef<OsStr>) {
        self.note(message);
    }

    fn info_msg(&self, message: impl AsRef<OsStr>) {
        self.note(message);
    }
}

impl Answers {
    fn note(&self, message: impl AsRef<OsStr>) {
        let message = message.as_ref().to_string_lossy().trim().to_owned();
        if message.is_empty() {
            return;
        }
        if let Ok(mut said) = self.said.lock() {
            said.push(message);
        }
    }
}

/// Ask PAM whether `password` is `user`'s.
///
/// The shape of the answer is the login screen's, because the screen is: an
/// [`Outcome::Started`] is a session got back into rather than begun, and
/// anything PAM said on the way comes back beside it.
#[must_use]
pub fn check(user: &str, password: &str) -> (Outcome, Vec<String>) {
    let said = Arc::new(Mutex::new(Vec::new()));
    let answers = Answers {
        user: user.to_owned(),
        password: password.to_owned(),
        asked: Cell::new(false),
        said: Arc::clone(&said),
    };
    let mut transaction = match TransactionBuilder::new_with_service(SERVICE)
        .username(user)
        .build(answers.into_conversation())
    {
        Ok(transaction) => transaction,
        Err(e) => return (broken(e), Vec::new()),
    };
    let result = transaction.authenticate(AuthnFlags::empty());
    let said = said.lock().map(|said| said.clone()).unwrap_or_default();
    match result {
        Ok(()) => (Outcome::Started, said),
        Err(e) => (outcome(e), said),
    }
}

/// What to tell somebody standing at a locked screen.
fn outcome(e: ErrorCode) -> Outcome {
    match e {
        // The ordinary answer, in the login screen's words.
        ErrorCode::AuthenticationError
        | ErrorCode::CredentialsInsufficient
        | ErrorCode::PermissionDenied
        | ErrorCode::UserUnknown => Outcome::Rejected("That password is not right.".into()),
        ErrorCode::MaxTries => {
            Outcome::Rejected("Too many tries. Wait a moment and try again.".into())
        }
        ErrorCode::AuthInfoUnavailable => {
            Outcome::Failed("The password could not be checked from here.".into())
        }
        e => broken(e),
    }
}

/// PAM could not do its job at all, which is worth saying differently: the
/// password may well be right, and the way back in is a text console.
fn broken(e: ErrorCode) -> Outcome {
    Outcome::Failed(format!("The password check is broken: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{ErrorCode, outcome};
    use alpymist_greeter::login::Outcome;

    #[test]
    fn a_wrong_password_is_a_wrong_password_and_not_a_fault() {
        assert!(matches!(
            outcome(ErrorCode::AuthenticationError),
            Outcome::Rejected(_)
        ));
        assert!(matches!(
            outcome(ErrorCode::UserUnknown),
            Outcome::Rejected(_),
        ));
    }

    #[test]
    fn a_broken_stack_says_so_rather_than_blaming_the_typing() {
        let Outcome::Failed(why) = outcome(ErrorCode::Abort) else {
            panic!("a broken PAM is not a rejection");
        };
        assert!(why.contains("broken"), "{why}");
    }
}

//! Asking PAM whether somebody is who they say.
//!
//! The lock screen's question, and all this does: start a transaction for a
//! service and a user, authenticate, end it. No account management, no
//! session, no credentials, no changing of passwords, and no way to ask for
//! them — what is not declared in [`ffi`] cannot be called.
//!
//! # Why this crate exists
//!
//! Every other crate in Alpymist but `alpymist-glesprobe` is
//! `#![forbid(unsafe_code)]`. PAM is a C library whose conversation is a
//! callback that hands back memory for PAM to free, and there is no safe way
//! to be that callback. It was first left to a third-party wrapper; under the
//! lock screen, between a password and the account, is the last place for a
//! crate nobody here has read (ADR 0024). So the `unsafe` is ours, in the one
//! module `ffi`, short enough to audit in a sitting, and everything else here
//! and everything that uses it stays safe.
//!
//! It is Linux-PAM's ABI that is declared: the only PAM Alpine ships. The
//! error numbers differ on other systems, so [`authenticate`] is Linux's
//! alone; the rest builds anywhere, which is what lets its tests run on a
//! developer's Mac.

mod ffi;

#[cfg(target_os = "linux")]
pub use ffi::authenticate;

/// This end of PAM's conversation: what a module asks for and what it says
/// while a transaction runs.
///
/// A module may ask more than once, and in any order; what is being asked is
/// in `request`, in the module's words and the system's language.
pub trait Conversation {
    /// Something asked for that may be shown as it is typed: a user name.
    ///
    /// # Errors
    ///
    /// Any [`Error`] ends the conversation, and with it the attempt.
    fn prompt(&self, request: &str) -> Result<String, Error>;

    /// Something asked for that may not be shown: a password. The answer is
    /// wiped from this side's memory as soon as PAM has its copy.
    ///
    /// # Errors
    ///
    /// Any [`Error`] ends the conversation, and with it the attempt.
    fn masked_prompt(&self, request: &str) -> Result<String, Error>;

    /// Something a module says went wrong.
    fn error(&self, message: &str);

    /// Something a module says, to be shown.
    fn info(&self, message: &str);
}

/// Why PAM did not say yes: its own return values, the ones an attempt to
/// authenticate can end in named and the rest kept as their number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `PAM_AUTH_ERR`: not the person. A wrong password, above all.
    Authentication,
    /// `PAM_CRED_INSUFFICIENT`: this program may not check that.
    CredentialsInsufficient,
    /// `PAM_AUTHINFO_UNAVAIL`: what the answer is checked against could not
    /// be reached.
    AuthInfoUnavailable,
    /// `PAM_USER_UNKNOWN`: no such account.
    UserUnknown,
    /// `PAM_MAXTRIES`: too many attempts; no more are taken for now.
    MaxTries,
    /// `PAM_PERM_DENIED`: refused.
    PermissionDenied,
    /// `PAM_CONV_ERR`: the conversation failed, which is also what a
    /// [`Conversation`] returns to end one.
    Conversation,
    /// `PAM_SERVICE_ERR`: a module failed.
    Service,
    /// `PAM_SYSTEM_ERR`: PAM itself failed.
    System,
    /// `PAM_BUF_ERR`: out of memory.
    Buffer,
    /// `PAM_ABORT`: the stack gave up.
    Abort,
    /// A service or user name with a NUL in it, which cannot be handed to C.
    Nul,
    /// Any other return value, by Linux-PAM's number.
    Other(i32),
}

impl Error {
    /// What a return value of Linux-PAM's means; `None` for `PAM_SUCCESS`.
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => return None,
            3 => Self::Service,
            4 => Self::System,
            5 => Self::Buffer,
            6 => Self::PermissionDenied,
            7 => Self::Authentication,
            8 => Self::CredentialsInsufficient,
            9 => Self::AuthInfoUnavailable,
            10 => Self::UserUnknown,
            11 => Self::MaxTries,
            19 => Self::Conversation,
            26 => Self::Abort,
            other => Self::Other(other),
        })
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Authentication => "authentication failed",
            Self::CredentialsInsufficient => "not permitted to check these credentials",
            Self::AuthInfoUnavailable => "the authentication service cannot be reached",
            Self::UserUnknown => "no such user",
            Self::MaxTries => "too many tries",
            Self::PermissionDenied => "permission denied",
            Self::Conversation => "the conversation failed",
            Self::Service => "a PAM module failed",
            Self::System => "PAM failed",
            Self::Buffer => "out of memory",
            Self::Abort => "PAM gave up",
            Self::Nul => "a name with a NUL in it",
            Self::Other(code) => return write!(f, "PAM error {code}"),
        })
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Error;

    /// The numbers are Linux-PAM's `_pam_types.h`, which is its ABI.
    #[test]
    fn the_return_values_are_linux_pams() {
        assert_eq!(Error::from_code(0), None);
        assert_eq!(Error::from_code(7), Some(Error::Authentication));
        assert_eq!(Error::from_code(10), Some(Error::UserUnknown));
        assert_eq!(Error::from_code(11), Some(Error::MaxTries));
        assert_eq!(Error::from_code(19), Some(Error::Conversation));
        assert_eq!(Error::from_code(26), Some(Error::Abort));
        // PAM_ACCT_EXPIRED: not something authenticating returns.
        assert_eq!(Error::from_code(13), Some(Error::Other(13)));
        assert_eq!(Error::Other(13).to_string(), "PAM error 13");
    }
}

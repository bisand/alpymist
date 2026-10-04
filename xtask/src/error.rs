//! What went wrong, as something to print.
//!
//! A task runner's errors are read by whoever ran it and by nothing else, so
//! an error here is its message: what was being done, then why it failed. `?`
//! takes any error there is, [`Context`] says what was being done, and
//! [`bail!`] and [`ensure!`] stop with a message of their own.

use std::fmt::{self, Display};

/// A failure, already put into words.
pub struct Error(String);

/// What every task returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(message: impl Display) -> Self {
        Self(message.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `main` prints what it returns with `Debug`; the message is what to print.
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// Not itself a `std::error::Error`, which is what leaves room for this.
impl<E: std::error::Error> From<E> for Error {
    fn from(e: E) -> Self {
        Self(e.to_string())
    }
}

/// Saying what was being done when something failed, or was not there.
pub trait Context<T> {
    /// Fail with `context`, then a colon, then why.
    fn context(self, context: impl Display) -> Result<T>;

    /// As [`Context::context`], with the words made only on failure.
    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T>;
}

impl<T, E: Display> Context<T> for std::result::Result<T, E> {
    fn context(self, context: impl Display) -> Result<T> {
        self.map_err(|e| Error(format!("{context}: {e}")))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T> {
        self.map_err(|e| Error(format!("{}: {e}", context())))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, context: impl Display) -> Result<T> {
        self.ok_or_else(|| Error::new(context))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T> {
        self.ok_or_else(|| Error::new(context()))
    }
}

/// Stop here, with this to say.
macro_rules! bail {
    ($($message:tt)+) => {
        return Err($crate::error::Error::new(format!($($message)+)))
    };
}

/// Stop here, with this to say, unless the condition holds.
macro_rules! ensure {
    ($condition:expr, $($message:tt)+) => {
        if !$condition {
            $crate::error::bail!($($message)+);
        }
    };
}

pub(crate) use {bail, ensure};

#[cfg(test)]
mod tests {
    use super::{Context, Result};

    fn parse(text: &str) -> Result<u32> {
        let n: u32 = text.parse()?;
        ensure!(n < 10, "{n} is not a digit");
        if n == 7 {
            bail!("not {n}");
        }
        Ok(n)
    }

    #[test]
    fn an_error_is_what_was_being_done_and_then_why() {
        assert_eq!(parse("3").unwrap(), 3);
        assert_eq!(
            parse("x").unwrap_err().to_string(),
            "invalid digit found in string"
        );
        assert_eq!(parse("12").unwrap_err().to_string(), "12 is not a digit");
        assert_eq!(parse("7").unwrap_err().to_string(), "not 7");
        let e = parse("x").context("reading the count").unwrap_err();
        assert_eq!(
            format!("{e:?}"),
            "reading the count: invalid digit found in string"
        );
        let e = parse("x")
            .with_context(|| format!("line {}", 4))
            .context("the file")
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            "the file: line 4: invalid digit found in string"
        );
    }

    #[test]
    fn nothing_there_is_an_error_with_only_the_context_to_say() {
        assert_eq!(
            None::<u8>.context("no version").unwrap_err().to_string(),
            "no version"
        );
        assert_eq!(Some(1).with_context(|| "unused").unwrap(), 1);
    }
}

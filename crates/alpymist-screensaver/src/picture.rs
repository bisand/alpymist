//! Which screensaver to show.
//!
//! A name, or `random`. The name is not checked when the settings file is read:
//! a screensaver is a package like any other, and one named here may have been
//! removed since, or may be about to be installed. It is resolved against what
//! is actually there when the screensaver comes on, and what to do about a name
//! that resolves to nothing is the launcher's business, not this module's.

use crate::definition::Definition;

/// The name that means "not one of them in particular".
pub const RANDOM: &str = "random";

/// What to show when the screensaver comes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    /// This one, every time.
    One(String),
    /// A different one each time it appears.
    Random,
}

impl Show {
    /// What the settings file writes.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::One(id) => id,
            Self::Random => RANDOM,
        }
    }

    /// What a name in the settings file asks for.
    #[must_use]
    pub fn of(id: &str) -> Self {
        if id == RANDOM {
            Self::Random
        } else {
            Self::One(id.to_owned())
        }
    }

    /// The one to draw now, out of those installed.
    ///
    /// `None` when there are none at all, or when this names one that is not
    /// there — which the caller should say out loud rather than quietly drawing
    /// something else, since somebody chose that name on purpose.
    #[must_use]
    pub fn resolve<'a>(&self, installed: &'a [Definition]) -> Option<&'a Definition> {
        match self {
            Self::One(id) => installed.iter().find(|d| &d.id == id),
            Self::Random => installed.get(pick(installed.len())),
        }
    }
}

impl Default for Show {
    /// A different one each time.
    ///
    /// The default rather than any particular screensaver, because no screensaver
    /// is part of this crate any more: naming one here would be this crate
    /// deciding which package you had installed.
    fn default() -> Self {
        Self::Random
    }
}

/// One of `count`, near enough at random.
///
/// The clock, rather than a random number generator: a screensaver appears at
/// most a few times an hour, so all this has to avoid is choosing the same one
/// every time, and a dependency to do that would be a dependency in every
/// package that reads these settings.
fn pick(count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    (nanos as usize) % count
}

#[cfg(test)]
mod tests {
    use super::{RANDOM, Show};
    use crate::definition::Definition;

    fn installed(ids: &[&str]) -> Vec<Definition> {
        ids.iter()
            .map(|id| Definition::parse(id, "name = \"X\"\nexec = \"x\"\n").expect("a definition"))
            .collect()
    }

    #[test]
    fn a_name_reads_back_as_what_it_names() {
        assert_eq!(Show::of("mountains").id(), "mountains");
        assert_eq!(Show::of(RANDOM), Show::Random);
        assert_eq!(Show::of(RANDOM).id(), RANDOM);
    }

    #[test]
    fn asking_for_one_gives_that_one() {
        let all = installed(&["mountains", "stars"]);
        let chosen = Show::of("stars").resolve(&all).expect("it is installed");
        assert_eq!(chosen.id, "stars");
    }

    #[test]
    fn asking_for_one_that_is_not_installed_gives_nothing() {
        let all = installed(&["mountains"]);
        assert!(
            Show::of("aquarium").resolve(&all).is_none(),
            "quietly showing the mountains would hide that it had gone"
        );
    }

    #[test]
    fn random_gives_one_that_is_installed() {
        let all = installed(&["mountains", "stars", "aquarium"]);
        for _ in 0..50 {
            let chosen = Show::Random.resolve(&all).expect("one of them");
            assert!(all.iter().any(|d| d.id == chosen.id));
        }
    }

    #[test]
    fn nothing_installed_resolves_to_nothing_rather_than_panicking() {
        assert!(Show::Random.resolve(&[]).is_none());
        assert!(Show::of("mountains").resolve(&[]).is_none());
    }

    #[test]
    fn the_default_names_no_particular_package() {
        assert_eq!(Show::default(), Show::Random);
    }
}

//! Release channels: which Alpymist repository a system follows.
//!
//! A channel is nothing more than the one Alpymist line in
//! `/etc/apk/repositories`, and, for dev, the key that line is signed with.
//! Upgrading is `apk upgrade`, whichever channel it is (ADR 0006).

use std::fmt;
use std::str::FromStr;

/// A repository of Alpymist packages a system can follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Released packages, whose index is signed by hand with the offline key.
    Stable,
    /// Every push to main, whose index CI signs with a key of its own.
    Dev,
}

impl Channel {
    /// Every channel, stable first.
    pub const ALL: [Self; 2] = [Self::Stable, Self::Dev];

    /// Its name, as `alpymistctl channel` takes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Dev => "dev",
        }
    }

    /// The line `/etc/apk/repositories` has for it.
    #[must_use]
    pub const fn repository(self) -> &'static str {
        match self {
            Self::Stable => "https://pkgs.alpymist.org/v3.24/alpymist",
            Self::Dev => "https://dev.pkgs.alpymist.org/v3.24/alpymist",
        }
    }

    /// The public key its index is signed with, when that key is not trusted
    /// on every system. Only dev's: `alpymist-keys` installs it to
    /// [`Channel::SHIPPED_KEYS`], and it is copied to `/etc/apk/keys` only
    /// while the system follows dev, because apk trusts every key there for
    /// every repository.
    #[must_use]
    pub const fn opt_in_key(self) -> Option<&'static str> {
        match self {
            Self::Stable => None,
            Self::Dev => Some("alpymist-dev-2026.rsa.pub"),
        }
    }

    /// Where `alpymist-keys` keeps the keys that are trusted only on request.
    pub const SHIPPED_KEYS: &'static str = "/usr/share/alpymist/keys";

    /// The channel a repositories file follows: its first Alpymist line that
    /// is not commented out. `None` when there is none.
    #[must_use]
    pub fn of_repositories(text: &str) -> Option<Self> {
        text.lines().find_map(Self::of_line)
    }

    /// `text` following `self`: the first Alpymist line replaced by this
    /// channel's, keeping any `@tag`, and any other Alpymist lines dropped.
    /// Appended when there was none. Everything else is left as it was.
    #[must_use]
    pub fn rewrite_repositories(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + self.repository().len());
        let mut written = false;
        for line in text.lines() {
            if Self::of_line(line).is_some() {
                if !written {
                    if let Some(tag) = line
                        .split_whitespace()
                        .next()
                        .filter(|w| w.starts_with('@'))
                    {
                        out.push_str(tag);
                        out.push(' ');
                    }
                    out.push_str(self.repository());
                    out.push('\n');
                    written = true;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        if !written {
            out.push_str(self.repository());
            out.push('\n');
        }
        out
    }

    /// The channel one line of a repositories file names, if it is an
    /// Alpymist repository and not commented out.
    fn of_line(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        let mut url = words.next()?;
        if url.starts_with('@') {
            url = words.next()?;
        }
        let url = url.trim_end_matches('/');
        Self::ALL.into_iter().find(|c| c.repository() == url)
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Channel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.name() == s)
            .ok_or_else(|| format!("`{s}` is not a channel: stable or dev"))
    }
}

#[cfg(test)]
mod tests {
    use super::Channel;

    const INSTALLED: &str = "https://dl-cdn.alpinelinux.org/alpine/v3.24/main\n\
                             https://dl-cdn.alpinelinux.org/alpine/v3.24/community\n\
                             https://pkgs.alpymist.org/v3.24/alpymist\n";

    #[test]
    fn an_installed_system_follows_stable() {
        assert_eq!(Channel::of_repositories(INSTALLED), Some(Channel::Stable));
    }

    #[test]
    fn switching_to_dev_replaces_only_the_alpymist_line() {
        let dev = Channel::Dev.rewrite_repositories(INSTALLED);
        assert_eq!(dev, INSTALLED.replace("://pkgs.", "://dev.pkgs."));
        assert_eq!(Channel::of_repositories(&dev), Some(Channel::Dev));
        assert_eq!(Channel::Stable.rewrite_repositories(&dev), INSTALLED);
    }

    #[test]
    fn switching_to_the_same_channel_changes_nothing() {
        assert_eq!(Channel::Stable.rewrite_repositories(INSTALLED), INSTALLED);
    }

    #[test]
    fn a_commented_line_is_not_followed_and_is_kept() {
        let text = "#https://pkgs.alpymist.org/v3.24/alpymist\n";
        assert_eq!(Channel::of_repositories(text), None);
        assert_eq!(
            Channel::Dev.rewrite_repositories(text),
            format!("{text}https://dev.pkgs.alpymist.org/v3.24/alpymist\n")
        );
    }

    #[test]
    fn both_lines_become_one_and_a_tag_is_kept() {
        let text = "@a https://pkgs.alpymist.org/v3.24/alpymist/\nx\n\
                    https://dev.pkgs.alpymist.org/v3.24/alpymist\n";
        assert_eq!(Channel::of_repositories(text), Some(Channel::Stable));
        assert_eq!(
            Channel::Dev.rewrite_repositories(text),
            "@a https://dev.pkgs.alpymist.org/v3.24/alpymist\nx\n"
        );
    }

    /// `alpymistctl channel dev` copies the key from where the package puts it.
    #[test]
    fn alpymist_keys_ships_dev_key_outside_apks_keys() {
        let apkbuild = include_str!("../../../aports/alpymist-keys/APKBUILD");
        let key = Channel::Dev.opt_in_key().unwrap();
        assert!(apkbuild.contains(&format!("\"$pkgdir\"{}/{key}", Channel::SHIPPED_KEYS)));
        assert!(!apkbuild.contains(&format!("\"$pkgdir\"/etc/apk/keys/{key}")));
        assert!(
            include_str!("../../../aports/alpymist-keys/alpymist-dev-2026.rsa.pub")
                .starts_with("-----BEGIN PUBLIC KEY-----")
        );
    }

    #[test]
    fn names_round_trip() {
        for c in Channel::ALL {
            assert_eq!(c.name().parse(), Ok(c));
        }
        assert!("edge".parse::<Channel>().is_err());
    }
}

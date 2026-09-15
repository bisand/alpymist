//! What the store knows about: every entry from every source.
//!
//! Entries are kept per source, so reloading one source — after a refresh,
//! or once its catalogue has been fetched for the first time — replaces its
//! list and leaves the others' alone. An entry is named across the store by
//! an [`At`]: which source, and where in its list.

use std::path::PathBuf;

/// Where an entry is: its source, and its index in that source's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct At {
    /// Index into the configured sources.
    pub source: u16,
    /// Index into that source's entries.
    pub index: u32,
}

/// A category, as Flathub files applications under the freedesktop.org main
/// categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// `AudioVideo`, `Audio`, `Video`.
    AudioVideo,
    /// `Development`.
    Development,
    /// `Education`.
    Education,
    /// `Game`.
    Games,
    /// `Graphics`.
    Graphics,
    /// `Network`.
    Network,
    /// `Office`.
    Office,
    /// `Science`.
    Science,
    /// `System`.
    System,
    /// `Utility`.
    Utilities,
}

impl Category {
    /// Every category, in the order the sidebar lists them.
    pub const ALL: [Self; 10] = [
        Self::AudioVideo,
        Self::Development,
        Self::Education,
        Self::Games,
        Self::Graphics,
        Self::Network,
        Self::Office,
        Self::Science,
        Self::System,
        Self::Utilities,
    ];

    /// The category a freedesktop.org category name belongs to.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "AudioVideo" | "Audio" | "Video" => Self::AudioVideo,
            "Development" => Self::Development,
            "Education" => Self::Education,
            "Game" => Self::Games,
            "Graphics" => Self::Graphics,
            "Network" => Self::Network,
            "Office" => Self::Office,
            "Science" => Self::Science,
            "System" => Self::System,
            "Utility" => Self::Utilities,
            _ => return None,
        })
    }

    /// Its bit in [`Entry::categories`].
    #[must_use]
    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// What the sidebar calls it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AudioVideo => "Audio & Video",
            Self::Development => "Development",
            Self::Education => "Education",
            Self::Games => "Games",
            Self::Graphics => "Graphics",
            Self::Network => "Networking",
            Self::Office => "Office",
            Self::Science => "Science",
            Self::System => "System",
            Self::Utilities => "Utilities",
        }
    }

    /// Its glyph, from Nerd Font's Material Design set.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::AudioVideo => "\u{f0fce}",
            Self::Development => "\u{f0169}",
            Self::Education => "\u{f0474}",
            Self::Games => "\u{f0297}",
            Self::Graphics => "\u{f03d8}",
            Self::Network => "\u{f059f}",
            Self::Office => "\u{f0219}",
            Self::Science => "\u{f0093}",
            Self::System => "\u{f0493}",
            Self::Utilities => "\u{f1064}",
        }
    }
}

/// Whether an entry is installed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum State {
    /// Not installed.
    #[default]
    Available,
    /// Installed at a version.
    Installed {
        /// The installed version, where the source says.
        version: String,
        /// Whether it was asked for, rather than pulled in by something
        /// else. Only what was asked for is listed as installed.
        explicit: bool,
        /// A newer version to update to, when there is one.
        update: Option<String>,
    },
}

impl State {
    /// Whether it is installed at all.
    #[must_use]
    pub fn installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }

    /// Whether there is an update for it.
    #[must_use]
    pub fn has_update(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                update: Some(_),
                ..
            }
        )
    }
}

/// One thing a source offers.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    /// The source's name for it: a Flatpak application ID, an apk package
    /// name.
    pub id: String,
    /// What it is called.
    pub name: String,
    /// One line on what it is.
    pub summary: String,
    /// Paragraphs on what it is, separated by blank lines.
    pub description: String,
    /// Who makes it.
    pub developer: String,
    /// The version the source offers.
    pub version: String,
    /// Its licence, as SPDX.
    pub license: String,
    /// Its website.
    pub homepage: String,
    /// Where in the source it comes from: an apk repository.
    pub origin: String,
    /// [`Category`] bits.
    pub categories: u16,
    /// Extra words to find it by, lowercase, space-separated.
    pub keywords: String,
    /// A small icon, 64 pixels square, and a large one, where the source has
    /// them.
    pub icon: Option<PathBuf>,
    /// A large icon, 128 pixels square.
    pub icon_large: Option<PathBuf>,
    /// Bytes to download, where known.
    pub download_size: Option<u64>,
    /// Bytes installed, where known.
    pub installed_size: Option<u64>,
    /// When the newest release came out, seconds since the epoch.
    pub released: u64,
    /// An application with a window, rather than a library or a tool.
    pub app: bool,
    /// Left out of results unless named exactly.
    pub hidden: bool,
    /// Cannot be removed from the store.
    pub protected: bool,
    /// Installed or not.
    pub state: State,
    /// `name`, lowercase, for search.
    pub name_lc: String,
    /// `summary`, lowercase, for search.
    pub summary_lc: String,
}

impl Entry {
    /// Fill the lowercase copies search reads. Sources call this once an
    /// entry is complete.
    pub fn index(&mut self) {
        self.name_lc = self.name.to_lowercase();
        self.summary_lc = self.summary.to_lowercase();
        self.keywords = self.keywords.to_lowercase();
    }
}

/// Every source's entries.
#[derive(Debug, Default)]
pub struct Catalog {
    lists: Vec<Vec<Entry>>,
}

impl Catalog {
    /// An empty catalogue for `sources` sources.
    #[must_use]
    pub fn new(sources: usize) -> Self {
        Self {
            lists: (0..sources).map(|_| Vec::new()).collect(),
        }
    }

    /// Replace a source's entries.
    pub fn replace(&mut self, source: u16, entries: Vec<Entry>) {
        if let Some(list) = self.lists.get_mut(usize::from(source)) {
            *list = entries;
        }
    }

    /// An entry.
    #[must_use]
    pub fn get(&self, at: At) -> Option<&Entry> {
        self.lists
            .get(usize::from(at.source))?
            .get(usize::try_from(at.index).ok()?)
    }

    /// A source's entries.
    #[must_use]
    pub fn list(&self, source: u16) -> &[Entry] {
        self.lists
            .get(usize::from(source))
            .map_or(&[], Vec::as_slice)
    }

    /// A source's entries, to change their state.
    pub fn list_mut(&mut self, source: u16) -> &mut [Entry] {
        self.lists
            .get_mut(usize::from(source))
            .map_or(&mut [], Vec::as_mut_slice)
    }

    /// How many sources there are.
    #[must_use]
    pub fn sources(&self) -> u16 {
        u16::try_from(self.lists.len()).unwrap_or(u16::MAX)
    }

    /// Every entry with where it is.
    pub fn iter(&self) -> impl Iterator<Item = (At, &Entry)> {
        self.lists.iter().enumerate().flat_map(|(s, list)| {
            list.iter().enumerate().map(move |(i, e)| {
                (
                    At {
                        source: u16::try_from(s).unwrap_or(u16::MAX),
                        index: u32::try_from(i).unwrap_or(u32::MAX),
                    },
                    e,
                )
            })
        })
    }

    /// Find an entry by its source and id.
    #[must_use]
    pub fn find(&self, source: u16, id: &str) -> Option<At> {
        let index = self.list(source).iter().position(|e| e.id == id)?;
        Some(At {
            source,
            index: u32::try_from(index).ok()?,
        })
    }

    /// Apply what a source says is installed: everything listed is
    /// installed, everything else is not.
    pub fn set_installed(&mut self, source: u16, installed: &[Installed]) {
        let map: std::collections::HashMap<&str, &Installed> =
            installed.iter().map(|i| (i.id.as_str(), i)).collect();
        for entry in self.list_mut(source) {
            entry.state = match map.get(entry.id.as_str()) {
                Some(i) => State::Installed {
                    version: i.version.clone(),
                    explicit: i.explicit,
                    update: i.update.clone(),
                },
                None => State::Available,
            };
        }
    }
}

/// Something a source says is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Its id.
    pub id: String,
    /// The installed version.
    pub version: String,
    /// Asked for, rather than pulled in.
    pub explicit: bool,
    /// A newer version, when there is one.
    pub update: Option<String>,
}

/// A size in bytes as people read it: 2.7 MB.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::{At, Catalog, Category, Entry, Installed, human_size};

    #[test]
    fn sizes_read_like_a_file_manager_says_them() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2_700_000), "2.7 MB");
        assert_eq!(human_size(45_700_000), "46 MB");
        assert_eq!(human_size(1_200_000_000), "1.2 GB");
    }

    #[test]
    fn categories_fold_the_sub_categories_in() {
        assert_eq!(Category::from_name("Audio"), Some(Category::AudioVideo));
        assert_eq!(Category::from_name("GTK"), None);
        let bits: u16 = Category::ALL.iter().map(|c| c.bit()).fold(0, |a, b| a | b);
        assert_eq!(bits.count_ones(), 10);
    }

    #[test]
    fn installed_state_follows_the_source() {
        let mut catalog = Catalog::new(2);
        let entry = |id: &str| Entry {
            id: id.into(),
            ..Entry::default()
        };
        catalog.replace(1, vec![entry("a"), entry("b")]);
        catalog.set_installed(
            1,
            &[Installed {
                id: "b".into(),
                version: "1".into(),
                explicit: true,
                update: Some("2".into()),
            }],
        );
        let b = catalog.find(1, "b").unwrap();
        assert_eq!(
            b,
            At {
                source: 1,
                index: 1
            }
        );
        assert!(catalog.get(b).unwrap().state.has_update());
        catalog.set_installed(1, &[]);
        assert!(!catalog.get(b).unwrap().state.installed());
        assert_eq!(catalog.iter().count(), 2);
    }
}

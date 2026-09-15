//! A Flatpak remote: Flathub, as the store ships.
//!
//! The catalogue is the remote's `AppStream` data, which `flatpak update
//! --appstream` keeps under the installation's `appstream/<remote>/<arch>/active`:
//! an XML file of every application, and its icons already cut to 64 and 128
//! pixels. Reading that is all a search needs — no web API, no network.
//! What is installed, and everything that changes something, goes through the
//! `flatpak` command, which is what the terminal uses too.

use crate::catalog::{Category, Entry, Installed};
use crate::config::{self, Installation};
use crate::source::{Op, Source, output, run_command};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A Flatpak remote.
pub struct Flatpak {
    settings: config::Flatpak,
}

impl Flatpak {
    /// The remote described by `settings`.
    #[must_use]
    pub fn new(settings: config::Flatpak) -> Self {
        Self { settings }
    }

    fn flatpak(&self) -> Command {
        let mut command = Command::new("flatpak");
        command.arg(self.settings.installation.flag());
        command
    }

    /// Where the remote's `AppStream` data is.
    fn appstream_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.settings.appstream {
            return Some(dir.clone());
        }
        let base = match self.settings.installation {
            Installation::User => std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))?
                .join("flatpak"),
            Installation::System => PathBuf::from("/var/lib/flatpak"),
        };
        Some(
            base.join("appstream")
                .join(&self.settings.remote)
                .join(std::env::consts::ARCH)
                .join("active"),
        )
    }

    fn remote_exists(&self) -> bool {
        let mut command = self.flatpak();
        command.args(["remotes", "--columns=name"]);
        output(command).is_ok_and(|text| text.lines().any(|l| l.trim() == self.settings.remote))
    }
}

impl Source for Flatpak {
    fn prepare(&self) -> Result<(), String> {
        if self.remote_exists() {
            return Ok(());
        }
        let Some(url) = &self.settings.url else {
            return Err(format!(
                "there is no remote called {} to install from",
                self.settings.remote
            ));
        };
        let mut command = self.flatpak();
        command.args(["remote-add", "--if-not-exists", &self.settings.remote, url]);
        output(command).map(|_| ())
    }

    fn has_catalog(&self) -> bool {
        self.appstream_dir().is_some_and(|d| {
            d.join("appstream.xml").exists() || d.join("appstream.xml.gz").exists()
        })
    }

    fn load(&self) -> Result<Vec<Entry>, String> {
        let dir = self
            .appstream_dir()
            .ok_or("there is no home directory to find Flatpak's data in")?;
        let xml = read_appstream(&dir)?;
        Ok(parse_appstream(&xml, &dir))
    }

    fn installed(&self) -> Result<Vec<Installed>, String> {
        let mut command = self.flatpak();
        command.args(["list", "--app", "--columns=application,version,origin"]);
        let listed = output(command)?;
        let mut updates = self.flatpak();
        updates.args([
            "remote-ls",
            "--updates",
            "--app",
            "--columns=application,version",
        ]);
        updates.arg(&self.settings.remote);
        // No updates is better than no list at all: this one may need the
        // network, and fails without it.
        let updates = output(updates).unwrap_or_default();
        Ok(parse_installed(&listed, &updates, &self.settings.remote))
    }

    fn run(&self, op: &Op, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
        let mut command = self.flatpak();
        match op {
            Op::Install(id) => {
                command.args([
                    "install",
                    "--noninteractive",
                    "-y",
                    &self.settings.remote,
                    id,
                ]);
            }
            Op::Remove(id) => {
                command.args(["uninstall", "--noninteractive", "-y", id]);
            }
            Op::Update(id) => {
                command.args(["update", "--noninteractive", "-y"]);
                command.args(id);
            }
            Op::Refresh => {
                self.prepare()?;
                command.args(["update", "--appstream", &self.settings.remote]);
            }
        }
        run_command(command, progress)
    }

    fn launch(&self, id: &str) -> Option<Command> {
        let mut command = Command::new("flatpak");
        command.args(["run", id]);
        Some(command)
    }
}

/// The `AppStream` XML, from the plain file Flatpak keeps beside the
/// compressed one, or from the compressed one.
fn read_appstream(dir: &Path) -> Result<Vec<u8>, String> {
    if let Ok(bytes) = std::fs::read(dir.join("appstream.xml")) {
        return Ok(bytes);
    }
    let path = dir.join("appstream.xml.gz");
    let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(file)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(bytes)
}

/// What `flatpak list` and `flatpak remote-ls --updates` said, as installed
/// entries from `remote`.
fn parse_installed(listed: &str, updates: &str, remote: &str) -> Vec<Installed> {
    let updates: std::collections::HashMap<&str, &str> = updates
        .lines()
        .filter_map(|l| {
            let mut cols = l.split('\t');
            Some((cols.next()?.trim(), cols.next().unwrap_or("").trim()))
        })
        .collect();
    listed
        .lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let id = cols.next()?.trim();
            let version = cols.next().unwrap_or("").trim();
            let origin = cols.next().unwrap_or("").trim();
            if id.is_empty() || origin != remote {
                return None;
            }
            Some(Installed {
                id: id.to_owned(),
                version: version.to_owned(),
                explicit: true,
                update: updates.get(id).map(|v| {
                    if v.is_empty() {
                        "a newer build".to_owned()
                    } else {
                        (*v).to_owned()
                    }
                }),
            })
        })
        .collect()
}

/// An element whose text or attributes the parser keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Component,
    Id,
    Name,
    Summary,
    Description,
    Paragraph,
    Item,
    Developer,
    DeveloperName,
    License,
    Homepage,
    Categories,
    Category,
    Keywords,
    Keyword,
    Icon64,
    Icon128,
    Releases,
    Bundle,
    /// Anything else, kept on the stack so ends match.
    Other,
    /// A subtree nothing is read from: translations, screenshots, ratings.
    Skip,
}

/// What is being built from one `<component>`.
#[derive(Default)]
struct Building {
    entry: Entry,
    wanted: bool,
    bundle: String,
    icon64: String,
    icon128: String,
    released: bool,
    /// The description's last block was a list item.
    in_list: bool,
}

/// Parse `AppStream` XML into entries, with icon paths under `dir`.
///
/// Only applications are kept, desktop and console: runtimes, extensions
/// and fonts are Flatpak's business, not something to pick from a list.
/// Translated text is skipped; the untranslated original is what is shown.
#[must_use]
pub fn parse_appstream(xml: &[u8], dir: &Path) -> Vec<Entry> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().check_end_names = false;
    let mut entries = Vec::with_capacity(4096);
    let mut stack: Vec<Tag> = Vec::with_capacity(16);
    let mut current: Option<Building> = None;
    let mut text = String::new();
    let icons64 = dir.join("icons/64x64");
    let icons128 = dir.join("icons/128x128");

    loop {
        let event = match reader.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(event) => event,
        };
        let skipping = stack.contains(&Tag::Skip);
        match event {
            Event::Start(start) => {
                let tag = if skipping {
                    Tag::Skip
                } else {
                    open_tag(&start, stack.last().copied(), current.as_mut())
                };
                if tag == Tag::Component {
                    let kind = attr(&start, b"type").unwrap_or_default();
                    current = Some(Building {
                        wanted: kind == "desktop-application" || kind == "desktop",
                        ..Building::default()
                    });
                }
                if matches!(
                    tag,
                    Tag::Id
                        | Tag::Name
                        | Tag::Summary
                        | Tag::Paragraph
                        | Tag::Item
                        | Tag::DeveloperName
                        | Tag::License
                        | Tag::Homepage
                        | Tag::Category
                        | Tag::Keyword
                        | Tag::Icon64
                        | Tag::Icon128
                        | Tag::Bundle
                ) {
                    text.clear();
                }
                stack.push(tag);
            }
            Event::Empty(start) => {
                if !skipping {
                    let _ = open_tag(&start, stack.last().copied(), current.as_mut());
                }
            }
            Event::Text(t) if !skipping && collecting(&stack) => {
                if let Ok(s) = t.xml10_content() {
                    text.push_str(&s);
                }
            }
            Event::CData(t) if !skipping && collecting(&stack) => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Event::GeneralRef(r) if !skipping && collecting(&stack) => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    text.push(ch);
                } else if let Ok(name) = r.decode() {
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::End(_) => {
                let Some(tag) = stack.pop() else { continue };
                let Some(building) = current.as_mut() else {
                    continue;
                };
                close_tag(tag, building, &mut text);
                if tag == Tag::Component
                    && let Some(done) = current.take()
                    && let Some(entry) = finish(done, &icons64, &icons128)
                {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
    entries
}

/// Whether text at the top of `stack` is being kept.
fn collecting(stack: &[Tag]) -> bool {
    stack.iter().rev().any(|t| {
        matches!(
            t,
            Tag::Id
                | Tag::Name
                | Tag::Summary
                | Tag::Paragraph
                | Tag::Item
                | Tag::DeveloperName
                | Tag::License
                | Tag::Homepage
                | Tag::Category
                | Tag::Keyword
                | Tag::Icon64
                | Tag::Icon128
                | Tag::Bundle
        )
    })
}

fn attr(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    let a = start.try_get_attribute(name).ok()??;
    // AppStream attributes are plain words and numbers: nothing to unescape.
    Some(String::from_utf8_lossy(&a.value).into_owned())
}

fn translated(start: &BytesStart<'_>) -> bool {
    start
        .try_get_attribute(b"xml:lang")
        .ok()
        .flatten()
        .is_some_and(|a| !a.value.is_empty() && a.value.as_ref() != b"C")
}

/// What an opening tag is, given its parent; attributes that carry the
/// value — a release's version — are read here.
fn open_tag(start: &BytesStart<'_>, parent: Option<Tag>, current: Option<&mut Building>) -> Tag {
    let name = start.local_name();
    let name = name.as_ref();
    match (parent, name) {
        // The document, a list in a description, and markup in a paragraph
        // are gone through for what is inside them.
        (None, b"components")
        | (Some(Tag::Description), b"ul" | b"ol")
        | (Some(Tag::Paragraph | Tag::Item), _) => Tag::Other,
        // Kept or not is decided from its type, where it is opened.
        (None | Some(Tag::Other), b"component") => Tag::Component,
        (Some(Tag::Component), b"id") => Tag::Id,
        (Some(Tag::Component), b"name") if !translated(start) => Tag::Name,
        (Some(Tag::Component), b"summary") if !translated(start) => Tag::Summary,
        (Some(Tag::Component), b"description") if !translated(start) => Tag::Description,
        (Some(Tag::Description), b"p") if !translated(start) => Tag::Paragraph,
        (Some(Tag::Other), b"li") if !translated(start) => Tag::Item,
        (Some(Tag::Component), b"developer") => Tag::Developer,
        (Some(Tag::Developer), b"name") | (Some(Tag::Component), b"developer_name")
            if !translated(start) =>
        {
            Tag::DeveloperName
        }
        (Some(Tag::Component), b"project_license") => Tag::License,
        (Some(Tag::Component), b"url") if attr(start, b"type").as_deref() == Some("homepage") => {
            Tag::Homepage
        }
        (Some(Tag::Component), b"categories") => Tag::Categories,
        (Some(Tag::Categories), b"category") => Tag::Category,
        (Some(Tag::Component), b"keywords") if !translated(start) => Tag::Keywords,
        (Some(Tag::Keywords), b"keyword") if !translated(start) => Tag::Keyword,
        (Some(Tag::Component), b"icon") => {
            let cached = attr(start, b"type").as_deref() == Some("cached");
            let scaled = attr(start, b"scale").is_some_and(|s| s != "1");
            match attr(start, b"height").as_deref() {
                Some("64") if cached && !scaled => Tag::Icon64,
                Some("128") if cached && !scaled => Tag::Icon128,
                _ => Tag::Other,
            }
        }
        (Some(Tag::Component), b"releases") => Tag::Releases,
        (Some(Tag::Releases), b"release") => {
            if let Some(b) = current
                && !b.released
            {
                b.released = true;
                b.entry.version = attr(start, b"version").unwrap_or_default();
                b.entry.released = attr(start, b"timestamp")
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
            }
            Tag::Skip
        }
        (Some(Tag::Component), b"bundle") if attr(start, b"type").as_deref() == Some("flatpak") => {
            Tag::Bundle
        }
        // Translations, screenshots, ratings and the rest: nothing is read
        // from them, so nothing inside them is looked at.
        _ => Tag::Skip,
    }
}

/// Whitespace as it would be displayed: runs collapsed, ends trimmed.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

fn close_tag(tag: Tag, b: &mut Building, text: &mut String) {
    let value = || collapse(text);
    let in_list = b.in_list;
    if matches!(tag, Tag::Paragraph | Tag::Item) {
        b.in_list = tag == Tag::Item;
    }
    let e = &mut b.entry;
    match tag {
        Tag::Id => e.id = value(),
        Tag::Name => e.name = value(),
        Tag::Summary => e.summary = value(),
        Tag::Paragraph => {
            if !e.description.is_empty() {
                e.description.push_str("\n\n");
            }
            e.description.push_str(&value());
        }
        Tag::Item => {
            if !e.description.is_empty() {
                e.description.push_str(if in_list { "\n" } else { "\n\n" });
            }
            e.description.push_str("• ");
            e.description.push_str(&value());
        }
        Tag::DeveloperName => {
            if e.developer.is_empty() {
                e.developer = value();
            }
        }
        Tag::License => e.license = value(),
        Tag::Homepage => e.homepage = value(),
        Tag::Category => {
            if let Some(c) = Category::from_name(text.trim()) {
                e.categories |= c.bit();
            }
        }
        Tag::Keyword => {
            if !e.keywords.is_empty() {
                e.keywords.push(' ');
            }
            e.keywords.push_str(&value());
        }
        Tag::Icon64 => b.icon64 = value(),
        Tag::Icon128 => b.icon128 = value(),
        Tag::Bundle => b.bundle = value(),
        _ => return,
    }
    text.clear();
}

/// The entry a component makes, if it is an application.
fn finish(b: Building, icons64: &Path, icons128: &Path) -> Option<Entry> {
    if !b.wanted {
        return None;
    }
    let mut e = b.entry;
    // `app/org.gimp.GIMP/aarch64/stable`: the ref's name is what `flatpak
    // install` takes, and older catalogues gave the component a `.desktop`
    // id instead.
    if let Some(id) = b.bundle.split('/').nth(1).filter(|id| !id.is_empty()) {
        e.id = id.to_owned();
    }
    if e.id.is_empty() || e.name.is_empty() {
        return None;
    }
    if !b.icon64.is_empty() {
        e.icon = Some(icons64.join(&b.icon64));
    }
    if !b.icon128.is_empty() {
        e.icon_large = Some(icons128.join(&b.icon128));
    }
    e.app = true;
    e.index();
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::{parse_appstream, parse_installed};
    use crate::catalog::Category;
    use std::path::Path;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<components version="0.8" origin="flatpak">
  <component type="desktop-application">
    <id>org.gimp.GIMP.desktop</id>
    <name>GNU Image Manipulation Program</name>
    <name xml:lang="nb">GNU bildebehandlingsprogram</name>
    <summary>Create images &amp; edit   photographs</summary>
    <summary xml:lang="de">Bilder erstellen</summary>
    <description>
      <p>GIMP is an <em>advanced</em> picture editor.</p>
      <p xml:lang="de">GIMP ist ein Bildbearbeitungsprogramm.</p>
      <ul><li>Layers</li><li><em>Filters</em></li></ul>
    </description>
    <developer id="org.gimp"><name>The GIMP team</name></developer>
    <project_license>GPL-3.0+</project_license>
    <url type="bugtracker">https://example.org/bugs</url>
    <url type="homepage">https://www.gimp.org/</url>
    <icon type="stock">org.gimp.GIMP</icon>
    <icon height="64" type="cached" width="64">org.gimp.GIMP.png</icon>
    <icon height="64" scale="2" type="cached" width="64">org.gimp.GIMP.png</icon>
    <icon height="128" type="cached" width="128">org.gimp.GIMP.png</icon>
    <categories><category>Graphics</category><category>2DGraphics</category></categories>
    <keywords><keyword>Photo</keyword><keyword xml:lang="de">Foto</keyword><keyword>paint</keyword></keywords>
    <screenshots><screenshot><caption>Main window</caption></screenshot></screenshots>
    <releases><release timestamp="1750000000" version="3.0.4"/><release version="3.0.2"/></releases>
    <bundle type="flatpak" runtime="org.gnome.Platform/aarch64/48">app/org.gimp.GIMP/aarch64/stable</bundle>
  </component>
  <component type="runtime">
    <id>org.gnome.Platform</id>
    <name>GNOME runtime</name>
  </component>
</components>"#;

    #[test]
    fn applications_are_read_and_runtimes_left_out() {
        let entries = parse_appstream(SAMPLE.as_bytes(), Path::new("/as"));
        assert_eq!(entries.len(), 1);
        let gimp = &entries[0];
        assert_eq!(gimp.id, "org.gimp.GIMP");
        assert_eq!(gimp.name, "GNU Image Manipulation Program");
        assert_eq!(gimp.summary, "Create images & edit photographs");
        assert_eq!(
            gimp.description,
            "GIMP is an advanced picture editor.\n\n• Layers\n• Filters"
        );
        assert_eq!(gimp.developer, "The GIMP team");
        assert_eq!(gimp.license, "GPL-3.0+");
        assert_eq!(gimp.homepage, "https://www.gimp.org/");
        assert_eq!(gimp.version, "3.0.4");
        assert_eq!(gimp.released, 1_750_000_000);
        assert_eq!(gimp.keywords, "photo paint");
        assert_eq!(gimp.categories, Category::Graphics.bit());
        assert_eq!(
            gimp.icon.as_deref(),
            Some(Path::new("/as/icons/64x64/org.gimp.GIMP.png"))
        );
        assert_eq!(
            gimp.icon_large.as_deref(),
            Some(Path::new("/as/icons/128x128/org.gimp.GIMP.png"))
        );
        assert_eq!(gimp.name_lc, "gnu image manipulation program");
    }

    #[test]
    fn installed_apps_from_other_remotes_are_not_ours() {
        let listed = "org.gimp.GIMP\t3.0.2\tflathub\norg.other.App\t1\tgnome-nightly\n";
        let updates = "org.gimp.GIMP\t3.0.4\n";
        let installed = parse_installed(listed, updates, "flathub");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].version, "3.0.2");
        assert_eq!(installed[0].update.as_deref(), Some("3.0.4"));
    }
}

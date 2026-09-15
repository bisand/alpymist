//! Alpine's repositories, through apk.
//!
//! The catalogue is apk's own index, read where apk caches it: one
//! `APKINDEX.<hash>.tar.gz` per line of `/etc/apk/repositories`, the hash
//! being the first eight hex digits of the SHA-1 of the index's URL. Each is
//! a signature and an index, two gzip streams back to back, and the index a
//! tar file holding a text file of `P:name`, `V:version` records. What is
//! installed is `/lib/apk/db/installed`, in the same format, and what was
//! asked for is `/etc/apk/world`. All of it can be read without root.
//!
//! Changing anything needs root, and goes through `alpymist-store-helper`
//! with pkexec, which accepts only package names and four verbs. polkit
//! decides who may and asks for the password, in the dialog the store's own
//! agent shows; see `alpymist-auth`.

use crate::catalog::{Entry, Installed};
use crate::config;
use crate::source::{Op, Source, run_command};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the helper is installed.
pub const HELPER: &str = "/usr/libexec/alpymist-store-helper";

/// The system's apk repositories.
pub struct Apk {
    settings: config::Apk,
}

impl Apk {
    /// The repositories under `settings.root`.
    #[must_use]
    pub fn new(settings: config::Apk) -> Self {
        Self { settings }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.settings.root.join(relative)
    }

    fn helper(verb: &str, names: &[&str]) -> Command {
        let mut command = Command::new("pkexec");
        command.args([HELPER, verb]);
        command.args(names);
        command
    }

    /// The repositories, in priority order, with the cached index of each.
    fn repositories(&self) -> Vec<Repository> {
        let arch = std::fs::read_to_string(self.path("etc/apk/arch")).map_or_else(
            |_| std::env::consts::ARCH.to_owned(),
            |a| a.trim().to_owned(),
        );
        let text = std::fs::read_to_string(self.path("etc/apk/repositories")).unwrap_or_default();
        parse_repositories(&text)
            .into_iter()
            .map(|(tag, url)| {
                let index = format!("{}/{arch}/APKINDEX.tar.gz", url.trim_end_matches('/'));
                let hash = sha1_smol::Sha1::from(index.as_bytes()).digest().to_string();
                let label = url
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_owned();
                Repository {
                    tag,
                    label,
                    cache: self.path(&format!("var/cache/apk/APKINDEX.{}.tar.gz", &hash[..8])),
                }
            })
            .collect()
    }
}

/// One line of `/etc/apk/repositories`.
struct Repository {
    /// `@testing`, for a tagged repository, whose packages are named with it.
    tag: Option<String>,
    /// Its last path segment: main, community, alpymist.
    label: String,
    /// Where apk caches its index.
    cache: PathBuf,
}

impl Source for Apk {
    fn has_catalog(&self) -> bool {
        self.repositories().iter().any(|r| r.cache.exists())
    }

    fn load(&self) -> Result<Vec<Entry>, String> {
        let repositories = self.repositories();
        if repositories.is_empty() {
            return Err("there are no repositories in /etc/apk/repositories".into());
        }
        let mut entries = Vec::with_capacity(32_000);
        let mut seen: HashSet<String> = HashSet::new();
        let mut problems = Vec::new();
        for repo in &repositories {
            let text = match read_index(&repo.cache) {
                Ok(text) => text,
                Err(e) => {
                    problems.push(e);
                    continue;
                }
            };
            for mut entry in parse_index(&text) {
                if let Some(tag) = &repo.tag {
                    entry.id = format!("{}@{tag}", entry.id);
                }
                // The first repository to offer a name is the one apk takes
                // it from; later ones only offer the same name again.
                if !seen.insert(entry.id.clone()) {
                    continue;
                }
                entry.origin.clone_from(&repo.label);
                let bare = entry.id.split('@').next().unwrap_or("");
                entry.hidden = self.settings.hide.iter().any(|p| p.matches(bare));
                entry.protected = self.settings.protect.iter().any(|p| p.matches(bare));
                entry.index();
                entries.push(entry);
            }
        }
        if entries.is_empty() && !problems.is_empty() {
            return Err(problems.join("; "));
        }
        Ok(entries)
    }

    fn installed(&self) -> Result<Vec<Installed>, String> {
        let db = std::fs::read_to_string(self.path("lib/apk/db/installed"))
            .map_err(|e| format!("/lib/apk/db/installed: {e}"))?;
        let world = std::fs::read_to_string(self.path("etc/apk/world")).unwrap_or_default();
        let mut available: HashMap<String, String> = HashMap::new();
        for repo in self.repositories() {
            if repo.tag.is_some() {
                continue;
            }
            if let Ok(text) = read_index(&repo.cache) {
                for (name, version) in names_and_versions(&text) {
                    let newest = available.entry(name.to_owned()).or_default();
                    if newest.is_empty() || compare_versions(version, newest).is_gt() {
                        version.clone_into(newest);
                    }
                }
            }
        }
        Ok(parse_installed(&db, &world, &available))
    }

    fn run(&self, op: &Op, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
        let command = match op {
            Op::Install(name) => Self::helper("add", &[name]),
            Op::Remove(name) => Self::helper("del", &[name]),
            Op::Update(Some(name)) => Self::helper("upgrade", &[name]),
            Op::Update(None) => Self::helper("upgrade", &[]),
            Op::Refresh => Self::helper("update", &[]),
        };
        run_command(command, progress).map_err(|e| explain(&e))
    }
}

/// pkexec's refusals, in words that say what happened.
fn explain(error: &str) -> String {
    if error.contains("dismissed") {
        "Cancelled".into()
    } else if error.contains("Not authorized") {
        "Not allowed: the password was not given, or this account is not an administrator".into()
    } else if error.contains("No authentication agent") {
        "Nothing could ask for the password: polkit did not reach the store".into()
    } else if error.contains("could not run pkexec") {
        "polkit is not installed, so system packages cannot be changed from here".into()
    } else {
        error.to_owned()
    }
}

/// `/etc/apk/repositories`: `(tag, url)` for each line that names one.
fn parse_repositories(text: &str) -> Vec<(Option<String>, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let first = words.next()?;
            if let Some(tag) = first.strip_prefix('@') {
                Some((Some(tag.to_owned()), words.next()?.to_owned()))
            } else {
                Some((None, first.to_owned()))
            }
        })
        .filter(|(_, url)| url.contains("://") || url.starts_with('/'))
        .collect()
}

/// The text of the `APKINDEX` inside a cached index.
fn read_index(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut tar = Vec::new();
    // The signature and the index are separate gzip streams; one tar file
    // when both are read through.
    flate2::read::MultiGzDecoder::new(std::io::BufReader::new(file))
        .read_to_end(&mut tar)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = tar_member(&tar, "APKINDEX")
        .ok_or_else(|| format!("{}: no APKINDEX inside", path.display()))?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// A member of a tar archive, by name.
fn tar_member<'a>(tar: &'a [u8], wanted: &str) -> Option<&'a [u8]> {
    let mut at = 0;
    while at + 512 <= tar.len() {
        let header = &tar[at..at + 512];
        if header.iter().all(|&b| b == 0) {
            // The signature's archive ends with zero blocks, and the index's
            // follows; keep looking.
            at += 512;
            continue;
        }
        let name_end = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let name = std::str::from_utf8(&header[..name_end]).unwrap_or("");
        let size_field = std::str::from_utf8(&header[124..136]).unwrap_or("");
        let size =
            usize::from_str_radix(size_field.trim_matches(|c: char| c == '\0' || c == ' '), 8)
                .ok()?;
        let data = at + 512;
        if name == wanted {
            return tar.get(data..data + size);
        }
        at = data + size.div_ceil(512) * 512;
    }
    None
}

/// Records of `K:value` lines, separated by blank lines.
fn records(text: &str) -> impl Iterator<Item = impl Iterator<Item = (u8, &str)>> {
    text.split("\n\n").map(|record| {
        record.lines().filter_map(|line| {
            let bytes = line.as_bytes();
            (bytes.len() >= 2 && bytes[1] == b':').then(|| (bytes[0], &line[2..]))
        })
    })
}

/// Entries from an index.
fn parse_index(text: &str) -> Vec<Entry> {
    records(text)
        .filter_map(|fields| {
            let mut e = Entry::default();
            for (key, value) in fields {
                match key {
                    b'P' => value.clone_into(&mut e.id),
                    b'V' => value.clone_into(&mut e.version),
                    b'T' => value.clone_into(&mut e.summary),
                    b'U' => value.clone_into(&mut e.homepage),
                    b'L' => value.clone_into(&mut e.license),
                    b'S' => e.download_size = value.parse().ok(),
                    b'I' => e.installed_size = value.parse().ok(),
                    b'm' => e.developer = maintainer(value),
                    b't' => e.released = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
            if e.id.is_empty() {
                return None;
            }
            e.name.clone_from(&e.id);
            Some(e)
        })
        .collect()
}

/// `Jane Doe <jane@example.org>` without the address.
fn maintainer(value: &str) -> String {
    value.split('<').next().unwrap_or(value).trim().to_owned()
}

/// Names and versions from an index or the installed database.
fn names_and_versions(text: &str) -> impl Iterator<Item = (&str, &str)> {
    records(text).filter_map(|fields| {
        let (mut name, mut version) = (None, None);
        for (key, value) in fields {
            match key {
                b'P' => name = Some(value),
                b'V' => version = Some(value),
                _ => {}
            }
        }
        Some((name?, version?))
    })
}

/// A world line's package name: `foo>=1.2` and `foo@testing` are `foo`.
fn world_name(line: &str) -> &str {
    let end = line.find(['=', '<', '>', '~', '@']).unwrap_or(line.len());
    line[..end].trim()
}

/// What is installed, given the database, the world file, and the newest
/// version of each name the repositories offer. Only a newer version is an
/// update: a package built locally, or from a repository since removed, can
/// be ahead of what the repositories have.
fn parse_installed(db: &str, world: &str, available: &HashMap<String, String>) -> Vec<Installed> {
    let world: HashSet<&str> = world
        .split_whitespace()
        .map(world_name)
        .filter(|n| !n.is_empty() && !n.starts_with('!'))
        .collect();
    names_and_versions(db)
        .map(|(name, version)| Installed {
            id: name.to_owned(),
            version: version.to_owned(),
            explicit: world.contains(name),
            update: available
                .get(name)
                .filter(|v| compare_versions(v, version).is_gt())
                .cloned(),
        })
        .collect()
}

/// Order two apk versions, as `apk version -t` does: numbers compared as
/// numbers part by part, then a letter, then suffixes — `_alpha`, `_beta`,
/// `_pre` and `_rc` before the release itself, `_cvs`, `_svn`, `_git`, `_hg`
/// and `_p` after it — then the `-r` revision.
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    version_key(a).cmp(&version_key(b))
}

/// A version as something that sorts: numbers, a letter, suffixes, revision.
fn version_key(version: &str) -> (Vec<u64>, u8, Vec<(u8, u64)>, u64) {
    let (base, revision) = match version.rsplit_once("-r") {
        Some((base, r)) if !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()) => {
            (base, r.parse().unwrap_or(0))
        }
        _ => (version, 0),
    };
    let (head, suffixes) = base.split_once('_').unwrap_or((base, ""));
    let mut numbers = Vec::new();
    let mut letter = 0;
    for part in head.split('.') {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        numbers.push(digits.parse().unwrap_or(0));
        if let Some(c) = part[digits.len()..].bytes().next() {
            letter = c;
        }
    }
    // A release with no suffix sorts between the pre-releases and the
    // patches: rank 4 of 9, appended last so a suffix always outranks it.
    let mut rest: Vec<(u8, u64)> = suffixes
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let name: String = s.chars().take_while(char::is_ascii_alphabetic).collect();
            let rank = match name.as_str() {
                "alpha" => 0,
                "beta" => 1,
                "pre" => 2,
                "rc" => 3,
                "cvs" => 5,
                "svn" => 6,
                "git" => 7,
                "hg" => 8,
                _ => 9,
            };
            (rank, s[name.len()..].parse().unwrap_or(0))
        })
        .collect();
    rest.push((4, 0));
    (numbers, letter, rest, revision)
}

#[cfg(test)]
mod tests {
    use super::{
        compare_versions, parse_index, parse_installed, parse_repositories, tar_member, world_name,
    };
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn versions_order_as_apk_orders_them() {
        assert_eq!(compare_versions("0.0.1-r10", "0.0.1-r8"), Greater);
        assert_eq!(compare_versions("1.10", "1.9"), Greater);
        assert_eq!(compare_versions("1.2_rc1", "1.2"), Less);
        assert_eq!(compare_versions("1.2_alpha", "1.2_beta"), Less);
        assert_eq!(compare_versions("1.2_p1", "1.2"), Greater);
        assert_eq!(compare_versions("1.2a", "1.2"), Greater);
        assert_eq!(compare_versions("1.2.1", "1.2"), Greater);
        assert_eq!(compare_versions("26.01-r0", "26.01-r0"), Equal);
        assert_eq!(compare_versions("5.9-r2", "5.9-r1"), Greater);
    }
    use std::collections::HashMap;

    const INDEX: &str = "C:Q1abc=\nP:7zip\nV:26.01-r0\nA:aarch64\nS:916236\nI:1902912\n\
T:File archiver with a high compression ratio\nU:https://7-zip.org/\nL:LGPL-2.0-only\n\
m:Jane Doe <jane@example.org>\nt:1781823476\n\nC:Q1def=\nP:7zip-doc\nV:26.01-r0\n\
T:File archiver (documentation)\n";

    #[test]
    fn index_records_become_entries() {
        let entries = parse_index(INDEX);
        assert_eq!(entries.len(), 2);
        let zip = &entries[0];
        assert_eq!(zip.id, "7zip");
        assert_eq!(zip.name, "7zip");
        assert_eq!(zip.version, "26.01-r0");
        assert_eq!(zip.download_size, Some(916_236));
        assert_eq!(zip.installed_size, Some(1_902_912));
        assert_eq!(zip.developer, "Jane Doe");
        assert_eq!(zip.released, 1_781_823_476);
    }

    #[test]
    fn repositories_keep_their_tags_and_skip_comments() {
        let text = "# main\nhttps://dl-cdn.alpinelinux.org/alpine/v3.24/main\n\n\
@testing https://dl-cdn.alpinelinux.org/alpine/edge/testing\n";
        let repos = parse_repositories(text);
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].0, None);
        assert_eq!(repos[1].0.as_deref(), Some("testing"));
    }

    #[test]
    fn world_names_lose_their_constraints() {
        assert_eq!(world_name("foo>=1.2"), "foo");
        assert_eq!(world_name("bar@testing"), "bar");
        assert_eq!(world_name("baz"), "baz");
    }

    #[test]
    fn installed_packages_know_whether_they_were_asked_for() {
        let db = "P:zsh\nV:5.9-r1\n\nP:musl\nV:1.2.5-r0\n\nP:alpymist-menu\nV:0.0.1-r4\n";
        let world = "zsh\nalpine-base\n";
        let available = HashMap::from([
            ("zsh".to_owned(), "5.9-r2".to_owned()),
            ("alpymist-menu".to_owned(), "0.0.1-r2".to_owned()),
        ]);
        let installed = parse_installed(db, world, &available);
        assert_eq!(installed.len(), 3);
        assert_eq!(
            installed[2].update, None,
            "built here, ahead of the repository"
        );
        assert!(installed[0].explicit);
        assert_eq!(installed[0].update.as_deref(), Some("5.9-r2"));
        assert!(!installed[1].explicit);
        assert_eq!(installed[1].update, None);
    }

    #[test]
    fn a_member_is_found_after_another_archive_ends() {
        let mut tar = Vec::new();
        let header = |name: &str, size: usize| {
            let mut h = vec![0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            let octal = format!("{size:011o}\0");
            h[124..136].copy_from_slice(octal.as_bytes());
            h
        };
        tar.extend(header(".SIGN.RSA.key", 3));
        tar.extend(b"sig".iter().copied().chain(std::iter::repeat_n(0, 509)));
        tar.extend(vec![0u8; 1024]);
        tar.extend(header("APKINDEX", 5));
        tar.extend(b"P:foo".iter().copied().chain(std::iter::repeat_n(0, 507)));
        assert_eq!(tar_member(&tar, "APKINDEX"), Some(&b"P:foo"[..]));
        assert_eq!(tar_member(&tar, "DESCRIPTION"), None);
    }
}

//! What the workspace depends on that is not ours.
//!
//! As little as possible, from crates.io, and well known (ADR 0023). That was
//! a habit; this makes it a check. `deps` fails when a locked package comes
//! from anywhere but crates.io, when a manifest names a third-party crate
//! that [`ALLOWED`] does not, when [`ALLOWED`] names one nothing uses any
//! more, and when the lockfile has grown past [`LOCKED`]. So a new dependency
//! is an edit here, with its reason beside it, and not a side effect of
//! another change.
//!
//! Both files are read a line at a time. `Cargo.lock` is written by cargo and
//! the manifests by us, and a TOML parser would be a dependency of the check
//! on dependencies.

use crate::error::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Where every third-party package has to come from.
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// The crates that are ours though they are not in this workspace: Denise,
/// from `bisand/denise`. Named one by one, since a prefix is anybody's to
/// publish under.
const OURS: [&str; 10] = [
    "denise",
    "denise-drm",
    "denise-evdev",
    "denise-fbdev",
    "denise-layout",
    "denise-macos",
    "denise-render",
    "denise-text",
    "denise-ui",
    "denise-winit",
];

/// Every third-party crate a manifest in this workspace may name, and what it
/// is for. Adding a line is the review: say why nothing of ours, and nothing
/// already here, does the job. Those with an issue beside them are to go.
const ALLOWED: [(&str, &str); 21] = [
    (
        "blocking",
        "one wait off the polkit agent's thread. To go, #115",
    ),
    (
        "bytemuck",
        "a shared buffer's bytes as pixels, without unsafe",
    ),
    ("clap", "the command line, and what completion is made from"),
    (
        "flate2",
        "apk's and Flathub's gzipped indexes, in pure Rust",
    ),
    (
        "jiff",
        "the login screen's clock in the machine's zone. #115",
    ),
    (
        "jpeg-encoder",
        "the boot menu's picture, at package build time",
    ),
    (
        "khronos-egl",
        "EGL's entry points for the GL ES probe. To go, #115",
    ),
    ("libloading", "opening libEGL at run time for that probe"),
    (
        "png",
        "the wallpaper, the store's icons, and every snapshot",
    ),
    ("quick-xml", "Flathub's AppStream catalogue"),
    ("rustix", "system calls std does not reach, without unsafe"),
    ("serde", "every settings file and every JSON answer"),
    ("serde_json", "Hyprland's answers, and the programs' own"),
    (
        "sha1_smol",
        "the names apk and Flatpak give cached files. To go, #115",
    ),
    (
        "signal-hook",
        "a signal handler on a bare console, without unsafe",
    ),
    ("smithay-client-toolkit", "Wayland surfaces and input. #113"),
    ("toml", "every settings file"),
    ("zbus", "iwd, fprintd and polkit on the system bus. #112"),
    ("zeroize", "a password wiped from memory once handed on"),
    ("zune-core", "zune-jpeg's options"),
    ("zune-jpeg", "decoding the boot picture, in pure Rust"),
];

/// How many third-party packages `Cargo.lock` may hold. Not what ships, which
/// is about half of it: the desktop preview's winit is most of the rest. A
/// crude figure, but one that cannot grow without being edited here. When the
/// lockfile shrinks, bring it down.
const LOCKED: usize = 312;

/// Check the workspace at `root` against all of the above.
pub fn deps(root: &Path) -> Result<()> {
    let manifest = read(&root.join("Cargo.toml"))?;
    let mut members = BTreeSet::new();
    let mut named: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in dependencies(&manifest) {
        named.entry(name).or_default().push("Cargo.toml".to_owned());
    }
    for dir in workspace_members(&manifest) {
        let text = read(&root.join(&dir).join("Cargo.toml"))?;
        let package =
            package_name(&text).with_context(|| format!("{dir}/Cargo.toml names no package"))?;
        for name in dependencies(&text) {
            named.entry(name).or_default().push(package.clone());
        }
        members.insert(package);
    }
    ensure!(!members.is_empty(), "Cargo.toml lists no workspace members");

    let locked = locked(&read(&root.join("Cargo.lock"))?);
    let third_party = locked.iter().filter(|p| p.source.is_some()).count();
    let mut wrong = problems(&members, &named, &locked);
    if third_party > LOCKED {
        wrong.push(format!(
            "Cargo.lock holds {third_party} third-party packages, and {LOCKED} is the most \
             it may; take the new ones out, or raise LOCKED in xtask/src/deps.rs and say why"
        ));
    }
    if !wrong.is_empty() {
        bail!("{}\nsee docs/adr/0023-dependencies.md", wrong.join("\n"));
    }

    let direct = named
        .keys()
        .filter(|n| !members.contains(*n) && !OURS.contains(&n.as_str()))
        .count();
    println!("{direct} third-party crates named, {third_party} locked, all from crates.io");
    if third_party < LOCKED {
        println!("LOCKED in xtask/src/deps.rs is {LOCKED}; it can come down to {third_party}");
    }
    Ok(())
}

/// Everything that breaks the rule, each as a line to print.
fn problems(
    members: &BTreeSet<String>,
    named: &BTreeMap<String, Vec<String>>,
    locked: &[Locked],
) -> Vec<String> {
    let mut wrong = Vec::new();
    for package in locked {
        match &package.source {
            Some(source) if source != CRATES_IO => wrong.push(format!(
                "{} {} is locked from {source}, not from crates.io",
                package.name, package.version
            )),
            // No source is a path: a member, or something outside the
            // workspace that no registry vouches for.
            None if !members.contains(&package.name) => wrong.push(format!(
                "{} {} is locked from a path outside the workspace",
                package.name, package.version
            )),
            _ => {}
        }
    }
    let allowed = |name: &str| ALLOWED.iter().any(|(n, _)| *n == name);
    for (name, by) in named {
        if !members.contains(name) && !OURS.contains(&name.as_str()) && !allowed(name) {
            wrong.push(format!(
                "{name} is named by {} and is not in ALLOWED in xtask/src/deps.rs; \
                 a new dependency is added there, with its reason",
                by.join(", ")
            ));
        }
    }
    for (name, _) in ALLOWED {
        if !named.contains_key(name) {
            wrong.push(format!(
                "{name} is in ALLOWED in xtask/src/deps.rs and nothing names it; take it out"
            ));
        }
    }
    wrong
}

/// One `[[package]]` of `Cargo.lock`.
#[derive(Debug, Default, PartialEq)]
struct Locked {
    name: String,
    version: String,
    /// Where it comes from; a path dependency has none.
    source: Option<String>,
}

/// The packages `Cargo.lock` holds.
fn locked(lock: &str) -> Vec<Locked> {
    let mut out: Vec<Locked> = Vec::new();
    for line in lock.lines() {
        if line == "[[package]]" {
            out.push(Locked::default());
        } else if let Some(package) = out.last_mut() {
            if let Some(v) = quoted(line, "name = ") {
                package.name = v;
            } else if let Some(v) = quoted(line, "version = ") {
                package.version = v;
            } else if let Some(v) = quoted(line, "source = ") {
                package.source = Some(v);
            }
        }
    }
    out
}

/// The directories under `members = [ … ]` in the workspace's manifest.
fn workspace_members(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        if line.starts_with("members = [") {
            inside = true;
        }
        if inside {
            out.extend(line.split('"').skip(1).step_by(2).map(str::to_owned));
            if line.trim_end().ends_with(']') {
                break;
            }
        }
    }
    out
}

/// The `name` under `[package]`.
fn package_name(manifest: &str) -> Option<String> {
    let mut table = "";
    for line in manifest.lines() {
        if let Some(name) = header(line) {
            table = name;
        } else if table == "package"
            && let Some(name) = quoted(line, "name = ")
        {
            return Some(name);
        }
    }
    None
}

/// Every crate a manifest names as a dependency of any kind: plain, dev and
/// build, for every target, and the workspace's own table. By the name the
/// registry knows it by, where a `package =` gives it another.
fn dependencies(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        if let Some(table) = header(line) {
            // `[dependencies.name]`, a dependency with a table to itself.
            let own = KINDS
                .iter()
                .find_map(|kind| table.strip_prefix(kind)?.strip_prefix('.'));
            if let Some(name) = own {
                out.push(name.to_owned());
            }
            inside = KINDS
                .iter()
                .any(|kind| table == *kind || table.ends_with(&format!(".{kind}")));
            continue;
        }
        if !inside {
            continue;
        }
        // A key starts its line; what continues an array does not.
        let key: String = line
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        let rest = line[key.len()..].trim_start();
        if key.is_empty() || !(rest.starts_with('=') || rest.starts_with('.')) {
            continue;
        }
        let renamed = rest
            .split_once("package = \"")
            .and_then(|(_, after)| after.split('"').next());
        out.push(renamed.map_or(key, str::to_owned));
    }
    out
}

/// The tables a dependency is declared in.
const KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// The name in a `[table]` line.
fn header(line: &str) -> Option<&str> {
    line.trim_end().strip_prefix('[')?.strip_suffix(']')
}

/// The text between the quotes of `key = "…"`.
fn quoted(line: &str, key: &str) -> Option<String> {
    Some(
        line.strip_prefix(key)?
            .trim()
            .strip_prefix('"')?
            .strip_suffix('"')?
            .to_owned(),
    )
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        ALLOWED, CRATES_IO, Locked, OURS, dependencies, deps, locked, package_name, problems,
        workspace_members,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    /// The check CI runs, on the workspace this is built from: a dependency
    /// added without a line here fails the tests as well.
    #[test]
    fn the_workspace_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        if let Err(e) = deps(&root) {
            panic!("{e}");
        }
    }

    #[test]
    fn the_lists_are_in_order_and_name_nothing_twice() {
        assert!(ALLOWED.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(OURS.windows(2).all(|w| w[0] < w[1]));
        assert!(ALLOWED.iter().all(|(name, _)| !OURS.contains(name)));
    }

    const MANIFEST: &str = r#"[package]
name = "alpymist-lock"
version.workspace = true

[features]
default = ["window"]
window = [
    "dep:denise",
]

[dependencies]
# A comment = not a dependency.
alpymist-core = { workspace = true }
denise.workspace = true
toml = { version = "0.9", default-features = false, features = [
    "parse",
] }
other = { package = "real-name", version = "1" }

[target.'cfg(target_os = "linux")'.dependencies]
nix = "0.29"

[dev-dependencies]
png = "0.17"

[build-dependencies.cc]
version = "1"

[lints]
workspace = true
"#;

    #[test]
    fn every_kind_of_dependency_is_found_and_nothing_else() {
        assert_eq!(
            dependencies(MANIFEST),
            [
                "alpymist-core",
                "denise",
                "toml",
                "real-name",
                "nix",
                "png",
                "cc"
            ]
        );
        assert_eq!(package_name(MANIFEST).as_deref(), Some("alpymist-lock"));
    }

    #[test]
    fn the_workspace_table_counts_and_its_members_are_read() {
        let root = "[workspace]\nresolver = \"3\"\nmembers = [\n    \"crates/a\",\n    \
                    \"xtask\",\n]\n\n[workspace.package]\nversion = \"0.1.0\"\n\n\
                    [workspace.dependencies]\na = { path = \"crates/a\" }\nserde = \"1\"\n\n\
                    [profile.release]\nlto = true\n";
        assert_eq!(workspace_members(root), ["crates/a", "xtask"]);
        assert_eq!(dependencies(root), ["a", "serde"]);
    }

    const LOCK: &str = "version = 4\n\n[[package]]\nname = \"alpymist\"\nversion = \"0.3.4\"\n\
                        dependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"serde\"\n\
                        version = \"1.0.0\"\n\
                        source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
                        checksum = \"00\"\n";

    #[test]
    fn a_locked_package_is_its_name_version_and_source() {
        assert_eq!(
            locked(LOCK),
            [
                Locked {
                    name: "alpymist".into(),
                    version: "0.3.4".into(),
                    source: None,
                },
                Locked {
                    name: "serde".into(),
                    version: "1.0.0".into(),
                    source: Some(CRATES_IO.into()),
                },
            ]
        );
    }

    /// A workspace of one member naming every allowed crate, so that only
    /// what a test adds is wrong with it.
    fn workspace() -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
        let members = BTreeSet::from(["alpymist".to_owned()]);
        let named = ALLOWED
            .iter()
            .map(|(name, _)| ((*name).to_owned(), vec!["alpymist".to_owned()]))
            .collect();
        (members, named)
    }

    fn package(name: &str, source: Option<&str>) -> Locked {
        Locked {
            name: name.into(),
            version: "1.0.0".into(),
            source: source.map(str::to_owned),
        }
    }

    #[test]
    fn crates_io_and_the_members_are_all_that_may_be_locked() {
        let (members, named) = workspace();
        let lock = [
            package("alpymist", None),
            package("serde", Some(CRATES_IO)),
            package("forked", Some("git+https://example.com/forked#abc")),
            package("mirrored", Some("registry+https://example.com/index")),
            package("beside", None),
        ];
        let wrong = problems(&members, &named, &lock);
        assert_eq!(wrong.len(), 3, "{wrong:#?}");
        assert!(wrong[0].starts_with("forked 1.0.0 is locked from git+"));
        assert!(wrong[1].starts_with("mirrored 1.0.0 is locked from registry+https://example"));
        assert!(wrong[2].starts_with("beside 1.0.0 is locked from a path outside"));
    }

    #[test]
    fn a_crate_not_on_the_list_is_refused_and_ours_are_not() {
        let (members, mut named) = workspace();
        named.insert("left-pad".into(), vec!["alpymist".into()]);
        named.insert("denise".into(), vec!["alpymist".into()]);
        named.insert("alpymist".into(), vec!["Cargo.toml".into()]);
        // A name that only starts as ours do is not ours.
        named.insert("denise-extras".into(), vec!["alpymist".into()]);
        let wrong = problems(&members, &named, &[]);
        assert_eq!(wrong.len(), 2, "{wrong:#?}");
        assert!(wrong[0].starts_with("denise-extras is named by alpymist"));
        assert!(wrong[1].starts_with("left-pad is named by alpymist"));
    }

    #[test]
    fn a_crate_no_longer_used_has_to_leave_the_list() {
        let (members, mut named) = workspace();
        named.remove("zeroize");
        let wrong = problems(&members, &named, &[]);
        assert_eq!(wrong.len(), 1, "{wrong:#?}");
        assert!(wrong[0].starts_with("zeroize is in ALLOWED"));
    }
}

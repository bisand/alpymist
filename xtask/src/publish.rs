//! Publishing packages to the Alpymist repository.
//!
//! CI builds the packages (the Release workflow); this signs and ships them.
//! The split is ADR 0002's offline key: the release key never reaches CI, so a
//! compromised workflow can produce a bad artifact but not a trusted update.
//!
//! What gets signed is only the repository index. apk accepts a package when
//! its hash matches a trusted index, whatever key abuild signed the package
//! itself with, and rejects one that does not — both checked by hand before
//! this was written.
//!
//! The repository is a GitHub Pages site behind `pkgs.alpymist.org`. Each
//! publish replaces it with a single commit, so the repository never grows
//! old binaries past Pages' size limit; rolling back means publishing an
//! older run again.
//!
//! The dev channel is the same, with a key and a site of its own
//! (`dev.pkgs.alpymist.org`), and published by CI on every push to main
//! (ADR 0006). Its key is trusted only by systems that ask to follow dev, and
//! CI's credentials reach only its site, so a compromised workflow can reach
//! those systems and no others.

use alpymist_core::Channel;
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Alpine release the repository serves; the installer's must match.
const ALPINE_VERSION: &str = "v3.24";
/// The repository's name under that release, as in `/etc/apk/repositories`.
const REPOSITORY: &str = "alpymist";
/// Every architecture the Release workflow builds, by artifact suffix.
const ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// What one channel is published with and to.
struct Target {
    channel: Channel,
    /// The key's file name, which is also the name apk looks it up by.
    key_name: &'static str,
    /// The public half, as shipped by `alpymist-keys`.
    public_key: &'static str,
    /// The Pages repository.
    remote: &'static str,
    /// The workflows whose runs it takes packages from.
    workflows: &'static [&'static str],
}

impl Target {
    const fn of(channel: Channel) -> Self {
        match channel {
            Channel::Stable => Self {
                channel,
                key_name: "alpymist-2026.rsa",
                public_key: "aports/alpymist-keys/alpymist-2026.rsa.pub",
                remote: "git@github.com:bisand/alpymist-packages.git",
                // Release builds the packages with each release; Packages built
                // them on every push before it, and its runs are still kept.
                workflows: &["Release", "Packages"],
            },
            Channel::Dev => Self {
                channel,
                key_name: "alpymist-dev-2026.rsa",
                public_key: "aports/alpymist-keys/alpymist-dev-2026.rsa.pub",
                remote: "git@github.com:bisand/alpymist-packages-dev.git",
                workflows: &["Dev"],
            },
        }
    }

    /// The address the site is served from, out of the channel's repository
    /// line, which is `https://<domain>/<release>/<repository>`.
    fn domain(&self) -> &'static str {
        self.channel
            .repository()
            .strip_prefix("https://")
            .and_then(|rest| rest.strip_suffix(&format!("/{ALPINE_VERSION}/{REPOSITORY}")))
            .unwrap_or_else(|| {
                panic!(
                    "{} is not https://<domain>/{ALPINE_VERSION}/{REPOSITORY}",
                    self.channel.repository()
                )
            })
    }
}

/// Where the packages to publish come from.
pub enum Packages {
    /// A workflow run's artifacts, downloaded with gh.
    Run(String),
    /// Artifacts already downloaded, one `packages-<arch>` directory per
    /// architecture, built from `commit`. How CI publishes dev.
    Downloaded { dir: PathBuf, commit: String },
}

/// Where the work happens; under `out/`, which Docker Desktop shares.
const STAGING: &str = "out/publish";
/// The container every Alpine tool runs in.
const BUILDER: &str = "alpymist-builder";

/// Index, sign and check one architecture's packages. Runs in the builder as
/// root, with the new site at /site, the published one at /old, and the key.
const SIGN: &str = r#"
set -eu
arch="$1"; desc="$2"
dir="/site/$VERSION/$REPO/$arch"
cd "$dir"
apk index --allow-untrusted --no-warnings --quiet \
	--description "$desc" --rewrite-arch "$arch" \
	--output APKINDEX.tar.gz *.apk
abuild-sign -q -k "/key/$KEY" -p "$KEY.pub" APKINDEX.tar.gz
tar -xzOf APKINDEX.tar.gz APKINDEX > "/work/new-$arch.txt"
old="/old/$VERSION/$REPO/$arch/APKINDEX.tar.gz"
if [ -f "$old" ]; then tar -xzOf "$old" APKINDEX > "/work/old-$arch.txt"; fi
# Trust it with nothing but the release key, as an installed system would.
mkdir -p /tmp/keys && cp "/site/$KEY.pub" /tmp/keys/
apk --arch "$arch" --keys-dir /tmp/keys --repositories-file /dev/null \
	-X "/site/$VERSION/$REPO" --no-cache search -q alpymist-keys \
	> "/work/verify-$arch.txt" 2>&1
"#;

/// Publish a channel's packages.
///
/// Without `push` it stops after signing and verifying, and says where the
/// site is, so it can be looked at first.
///
/// # Errors
/// Fails when the packages are not from a successful run of the channel's
/// workflow on main, when stable is given packages that were not downloaded
/// from such a run, when a package was rebuilt without a version bump, when
/// the signed index does not verify, or when any tool it drives fails.
pub fn publish(channel: Channel, packages: &Packages, key: &Path, push: bool) -> Result<()> {
    let target = Target::of(channel);
    ensure!(
        key.file_name().and_then(|n| n.to_str()) == Some(target.key_name),
        "expected {channel}'s key, {}; got {}",
        target.key_name,
        key.display()
    );
    ensure!(key.is_file(), "no key at {}", key.display());
    let key = key.canonicalize()?;

    let (sha, what) = match packages {
        Packages::Run(run) => (check_run(&target, run)?, format!("run {run}")),
        // Stable is signed by hand from what CI built for a reviewed commit;
        // packages from anywhere else would skip that.
        Packages::Downloaded { .. } if channel == Channel::Stable => {
            bail!("stable is published from a Release run: use --run")
        }
        Packages::Downloaded { commit, .. } => {
            ensure!(on_main(commit)?, "{commit} is not on main");
            (commit.clone(), format!("main at {commit}"))
        }
    };
    let short: String = sha.chars().take(12).collect();
    println!("publishing {what} to {channel} (main at {short})");
    ensure_builder()?;

    let staging = Path::new(STAGING);
    if staging.exists() {
        std::fs::remove_dir_all(staging).context("clearing out/publish")?;
    }
    std::fs::create_dir_all(staging)?;
    let staging = staging.canonicalize()?;
    let old = staging.join("old");
    let site = staging.join("site");

    let downloads = match packages {
        Packages::Run(run) => {
            println!("downloading the packages");
            let downloads = staging.join("downloads");
            let mut download = Command::new("gh");
            download
                .args(["run", "download", run, "--dir"])
                .arg(&downloads);
            for arch in ARCHES {
                download.args(["--name", &format!("packages-{arch}")]);
            }
            run_tool(&mut download)?;
            downloads
        }
        Packages::Downloaded { dir, .. } => dir.clone(),
    };

    println!("fetching what is published now");
    std::fs::create_dir_all(&old)?;
    run_tool(
        Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", target.remote])
            .arg(&old),
    )?;

    stage(&target, &what, &downloads, &old, &site)?;
    for arch in ARCHES {
        sign(&target, arch, &short, &site, &old, &staging, &key)?;
    }
    println!("signed and verified");

    if !push {
        println!(
            "\nnot pushed. The site is in {}; run again with --push to publish it.",
            site.display()
        );
        return Ok(());
    }

    let message = format!("Publish {what}\n\nFrom bisand/alpymist at {sha}.");
    for args in [
        &["init", "--quiet", "-b", "main"][..],
        &["add", "--all"],
        &["commit", "--quiet", "-m", &message],
        &["push", "--quiet", "--force", target.remote, "main"],
    ] {
        run_tool(Command::new("git").arg("-C").arg(&site).args(args))?;
    }
    println!(
        "pushed; GitHub Pages serves it at https://{}/ within a minute or two",
        target.domain()
    );
    Ok(())
}

/// Lay out the site: each architecture's packages, the key, and the page.
///
/// A package whose version is already published keeps the published file.
/// CI rebuilds everything, and the builds are not yet reproducible, so the
/// same version comes out with a different hash each time; replacing it would
/// break the cached index of every system that already has it, for no change.
/// The flip side: a change ships only with a new pkgver or pkgrel.
fn stage(target: &Target, what: &str, downloads: &Path, old: &Path, site: &Path) -> Result<()> {
    for arch in ARCHES {
        let dir = site.join(ALPINE_VERSION).join(REPOSITORY).join(arch);
        let published = old.join(ALPINE_VERSION).join(REPOSITORY).join(arch);
        std::fs::create_dir_all(&dir)?;
        let from = downloads.join(format!("packages-{arch}"));
        let (mut new, mut kept) = (Vec::new(), 0);
        for entry in
            std::fs::read_dir(&from).with_context(|| format!("reading {}", from.display()))?
        {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "apk") {
                continue;
            }
            let name = path.file_name().unwrap_or_default();
            ensure!(
                target.channel == Channel::Dev || !dev_stamped(&name.to_string_lossy()),
                "{} carries a dev version; stable takes only Release runs' packages",
                name.to_string_lossy()
            );
            let previous = published.join(name);
            if previous.is_file() {
                std::fs::copy(&previous, dir.join(name))?;
                kept += 1;
            } else {
                std::fs::copy(&path, dir.join(name))?;
                new.push(name.to_string_lossy().into_owned());
            }
        }
        ensure!(kept + new.len() > 0, "{what} has no packages for {arch}");
        new.sort();
        println!(
            "  {arch}: {} new, {kept} already published and kept as they are",
            new.len()
        );
        for name in &new {
            println!("    + {name}");
        }
    }
    std::fs::copy(
        target.public_key,
        site.join(format!("{}.pub", target.key_name)),
    )
    .context("copying the public key")?;
    std::fs::write(site.join("CNAME"), format!("{}\n", target.domain()))?;
    // Jekyll would otherwise process the site, slowly, for nothing.
    std::fs::write(site.join(".nojekyll"), "")?;
    std::fs::write(site.join("index.html"), landing_page(target))?;

    Ok(())
}

/// Index and sign one architecture, then check it the way a system would.
fn sign(
    target: &Target,
    arch: &str,
    short: &str,
    site: &Path,
    old: &Path,
    staging: &Path,
    key: &Path,
) -> Result<()> {
    println!("signing the {arch} index");
    run_tool(
        Command::new("docker")
            .args(["run", "--rm", "--user", "root"])
            .args(["--env", &format!("VERSION={ALPINE_VERSION}")])
            .args(["--env", &format!("REPO={REPOSITORY}")])
            .args(["--env", &format!("KEY={}", target.key_name)])
            .arg("-v")
            .arg(format!("{}:/site", site.display()))
            .arg("-v")
            .arg(format!("{}:/old:ro", old.display()))
            .arg("-v")
            .arg(format!("{}:/work", staging.display()))
            .arg("-v")
            .arg(format!("{}:/key/{}:ro", key.display(), target.key_name))
            .args([BUILDER, "sh", "-c", SIGN, "sign"])
            .args([arch, &format!("alpymist {short}")]),
    )?;

    let verified = std::fs::read_to_string(staging.join(format!("verify-{arch}.txt")))?;
    ensure!(
        verified.trim() == "alpymist-keys",
        "the signed {arch} index does not verify with {} alone:\n{verified}",
        target.key_name
    );

    let new = std::fs::read_to_string(staging.join(format!("new-{arch}.txt")))?;
    if let Ok(published) = std::fs::read_to_string(staging.join(format!("old-{arch}.txt"))) {
        let clashes = rebuilt_in_place(&published, &new);
        ensure!(
            clashes.is_empty(),
            "these {arch} packages changed without a new version, so installed systems \
             would never receive them (and apk would reject the mismatch for anyone \
             with the old index cached). Bump pkgrel or pkgver:\n  {}",
            clashes.join("\n  ")
        );
    }
    Ok(())
}

/// The run must be a successful run of the channel's workflow, for a commit on main, so what gets
/// signed is what was reviewed. Returns the commit it built.
///
/// A run from the Run workflow button says main as its branch. A run for a
/// release says the release's tag instead, which could name any commit, so
/// that one is asked of GitHub: the tagged commit must be main or behind it.
fn check_run(target: &Target, run: &str) -> Result<String> {
    let out = Command::new("gh")
        .args([
            "run",
            "view",
            run,
            "--json",
            "conclusion,headSha,headBranch,workflowName",
        ])
        .args([
            "--jq",
            r#"[.conclusion,.headSha,.headBranch,.workflowName]|join("\t")"#,
        ])
        .output()
        .context("running gh")?;
    if !out.status.success() {
        bail!(
            "gh run view {run}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let fields: Vec<&str> = text.trim().split('\t').collect();
    let [conclusion, sha, branch, workflow] = fields[..] else {
        bail!("unexpected answer from gh: {text}");
    };
    ensure!(
        target.workflows.contains(&workflow),
        "run {run} is a {workflow} run, not {}, which {} is published from",
        target.workflows.join(" or "),
        target.channel
    );
    ensure!(
        branch == "main" || on_main(sha)?,
        "run {run} built {branch}, which is not on main"
    );
    ensure!(
        conclusion == "success",
        "run {run} did not succeed ({conclusion})"
    );
    Ok(sha.to_string())
}

/// Whether `sha` is main or a commit main has moved past.
fn on_main(sha: &str) -> Result<bool> {
    let out = Command::new("gh")
        .args([
            "api",
            &format!("repos/{{owner}}/{{repo}}/compare/main...{sha}"),
            "--jq",
            ".status",
        ])
        .output()
        .context("running gh")?;
    if !out.status.success() {
        bail!(
            "comparing {sha} with main: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(matches!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "identical" | "behind"
    ))
}

/// Build the builder image when Docker no longer has it. Image pruning removes
/// it without warning, and publishing then failed halfway through.
fn ensure_builder() -> Result<()> {
    let present = Command::new("docker")
        .args(["image", "inspect", BUILDER])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("running docker")?
        .success();
    if !present {
        println!("building the {BUILDER} image, which Docker no longer has");
        run_tool(Command::new("docker").args(["build", "-q", "-t", BUILDER, "builder"]))?;
    }
    Ok(())
}

/// Run a tool, failing with its name when it fails.
fn run_tool(command: &mut Command) -> Result<()> {
    let name = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .with_context(|| format!("running {name}"))?;
    ensure!(status.success(), "{name} failed ({status})");
    Ok(())
}

/// Packages published before under the same name and version, with different
/// contents. apk never upgrades to those, so they must not go out.
fn rebuilt_in_place(published: &str, new: &str) -> Vec<String> {
    let published = identities(published);
    identities(new)
        .into_iter()
        .filter_map(|(name, hash)| {
            published
                .get(&name)
                .filter(|&old| *old != hash)
                .map(|_| name)
        })
        .collect()
}

/// `name-version` to content hash, from APKINDEX text.
fn identities(index: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for record in index.split("\n\n") {
        let field = |key: &str| {
            record
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .map(str::to_string)
        };
        if let (Some(hash), Some(name), Some(version)) = (field("C:"), field("P:"), field("V:")) {
            out.insert(format!("{name}-{version}"), hash);
        }
    }
    out
}

/// What a person who opens the address in a browser sees.
fn landing_page(target: &Target) -> String {
    let (title, key) = (
        match target.channel {
            Channel::Stable => "Alpymist packages",
            Channel::Dev => "Alpymist packages: dev",
        },
        target.key_name,
    );
    let mut page = format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n\
         <style>body{{font:16px/1.5 system-ui,sans-serif;max-width:40rem;margin:3rem auto;\
         padding:0 1rem;background:#0b121e;color:#eaf0f6}}code,pre{{font-family:\"Fira Mono\",\
         monospace}}pre{{background:#070c14;padding:1rem;overflow-x:auto}}a{{color:#7fb8d9}}</style>\n\
         <h1>{title}</h1>\n"
    );
    match target.channel {
        Channel::Stable => {
            page.push_str(
                "<p>The apk repository for <a href=\"https://alpymist.org\">Alpymist</a>. \
                 Installed systems already use it. On another Alpine machine:</p>\n<pre>",
            );
            let _ = write!(
                page,
                "doas wget -O /etc/apk/keys/{key}.pub https://{domain}/{key}.pub\n\
                 echo {repo} | doas tee -a /etc/apk/repositories\n\
                 doas apk update</pre>\n",
                domain = target.domain(),
                repo = target.channel.repository(),
            );
            page.push_str(
                "<p>Only the index is signed, by hand, with a key CI never sees; it pins every \
                 package's hash. Check the key's fingerprint against the source repository before \
                 trusting it.</p>\n",
            );
        }
        Channel::Dev => {
            page.push_str(
                "<p>Every push to main of <a href=\"https://github.com/bisand/alpymist\">Alpymist</a>, \
                 built and signed by CI with a key of its own. Expect breakage. On Alpymist:</p>\n\
                 <pre>doas alpymistctl channel dev</pre>\n\
                 <p>and back with <code>doas alpymistctl channel stable</code>, which also \
                 stops trusting this key.</p>\n",
            );
        }
    }
    page
}

/// Where a channel's key lives unless told otherwise.
pub fn default_key(channel: Channel) -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config/alpymist/keys")
        .join(Target::of(channel).key_name)
}

/// Whether an apk file name carries the version `build-packages.sh` gives dev
/// packages: `_git` and a fourteen-digit time. Upstream snapshots such as
/// ghostty's `_git20260908` have a date only.
fn dev_stamped(file: &str) -> bool {
    file.match_indices("_git").any(|(i, m)| {
        let rest = &file[i + m.len()..];
        rest.len() > 14
            && rest.as_bytes()[..14].iter().all(u8::is_ascii_digit)
            && rest.as_bytes()[14] == b'-'
    })
}

#[cfg(test)]
mod tests {
    use super::{Target, dev_stamped, rebuilt_in_place};
    use alpymist_core::Channel;

    #[test]
    fn each_channel_is_served_where_systems_look() {
        assert_eq!(Target::of(Channel::Stable).domain(), "pkgs.alpymist.org");
        assert_eq!(Target::of(Channel::Dev).domain(), "dev.pkgs.alpymist.org");
        for channel in Channel::ALL {
            let target = Target::of(channel);
            assert!(std::path::Path::new("..").join(target.public_key).is_file());
        }
    }

    #[test]
    fn dev_versions_are_told_from_upstream_snapshots() {
        assert!(dev_stamped("alpymist-menu-0.0.1_git20260915120301-r6.apk"));
        assert!(!dev_stamped("ghostty-1.3.1_git20260908-r0.apk"));
        assert!(!dev_stamped("alpymist-menu-0.0.1-r6.apk"));
    }

    const PUBLISHED: &str = "C:Q1aaa=\nP:ghostty\nV:1.3.1-r0\nA:x86_64\n\n\
                             C:Q1bbb=\nP:alpymist-desktop\nV:0.0.1-r0\n\n";

    #[test]
    fn an_unchanged_package_is_fine() {
        assert!(rebuilt_in_place(PUBLISHED, PUBLISHED).is_empty());
    }

    #[test]
    fn a_new_version_is_fine() {
        let new = PUBLISHED
            .replace("V:0.0.1-r0\n\n", "V:0.0.1-r1\n\n")
            .replace("Q1bbb", "Q1ccc");
        assert!(rebuilt_in_place(PUBLISHED, &new).is_empty());
    }

    #[test]
    fn a_rebuild_under_the_same_version_is_caught() {
        let new = PUBLISHED.replace("Q1bbb", "Q1ccc");
        assert_eq!(
            rebuilt_in_place(PUBLISHED, &new),
            ["alpymist-desktop-0.0.1-r0"]
        );
    }

    #[test]
    fn nothing_published_yet_is_fine() {
        assert!(rebuilt_in_place("", PUBLISHED).is_empty());
    }
}

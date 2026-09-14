//! Publishing packages to the Alpymist repository.
//!
//! CI builds the packages (the Packages workflow); this signs and ships them.
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

use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Alpine release the repository serves; the installer's must match.
const ALPINE_VERSION: &str = "v3.24";
/// The repository's name under that release, as in `/etc/apk/repositories`.
const REPOSITORY: &str = "alpymist";
/// Every architecture the Packages workflow builds, by artifact suffix.
const ARCHES: [&str; 2] = ["x86_64", "aarch64"];
/// The release key's file name, which is also the name apk looks it up by.
const KEY_NAME: &str = "alpymist-2026.rsa";
/// The public half, as shipped by `alpymist-keys`.
const PUBLIC_KEY: &str = "aports/alpymist-keys/alpymist-2026.rsa.pub";
/// The Pages repository.
const SITE_REMOTE: &str = "git@github.com:bisand/alpymist-packages.git";
/// The address the site is served from.
const DOMAIN: &str = "pkgs.alpymist.org";
/// The workflow whose runs are publishable.
const WORKFLOW: &str = "Packages";
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

/// Publish the packages from a Packages run.
///
/// Without `push` it stops after signing and verifying, and says where the
/// site is, so it can be looked at first.
///
/// # Errors
/// Fails when the run is not a successful Packages run on main, when a
/// package was rebuilt without a version bump, when the signed index does not
/// verify, or when any tool it drives fails.
pub fn publish(run: &str, key: &Path, push: bool) -> Result<()> {
    ensure!(
        key.file_name().and_then(|n| n.to_str()) == Some(KEY_NAME),
        "expected the release key, {KEY_NAME}; got {}",
        key.display()
    );
    ensure!(key.is_file(), "no key at {}", key.display());
    let key = key.canonicalize()?;

    let sha = check_run(run)?;
    let short: String = sha.chars().take(12).collect();
    println!("publishing Packages run {run} (main at {short})");

    let staging = Path::new(STAGING);
    if staging.exists() {
        std::fs::remove_dir_all(staging).context("clearing out/publish")?;
    }
    std::fs::create_dir_all(staging)?;
    let staging = staging.canonicalize()?;
    let downloads = staging.join("downloads");
    let old = staging.join("old");
    let site = staging.join("site");

    println!("downloading the packages");
    let mut download = Command::new("gh");
    download
        .args(["run", "download", run, "--dir"])
        .arg(&downloads);
    for arch in ARCHES {
        download.args(["--name", &format!("packages-{arch}")]);
    }
    run_tool(&mut download)?;

    println!("fetching what is published now");
    std::fs::create_dir_all(&old)?;
    run_tool(
        Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", SITE_REMOTE])
            .arg(&old),
    )?;

    stage(run, &downloads, &site)?;
    for arch in ARCHES {
        sign(arch, &short, &site, &old, &staging, &key)?;
    }
    println!("signed and verified");

    if !push {
        println!(
            "\nnot pushed. The site is in {}; run again with --push to publish it.",
            site.display()
        );
        return Ok(());
    }

    let message = format!("Publish Packages run {run}\n\nFrom bisand/alpymist at {sha}.");
    for args in [
        &["init", "--quiet", "-b", "main"][..],
        &["add", "--all"],
        &["commit", "--quiet", "-m", &message],
        &["push", "--quiet", "--force", SITE_REMOTE, "main"],
    ] {
        run_tool(Command::new("git").arg("-C").arg(&site).args(args))?;
    }
    println!("pushed; GitHub Pages serves it at https://{DOMAIN}/ within a minute or two");
    Ok(())
}

/// Lay out the site: each architecture's packages, the key, and the page.
fn stage(run: &str, downloads: &Path, site: &Path) -> Result<()> {
    for arch in ARCHES {
        let dir = site.join(ALPINE_VERSION).join(REPOSITORY).join(arch);
        std::fs::create_dir_all(&dir)?;
        let from = downloads.join(format!("packages-{arch}"));
        let mut count = 0;
        for entry in
            std::fs::read_dir(&from).with_context(|| format!("reading {}", from.display()))?
        {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "apk") {
                std::fs::copy(&path, dir.join(path.file_name().unwrap_or_default()))?;
                count += 1;
            }
        }
        ensure!(count > 0, "run {run} has no packages for {arch}");
        println!("  {arch}: {count} packages");
    }
    std::fs::copy(PUBLIC_KEY, site.join(format!("{KEY_NAME}.pub")))
        .context("copying the public key")?;
    std::fs::write(site.join("CNAME"), format!("{DOMAIN}\n"))?;
    // Jekyll would otherwise process the site, slowly, for nothing.
    std::fs::write(site.join(".nojekyll"), "")?;
    std::fs::write(site.join("index.html"), landing_page())?;

    Ok(())
}

/// Index and sign one architecture, then check it the way a system would.
fn sign(
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
            .args(["--env", &format!("KEY={KEY_NAME}")])
            .arg("-v")
            .arg(format!("{}:/site", site.display()))
            .arg("-v")
            .arg(format!("{}:/old:ro", old.display()))
            .arg("-v")
            .arg(format!("{}:/work", staging.display()))
            .arg("-v")
            .arg(format!("{}:/key/{KEY_NAME}:ro", key.display()))
            .args([BUILDER, "sh", "-c", SIGN, "sign"])
            .args([arch, &format!("alpymist {short}")]),
    )?;

    let verified = std::fs::read_to_string(staging.join(format!("verify-{arch}.txt")))?;
    ensure!(
        verified.trim() == "alpymist-keys",
        "the signed {arch} index does not verify with the release key alone:\n{verified}"
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

/// The run must be a successful Packages run on main, so what gets signed is
/// what was reviewed. Returns the commit it built.
fn check_run(run: &str) -> Result<String> {
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
        workflow == WORKFLOW,
        "run {run} is a {workflow} run, not {WORKFLOW}"
    );
    ensure!(branch == "main", "run {run} built {branch}, not main");
    ensure!(
        conclusion == "success",
        "run {run} did not succeed ({conclusion})"
    );
    Ok(sha.to_string())
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
fn landing_page() -> String {
    let mut page = String::from(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Alpymist packages</title>\n\
         <style>body{font:16px/1.5 system-ui,sans-serif;max-width:40rem;margin:3rem auto;\
         padding:0 1rem;background:#0b121e;color:#eaf0f6}code,pre{font-family:\"Fira Mono\",\
         monospace}pre{background:#070c14;padding:1rem;overflow-x:auto}a{color:#7fb8d9}</style>\n\
         <h1>Alpymist packages</h1>\n\
         <p>The apk repository for <a href=\"https://alpymist.org\">Alpymist</a>. \
         Installed systems already use it. On another Alpine machine:</p>\n<pre>",
    );
    let _ = write!(
        page,
        "doas wget -O /etc/apk/keys/{KEY_NAME}.pub https://{DOMAIN}/{KEY_NAME}.pub\n\
         echo https://{DOMAIN}/{ALPINE_VERSION}/{REPOSITORY} | doas tee -a /etc/apk/repositories\n\
         doas apk update</pre>\n"
    );
    page.push_str(
        "<p>Only the index is signed, by hand, with a key CI never sees; it pins every \
         package's hash. Check the key's fingerprint against the source repository before \
         trusting it.</p>\n",
    );
    page
}

/// Where the release key lives unless told otherwise.
pub fn default_key() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config/alpymist/keys")
        .join(KEY_NAME)
}

#[cfg(test)]
mod tests {
    use super::rebuilt_in_place;

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

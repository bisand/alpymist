//! That `ci/build-packages.sh` builds each package after the first-party
//! packages it needs to be built.
//!
//! abuild installs a package's own `depends` and `makedepends` before building
//! it, and a first-party one comes only from what this build has already made.
//! Built in the wrong order, the build stops with "no such package" on main
//! after the pull request's checks passed, since those build no packages: which
//! is how alpymist-desktop failed once it depended on squint itself.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
    }

    /// The packages in the order the build script builds them.
    fn build_order() -> Vec<String> {
        let script = std::fs::read_to_string(root().join("ci/build-packages.sh")).unwrap();
        let line = script
            .lines()
            .find(|l| l.trim_start().starts_with("for pkg in alpymist-keys"))
            .expect("the build loop");
        let list = line.split_once(" in ").unwrap().1;
        list.split(';')
            .next()
            .unwrap()
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }

    /// A variable of an APKBUILD's own, `depends` or `makedepends`, at the top
    /// level: a subpackage's are not installed to build it.
    fn variable(apkbuild: &str, name: &str) -> Vec<String> {
        let start = format!("{name}=\"");
        let Some(at) = apkbuild.lines().position(|l| l.starts_with(&start)) else {
            return Vec::new();
        };
        let mut text = String::new();
        for line in apkbuild.lines().skip(at) {
            text.push_str(line);
            text.push('\n');
            if line.trim_end().ends_with('"') && text.len() > start.len() + 1 {
                break;
            }
        }
        text[start.len()..]
            .trim_end()
            .trim_end_matches('"')
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }

    /// A dependency without its version: `alpymist-shell` from
    /// `alpymist-shell=$pkgver-r$pkgrel`.
    fn name(dependency: &str) -> &str {
        dependency
            .split(['=', '<', '>', '~'])
            .next()
            .unwrap_or(dependency)
    }

    #[test]
    fn every_package_is_built_after_what_it_needs_to_be_built() {
        let order = build_order();
        let aports: Vec<String> = std::fs::read_dir(root().join("aports"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().join("APKBUILD").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        // Which aport makes each package name: its own, and its subpackages'.
        let mut maker: HashMap<String, String> = HashMap::new();
        let mut apkbuilds = HashMap::new();
        for aport in &aports {
            let apkbuild =
                std::fs::read_to_string(root().join("aports").join(aport).join("APKBUILD"))
                    .unwrap();
            maker.insert(aport.clone(), aport.clone());
            for sub in variable(&apkbuild, "subpackages") {
                let sub = sub.split(':').next().unwrap().replace("$pkgname", aport);
                maker.insert(sub, aport.clone());
            }
            apkbuilds.insert(aport.clone(), apkbuild);
        }

        for aport in &aports {
            let at = order
                .iter()
                .position(|p| p == aport)
                .unwrap_or_else(|| panic!("{aport} is never built: add it to the build loop"));
            let apkbuild = &apkbuilds[aport];
            for dependency in variable(apkbuild, "depends")
                .into_iter()
                .chain(variable(apkbuild, "makedepends"))
            {
                let Some(needed) = maker.get(name(&dependency)) else {
                    continue;
                };
                if needed == aport {
                    continue;
                }
                let before = order.iter().position(|p| p == needed).unwrap();
                assert!(
                    before < at,
                    "{aport} needs {} to be built, which the build loop makes after it",
                    name(&dependency)
                );
            }
        }
    }

    #[test]
    fn a_dependency_is_named_without_its_version() {
        assert_eq!(name("alpymist-shell=$pkgver-r$pkgrel"), "alpymist-shell");
        assert_eq!(name("squint"), "squint");
        assert_eq!(name("foo>=1.2"), "foo");
    }
}

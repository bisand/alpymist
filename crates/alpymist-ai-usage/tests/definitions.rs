//! The provider files Alpymist ships, and the program they name.
//!
//! What keeps a file and its program from drifting apart: every file in
//! `desktop/ai-usage` reads, runs `alpymist-ai-provider` with a name that
//! program answers to, and is one the package installs.

use alpymist_ai_usage::definition::{Definition, discover_in};
use std::path::PathBuf;

fn shipped() -> Vec<Definition> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../desktop/ai-usage");
    let found = discover_in(std::slice::from_ref(&dir));
    let files = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .count();
    assert_eq!(
        found.len(),
        files,
        "a file in desktop/ai-usage does not read"
    );
    found
}

#[test]
fn every_shipped_provider_names_a_program_that_answers_to_it() {
    let program = include_str!("../src/bin/provider.rs");
    let found = shipped();
    assert_eq!(
        found.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["anthropic-api", "claude", "openai-api", "openrouter"]
    );
    for def in found {
        let mut lines = vec![def.exec.clone()];
        lines.extend(def.enable.clone());
        lines.extend(def.disable.clone());
        for line in lines {
            let argv: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(argv[0], "alpymist-ai-provider", "{}: {line}", def.id);
            assert!(
                program.contains(&format!("Some(\"{}\")", argv[1])),
                "{}: alpymist-ai-provider does not answer to {}",
                def.id,
                argv[1]
            );
        }
        assert!(
            !def.unofficial,
            "{}: only documented ways are shipped here",
            def.id
        );
    }
}

#[test]
fn the_package_installs_every_provider_file() {
    let apkbuild = include_str!("../../../aports/alpymist-ai-usage/APKBUILD");
    assert!(
        apkbuild.contains("desktop/ai-usage/*.toml"),
        "the package installs the files by pattern, so a new one is not forgotten"
    );
    // A tool the vendor installs is never a dependency of the package.
    for def in shipped() {
        if let Some(requires) = def.requires {
            assert!(
                !apkbuild.contains(&format!(" {} ", requires.program)),
                "{} is installed its vendor's way, not as a dependency",
                requires.program
            );
        }
    }
}

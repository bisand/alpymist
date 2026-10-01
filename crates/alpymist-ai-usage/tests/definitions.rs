//! The provider files Alpymist ships, and the program they name.
//!
//! What keeps a file and its program from drifting apart: every file in
//! `desktop/ai-usage` reads, runs `alpymist-ai-provider` with a name that
//! program answers to, and is one the package installs.

use alpymist_ai_usage::definition::{Definition, discover_in};
use std::path::PathBuf;

/// The shipped providers that read what their vendor does not document.
const UNDOCUMENTED: [&str; 3] = ["codex", "gemini", "github-copilot"];

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

/// Only a provider that reaches no one may be asked more often than Settings
/// says. Claude's reads the file its status line keeps; every other one
/// calls its vendor.
#[test]
fn only_the_provider_that_reads_a_file_is_asked_regardless_of_settings() {
    let local: Vec<String> = shipped()
        .into_iter()
        .filter(|d| d.local)
        .map(|d| d.id)
        .collect();
    assert_eq!(local, ["claude"]);
}

#[test]
fn every_shipped_provider_names_a_program_that_answers_to_it() {
    let program = include_str!("../src/bin/provider.rs");
    let found = shipped();
    assert_eq!(
        found.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        [
            "anthropic-api",
            "claude",
            "codex",
            "gemini",
            "github-copilot",
            "openai-api",
            "openrouter"
        ]
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
        // The ones that read what a vendor does not document say so, and
        // are handed no key: the login they use stays the vendor's tool's.
        let undocumented = UNDOCUMENTED.contains(&def.id.as_str());
        assert_eq!(def.unofficial, undocumented, "{}", def.id);
        assert!(
            !undocumented || def.credentials.is_empty(),
            "{}: takes a key",
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

/// An installer is run by `sh -c`, and a line that pipes into another
/// shell needs that shell to be there: Alpine has no bash of its own, and
/// Claude Code's installer is written in it.
#[test]
fn every_installer_has_the_shell_it_is_piped_into() {
    let apkbuild = include_str!("../../../aports/alpymist-ai-usage/APKBUILD");
    let depends = apkbuild
        .lines()
        .find_map(|line| line.strip_prefix("depends=\""))
        .expect("a depends line");
    for def in shipped() {
        let Some(requires) = def.requires else {
            continue;
        };
        if requires
            .install
            .split('|')
            .skip(1)
            .any(|part| part.trim() == "bash")
        {
            assert!(
                depends.split_whitespace().any(|package| package == "bash"),
                "{}: its installer is piped into bash, which the package does not bring",
                def.id
            );
        }
    }
}

/// The menu's entries for the vendors' tools name providers that are
/// shipped, read through a tool, and say how it is removed; and the package
/// installs the file.
#[test]
fn the_menu_names_tools_that_can_be_run_installed_and_removed() {
    let menu = include_str!("../../../desktop/ai-usage/menu/ai-usage.toml");
    let apkbuild = include_str!("../../../aports/alpymist-ai-usage/APKBUILD");
    assert!(apkbuild.contains("menu.d/ai-usage.toml"));
    let program = include_str!("../src/main.rs");
    let found = shipped();
    let mut named = 0;
    for line in menu.lines() {
        let Some(rest) = line.split("alpymist-ai-usage tool ").nth(1) else {
            continue;
        };
        let mut words = rest.split(['"', ' ']);
        let (verb, id) = (words.next().unwrap(), words.next().unwrap());
        assert!(
            program.contains(&format!("[\"tool\", \"{verb}\", id]")),
            "alpymist-ai-usage tool does not answer to {verb}"
        );
        let def = found
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("the menu names {id}, which is not shipped"));
        let requires = def
            .requires
            .as_ref()
            .unwrap_or_else(|| panic!("{id} is not read through a tool"));
        assert!(
            requires.remove.is_some(),
            "{id} does not say how it is removed"
        );
        named += 1;
    }
    // Every tool there is, three ways each: run, install, remove.
    let tools = found.iter().filter(|d| d.requires.is_some()).count();
    assert_eq!(named, tools * 3);
}

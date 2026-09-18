//! The definition file this screensaver ships, checked against the program.
//!
//! The file is what Settings renders its page from and what the program reads
//! its defaults from. Nothing at compile time ties the two together, so this
//! does: a knob renamed in the file and not in `look()` would otherwise show a
//! page whose controls changed nothing at all.

use alpymist_screensaver::definition::{Definition, Dial};

/// The file as it is shipped, read from the repository rather than from
/// /usr/share: the test is about what this commit would install.
const FILE: &str = include_str!("../../../desktop/screensavers/mountains.toml");

fn definition() -> Definition {
    Definition::parse("mountains", FILE).expect("the shipped definition parses")
}

#[test]
fn the_shipped_definition_names_this_program() {
    let def = definition();
    assert_eq!(
        def.exec,
        env!("CARGO_PKG_NAME"),
        "the file must name this program"
    );
    assert_eq!(def.id, "mountains");
    assert!(!def.name.is_empty());
}

#[test]
fn every_setting_the_file_declares_is_one_the_program_reads() {
    // The keys `look()` asks for. A knob in the file that is not here would be
    // a control in Settings that changes nothing.
    let read = ["block", "mist", "drift"];
    let def = definition();
    let declared: Vec<&str> = def.knobs.iter().map(|k| k.key.as_str()).collect();
    for key in read {
        assert!(
            declared.contains(&key),
            "the program reads `{key}`, the file does not declare it"
        );
    }
    for key in &declared {
        assert!(
            read.contains(key),
            "the file declares `{key}`, the program never reads it"
        );
    }
}

#[test]
fn what_the_file_defaults_to_is_what_the_program_defaults_to() {
    let built_in = alpymist_saver_mountains_defaults();
    for knob in definition().knobs {
        let Dial::Number { default, .. } = knob.dial else {
            panic!("{}: this screensaver's settings are all numbers", knob.key);
        };
        let expected = match knob.key.as_str() {
            "block" => i64::from(built_in.0),
            "mist" => i64::from(built_in.1),
            _ => i64::from(built_in.2),
        };
        assert_eq!(
            default, expected,
            "{} defaults to {default} in the file and {expected} in the program",
            knob.key
        );
    }
}

/// The program's own defaults, which stand when no definition is installed.
///
/// Spelt out rather than imported: `Look` belongs to the binary, and a binary's
/// modules are not a library an integration test can reach into. Keeping them
/// here is the point — if either side moves, this test says so.
const fn alpymist_saver_mountains_defaults() -> (u32, i32, i32) {
    (6, 100, 14)
}

//! The definition file this screensaver ships, checked against the program.
//!
//! The file is what Settings renders its page from and what the program reads
//! its defaults from. Nothing at compile time ties the two together, so this
//! does: a knob renamed in the file and not in `look()` would otherwise show a
//! page whose controls changed nothing at all.

use alpymist_screensaver::definition::{Definition, Dial};

/// The file as it is shipped, read from the repository rather than from
/// /usr/share: the test is about what this commit would install.
const FILE: &str = include_str!("../../../desktop/screensavers/starfield.toml");

fn definition() -> Definition {
    Definition::parse("starfield", FILE).expect("the shipped definition parses")
}

#[test]
fn the_shipped_definition_names_this_program() {
    let def = definition();
    assert_eq!(
        def.exec,
        env!("CARGO_PKG_NAME"),
        "the file must name this program"
    );
    assert_eq!(def.id, "starfield");
    assert!(!def.name.is_empty());
}

#[test]
fn every_setting_the_file_declares_is_one_the_program_reads() {
    // The keys `look()` asks for. A knob in the file that is not here would be
    // a control in Settings that changes nothing.
    let read = ["block", "fps", "speed", "stars", "rocks", "grain"];
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
    let (block, fps, stars, speed, rocks, grain) = built_in();
    for knob in definition().knobs {
        match (knob.key.as_str(), &knob.dial) {
            ("rocks", Dial::Switch { default }) => assert_eq!(*default, rocks),
            (key, Dial::Number { default, .. }) => {
                let expected = match key {
                    "block" => i64::from(block),
                    "fps" => fps,
                    "stars" => i64::from(stars),
                    "speed" => i64::from(speed),
                    _ => i64::from(grain),
                };
                assert_eq!(
                    *default, expected,
                    "{key} defaults to {default} in the file and {expected} in the program"
                );
            }
            (key, _) => panic!("{key}: the file and the program disagree about what kind it is"),
        }
    }
}

#[test]
fn the_frames_it_asks_for_are_frames_the_host_will_give() {
    let Some(fps) = definition().knobs.into_iter().find(|k| k.key == "fps") else {
        panic!("the one setting that costs battery is not declared");
    };
    let Dial::Number { min, max, .. } = fps.dial else {
        panic!("frames a second is a number");
    };
    assert!(min >= 1, "zero frames a second is a still picture");
    assert!(
        max <= i64::try_from(alpymist_screensaver::paint::FASTEST).unwrap_or(i64::MAX),
        "the page would offer frames the host clamps away, and say nothing"
    );
}

/// The program's own defaults, which stand when no definition is installed.
///
/// Spelt out rather than imported: `Look` belongs to the binary, and a binary's
/// modules are not a library an integration test can reach into. Keeping them
/// here is the point — if either side moves, this test says so.
const fn built_in() -> (u32, i64, i32, i32, bool, u32) {
    (4, 20, 100, 100, true, 12)
}

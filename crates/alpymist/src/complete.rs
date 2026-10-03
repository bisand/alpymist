//! `alpymist complete`: what Tab offers after `alpymist …` in a shell.
//!
//! The shells' own files, in `desktop/completion`, know nothing of the
//! commands: each hands the words typed so far to `alpymist complete` and
//! shows what it prints, a value and what it is on each line with a tab
//! between. So what is offered is what this build takes, and nothing has to
//! be made again when a command or a setting is added.
//!
//! Subcommands, options and the values an option lists come from clap's own
//! description of the command line. What clap cannot know is in [`named`]:
//! the settings, a setting's values, the screens.

use alpymist_core::Channel;
use alpymist_settings::{Kind, Settings};
use clap::{Arg, Command, ValueHint};
use std::process::ExitCode;

/// How `alpymist complete` ends when the word is a file's name, which the
/// shell knows how to complete and this does not. The shells' files hold the
/// same number.
const FILES: u8 = 3;

/// One thing Tab could make of the word.
#[derive(Debug, PartialEq, Eq)]
pub struct Candidate {
    /// What is typed.
    pub value: String,
    /// What it is, in a few words; empty when there is nothing to say.
    pub about: String,
}

impl Candidate {
    fn new(value: impl Into<String>, about: impl AsRef<str>) -> Self {
        Self {
            value: value.into(),
            about: brief(about.as_ref()),
        }
    }
}

/// What to offer for a word.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer {
    /// These, and nothing when there are none.
    These(Vec<Candidate>),
    /// Files, which the shell lists.
    Files,
}

/// Print what the last of `words` could become; the words before it are what
/// was typed after `alpymist`.
pub fn run(command: &mut Command, words: &[String]) -> ExitCode {
    command.build();
    match offer(command, words) {
        Offer::Files => ExitCode::from(FILES),
        Offer::These(candidates) => {
            for c in candidates {
                if c.about.is_empty() {
                    println!("{}", c.value);
                } else {
                    println!("{}\t{}", c.value, c.about);
                }
            }
            ExitCode::SUCCESS
        }
    }
}

/// The start of a help text, short enough for a list: its first clause.
fn brief(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = [". ", ": ", "; "]
        .iter()
        .filter_map(|stop| text.find(stop))
        .min()
        .unwrap_or(text.len());
    text[..end].trim_end_matches('.').to_owned()
}

fn takes_value(arg: &Arg) -> bool {
    arg.get_action().takes_values()
}

/// What the last of `words` could become. `root` has been built.
fn offer(root: &Command, words: &[String]) -> Offer {
    let Some((current, before)) = words.split_last() else {
        return Offer::These(Vec::new());
    };
    let mut command = root;
    // The subcommands gone into, and the positional arguments given to the
    // last of them.
    let mut path: Vec<&str> = Vec::new();
    let mut given: Vec<&str> = Vec::new();
    // An option whose value is the word being completed.
    let mut awaiting: Option<&Arg> = None;
    // Past `--` or into arguments passed on to another program: no word is
    // an option or a subcommand any more.
    let mut passed_on = false;

    let mut i = 0;
    while i < before.len() {
        let word = before[i].as_str();
        i += 1;
        if passed_on {
            given.push(word);
        } else if word == "--" {
            passed_on = true;
        } else if let Some(long) = word.strip_prefix("--") {
            let option = command
                .get_arguments()
                .find(|a| a.get_long() == Some(long))
                .filter(|a| takes_value(a));
            // `--save=FILE` has its value; `--save FILE` has it next.
            if let Some(option) = option {
                if i == before.len() {
                    awaiting = Some(option);
                }
                i += 1;
            }
        } else if word == "-h" || word == "-V" {
        } else if let Some(sub) = given
            .is_empty()
            .then(|| command.find_subcommand(word))
            .flatten()
        {
            command = sub;
            path.push(sub.get_name());
        } else {
            given.push(word);
            passed_on = command
                .get_positionals()
                .nth(given.len() - 1)
                .is_some_and(Arg::is_trailing_var_arg_set);
        }
    }

    let found = if let Some(option) = awaiting {
        values(&path, option, &given)
    } else if passed_on {
        Offer::Files
    } else if current.starts_with('-') {
        Offer::These(options(command))
    } else if given.is_empty() && command.has_subcommands() {
        Offer::These(
            command
                .get_subcommands()
                .filter(|s| !s.is_hide_set())
                .map(|s| {
                    Candidate::new(
                        s.get_name(),
                        s.get_about().map(ToString::to_string).unwrap_or_default(),
                    )
                })
                .collect(),
        )
    } else if let Some(positional) = command.get_positionals().nth(given.len()) {
        if positional.is_trailing_var_arg_set() {
            Offer::Files
        } else {
            values(&path, positional, &given)
        }
    } else {
        Offer::These(Vec::new())
    };
    match found {
        Offer::Files => Offer::Files,
        Offer::These(mut candidates) => {
            candidates.retain(|c| c.value.starts_with(current.as_str()));
            Offer::These(candidates)
        }
    }
}

/// A command's options, as `--name`.
fn options(command: &Command) -> Vec<Candidate> {
    command
        .get_arguments()
        .filter(|a| !a.is_hide_set())
        .filter_map(|a| {
            let about = a.get_help().map(ToString::to_string).unwrap_or_default();
            Some(Candidate::new(format!("--{}", a.get_long()?), about))
        })
        .collect()
}

/// What an argument takes: what is named for it here, or what clap lists
/// for it, or a file.
fn values(path: &[&str], arg: &Arg, given: &[&str]) -> Offer {
    if arg.is_hide_set() {
        return Offer::These(Vec::new());
    }
    if let Some(candidates) = named(path, arg.get_id().as_str(), given) {
        return Offer::These(candidates);
    }
    let listed: Vec<Candidate> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| {
            Candidate::new(
                v.get_name(),
                v.get_help().map(ToString::to_string).unwrap_or_default(),
            )
        })
        .collect();
    if listed.is_empty()
        && matches!(
            arg.get_value_hint(),
            ValueHint::AnyPath | ValueHint::FilePath | ValueHint::DirPath
        )
    {
        return Offer::Files;
    }
    Offer::These(listed)
}

/// What clap cannot list: the values of the argument `id` of the subcommand
/// at `path`, after the positional arguments `given`. `None` for an argument
/// this has nothing to say about.
fn named(path: &[&str], id: &str, given: &[&str]) -> Option<Vec<Candidate>> {
    Some(match (path, id) {
        (["list"], "filter") => {
            let all = Settings::new();
            let mut found: Vec<Candidate> = all
                .areas()
                .iter()
                .map(|a| Candidate::new(a.id, a.title))
                .collect();
            found.extend(settings(&all, |_| true));
            found
        }
        (["set"], "id") => settings(&Settings::new(), |_| true),
        // Something to do has no value to print or to put back.
        (["get" | "reset"], "id") => settings(&Settings::new(), |kind| {
            !matches!(kind, Kind::Action { .. })
        }),
        (["set"], "value") => setting_values(&Settings::new(), given.first()?),
        (["open"], "category") => alpymist_settings::default_apps::CATEGORIES
            .iter()
            .map(|c| Candidate::new(c.id, c.title))
            .collect(),
        (["channel"], "channel") => Channel::ALL
            .iter()
            .map(|c| Candidate::new(c.name(), ""))
            .collect(),
        (["displays", "set"], "screen") => screens(),
        (["displays", "set"], "mirror") => {
            let mut found = screens();
            found.push(Candidate::new("none", "A picture of its own"));
            found
        }
        _ => return None,
    })
}

/// The settings of the kinds `wanted`, by id.
fn settings(all: &Settings, wanted: impl Fn(&Kind) -> bool) -> Vec<Candidate> {
    all.all()
        .iter()
        .filter(|s| wanted(&s.kind))
        .map(|s| Candidate::new(s.id, s.title))
        .collect()
}

/// What `alpymist set ID` can be given, for a setting with a list of values.
/// A number or a line of text has none to offer.
fn setting_values(all: &Settings, id: &str) -> Vec<Candidate> {
    match all.find(id).map(|s| &s.kind) {
        Some(Kind::Switch) => vec![Candidate::new("on", ""), Candidate::new("off", "")],
        Some(Kind::Choice(choices)) => choices
            .iter()
            .map(|c| Candidate::new(c.value.as_str(), c.label.as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// The screens connected, by connector; none when Hyprland is not there to
/// ask.
fn screens() -> Vec<Candidate> {
    alpymist_displays::hypr::monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Candidate::new(m.name.as_str(), m.model.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Offer, brief, offer};
    use clap::CommandFactory as _;

    /// What Tab offers after `alpymist` and `line`, whose last word is the
    /// one being typed: a line ending in a space is at a new word.
    fn tab(line: &str) -> Offer {
        let mut command = crate::Cli::command();
        command.build();
        let words: Vec<String> = line.split(' ').map(str::to_owned).collect();
        offer(&command, &words)
    }

    fn offered(line: &str) -> Vec<String> {
        match tab(line) {
            Offer::These(candidates) => candidates.into_iter().map(|c| c.value).collect(),
            Offer::Files => panic!("`{line}` offers files"),
        }
    }

    #[test]
    fn the_subcommands_are_offered_and_the_hidden_ones_are_not() {
        let all = offered("");
        assert!(all.contains(&"set".to_owned()));
        assert!(all.contains(&"displays".to_owned()));
        assert!(!all.contains(&"complete".to_owned()));
        assert!(!all.contains(&"menu-fragment".to_owned()));
        assert_eq!(offered("di"), ["displays"]);
        assert!(offered("displays ").contains(&"overview".to_owned()));
        assert!(!offered("displays ").contains(&"watch".to_owned()));
        assert_eq!(offered("firmware b"), ["broadcom"]);
    }

    #[test]
    fn a_dash_offers_the_commands_options() {
        assert_eq!(offered("set --"), ["--force", "--no-live", "--help"]);
        assert_eq!(offered("displays set DP-1 --mo"), ["--mode"]);
        assert!(offered("watchdog --").contains(&"--list".to_owned()));
    }

    #[test]
    fn settings_are_offered_by_id() {
        assert!(offered("set touchpad.").contains(&"touchpad.natural-scroll".to_owned()));
        assert_eq!(offered("get touchpad.natural"), ["touchpad.natural-scroll"]);
        // An option before the setting is not taken for it.
        assert_eq!(
            offered("set --force touchpad.natural"),
            ["touchpad.natural-scroll"]
        );
        // `list` takes an area too.
        assert!(offered("list touch").contains(&"touchpad".to_owned()));
    }

    #[test]
    fn something_to_do_is_offered_to_set_alone() {
        let all = alpymist_settings::Settings::new();
        let action = all
            .all()
            .iter()
            .find(|s| matches!(s.kind, alpymist_settings::Kind::Action { .. }))
            .expect("a setting that is something to do");
        let id = action.id.to_owned();
        assert!(offered(&format!("set {id}")).contains(&id));
        assert!(!offered(&format!("get {id}")).contains(&id));
        assert!(!offered(&format!("reset {id}")).contains(&id));
    }

    #[test]
    fn a_settings_values_are_offered_once_it_is_named() {
        assert_eq!(offered("set touchpad.natural-scroll "), ["on", "off"]);
        assert_eq!(offered("set --no-live touchpad.natural-scroll of"), ["off"]);
        let all = alpymist_settings::Settings::new();
        let (id, choices) = all
            .all()
            .iter()
            .find_map(|s| match &s.kind {
                alpymist_settings::Kind::Choice(choices) => Some((s.id, choices)),
                _ => None,
            })
            .expect("a setting with choices");
        let values: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(offered(&format!("set {id} ")), values);
        assert!(offered("set no.such-setting ").is_empty());
    }

    #[test]
    fn what_clap_lists_is_offered() {
        assert_eq!(offered("guest "), ["on", "off"]);
        assert_eq!(offered("probe --format "), ["human", "json"]);
        assert_eq!(offered("session p"), ["prepare"]);
        assert!(offered("key ").contains(&"volume-up".to_owned()));
        assert_eq!(offered("channel "), ["stable", "dev"]);
        assert!(offered("open ").contains(&"browser".to_owned()));
        assert_eq!(offered("displays set DP-1 --flip "), ["true", "false"]);
    }

    #[test]
    fn a_files_name_is_left_to_the_shell() {
        assert_eq!(tab("report --save "), Offer::Files);
        assert_eq!(tab("firmware broadcom --to /med"), Offer::Files);
        assert_eq!(tab("open images "), Offer::Files);
        // Passed on to the terminal's program: none of it is alpymist's.
        assert_eq!(tab("open terminal -- htop --"), Offer::Files);
        // An option's value given, the next word is the command's again.
        assert_eq!(offered("report --save here --i"), ["--issue"]);
    }

    /// The shells' files are written by hand and say two things this file
    /// says too: how it is run, and how it ends for a file's name.
    #[test]
    fn the_shells_files_run_this_and_know_how_it_ends_for_a_file() {
        let ends = super::FILES.to_string();
        for (shell, file) in [
            ("zsh", include_str!("../../../desktop/completion/_alpymist")),
            (
                "bash",
                include_str!("../../../desktop/completion/alpymist.bash"),
            ),
            (
                "fish",
                include_str!("../../../desktop/completion/alpymist.fish"),
            ),
        ] {
            let code: Vec<&str> = file
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .collect();
            assert!(
                code.iter().any(|l| l.contains("alpymist complete -- ")),
                "{shell} does not run `alpymist complete`"
            );
            assert!(
                code.iter().any(
                    |l| l.split(|c: char| !c.is_ascii_digit()).any(|n| n == ends)
                        && (l.contains("$?") || l.contains("$status"))
                ),
                "{shell} does not look for {ends}, a file's name"
            );
        }
        let command = crate::Cli::command();
        assert!(
            command
                .find_subcommand("complete")
                .is_some_and(clap::Command::is_hide_set),
            "the shells' files run `alpymist complete`"
        );
    }

    #[test]
    fn a_long_help_is_cut_to_its_first_clause() {
        assert_eq!(
            brief("Show or change the release channel: stable, or dev."),
            "Show or change the release channel"
        );
        assert_eq!(
            brief("Print a setting's\nvalue."),
            "Print a setting's value"
        );
        assert_eq!(brief(""), "");
    }
}

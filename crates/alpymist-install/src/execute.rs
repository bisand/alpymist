//! Running an install plan.
//!
//! Deliberately thin. [`crate::plan`] decides what will happen and
//! [`crate::safety`] decides whether it may; this only carries it out and says
//! what it saw. Keeping the decisions elsewhere is what lets them be tested
//! without a disk in the room.
//!
//! # Two things this does that are not obvious
//!
//! The safety check runs again immediately before the destructive step, not
//! only when the plan was built. Minutes can pass on the confirmation screen,
//! and in that time a disk can be mounted or unplugged. Checking once, early,
//! would be checking the wrong moment.
//!
//! Dry run is the default. Executing for real takes a separate, explicit
//! argument, so no caller can destroy a disk by forgetting a flag.

use crate::plan::{Plan, Step};
use crate::safety::{self, Refusal, SystemFacts};
use std::io::Write as _;
use std::process::{Command, Stdio};

/// Whether to actually do it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Report each step without running anything.
    DryRun,
    /// Really run it. Chosen explicitly, never by default.
    Commit,
}

/// What happened, as it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// A step is about to run.
    Starting {
        /// Position in the plan, from zero.
        index: usize,
        /// How many steps there are.
        total: usize,
        /// What it is doing.
        title: String,
        /// The command, for the log.
        command: String,
    },
    /// A line the step printed.
    Output(String),
    /// A step finished.
    Finished {
        /// Position in the plan.
        index: usize,
        /// Whether it succeeded.
        ok: bool,
    },
    /// The disk was not safe to touch, so nothing destructive ran.
    Refused(Vec<String>),
    /// The whole plan finished.
    Done {
        /// Whether every step succeeded.
        ok: bool,
    },
}

/// Run `plan`, reporting progress as it goes.
///
/// Stops at the first failing step: a plan is a sequence, and continuing past a
/// failed partitioning would build a system on a disk that was never prepared.
///
/// Returns whether everything succeeded.
pub fn run(plan: &Plan, mode: Mode, report: &mut dyn FnMut(Progress)) -> bool {
    run_with(plan, mode, report, &mut |step| execute(step, mode))
}

/// The body of [`run`], with command execution injected.
///
/// Taking the runner as an argument is what lets the ordering, the safety
/// re-check and the stop-on-failure behaviour be tested without running
/// anything at all.
pub fn run_with(
    plan: &Plan,
    mode: Mode,
    report: &mut dyn FnMut(Progress),
    runner: &mut dyn FnMut(&Step) -> Result<Vec<String>, String>,
) -> bool {
    let total = plan.steps.len();
    let mut checked = false;

    for (index, step) in plan.steps.iter().enumerate() {
        // Re-check at the last possible moment, once, before the first step
        // that could destroy anything.
        if step.destructive && !checked {
            checked = true;
            if mode == Mode::Commit
                && let Err(refusals) = safety::check(&plan.target, true, &safety::gather())
            {
                report(Progress::Refused(
                    refusals.iter().map(Refusal::message).collect(),
                ));
                report(Progress::Done { ok: false });
                return false;
            }
        }

        report(Progress::Starting {
            index,
            total,
            title: step.title.clone(),
            command: step.display(),
        });

        match runner(step) {
            Ok(lines) => {
                for line in lines {
                    report(Progress::Output(line));
                }
                report(Progress::Finished { index, ok: true });
            }
            Err(why) if step.may_fail => {
                report(Progress::Output(why));
                report(Progress::Output(format!(
                    "{} did not work; carrying on without it.",
                    step.title
                )));
                report(Progress::Finished { index, ok: false });
            }
            Err(why) => {
                report(Progress::Output(why));
                report(Progress::Finished { index, ok: false });
                report(Progress::Done { ok: false });
                return false;
            }
        }
    }

    report(Progress::Done { ok: true });
    true
}

/// Run one step, or describe it.
///
/// # Errors
/// Returns what went wrong, ready to show the user.
fn execute(step: &Step, mode: Mode) -> Result<Vec<String>, String> {
    if mode == Mode::DryRun {
        return Ok(vec![format!("would run: {}", step.display())]);
    }
    let Some((program, args)) = step.argv.split_first() else {
        return Err("a step with no command".into());
    };

    let mut child = Command::new(program)
        .args(args)
        .envs(step.env.iter().map(|(k, v)| (k.clone(), v.clone())))
        // Never inherit the installer's own stdin: a command that stops to ask
        // a question would otherwise wait forever on a keyboard it cannot see.
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    {
        // Dropped at the end of this block, which closes the pipe: the command
        // sees end of input rather than waiting for more.
        let mut pipe = child.stdin.take().ok_or("could not open standard input")?;
        if let Some(input) = &step.stdin {
            pipe.write_all(input.bytes())
                .map_err(|e| format!("could not feed {program}: {e}"))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("{program} did not finish: {e}"))?;

    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .map(str::to_string)
        .collect();

    if output.status.success() {
        Ok(lines)
    } else {
        lines.push(format!("{program} failed: {}", output.status));
        Err(lines.join("\n"))
    }
}

/// Check a target before anything is planned, for the confirmation screen.
///
/// # Errors
/// Returns the reasons it may not be written to.
pub fn preflight(target: &str, confirmed: bool) -> Result<(), Vec<String>> {
    safety::check(target, confirmed, &SystemFacts::default_for_host())
        .map_err(|refusals| refusals.iter().map(Refusal::message).collect())
}

#[cfg(test)]
mod tests {
    use super::{Mode, Progress, run_with};
    use crate::plan::{Plan, Step};

    fn step(title: &str, destructive: bool) -> Step {
        let mut s = Step {
            title: title.to_string(),
            argv: vec!["true".to_string()],
            env: Vec::new(),
            stdin: None,
            destructive: false,
            may_fail: false,
        };
        s.destructive = destructive;
        s
    }

    fn plan() -> Plan {
        Plan {
            steps: vec![
                step("look", false),
                step("erase", true),
                step("configure", false),
            ],
            target: "/dev/sdb".to_string(),
        }
    }

    fn collect(mode: Mode, outcome: &'static [bool]) -> (bool, Vec<Progress>) {
        let mut seen = Vec::new();
        let mut n = 0;
        let ok = run_with(&plan(), mode, &mut |p| seen.push(p), &mut |_step| {
            let good = outcome.get(n).copied().unwrap_or(true);
            n += 1;
            if good {
                Ok(vec!["fine".into()])
            } else {
                Err("it broke".into())
            }
        });
        (ok, seen)
    }

    #[test]
    fn every_step_runs_in_order_and_reports_itself() {
        let (ok, seen) = collect(Mode::DryRun, &[true, true, true]);
        assert!(ok);
        let titles: Vec<String> = seen
            .iter()
            .filter_map(|p| match p {
                Progress::Starting { title, .. } => Some(title.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["look", "erase", "configure"]);
    }

    /// Continuing past a failed partitioning would build a system on a disk
    /// that was never prepared.
    #[test]
    fn a_failed_step_stops_the_plan() {
        let (ok, seen) = collect(Mode::DryRun, &[true, false, true]);
        assert!(!ok);
        let started = seen
            .iter()
            .filter(|p| matches!(p, Progress::Starting { .. }))
            .count();
        assert_eq!(started, 2, "the third step should never have started");
        assert!(matches!(seen.last(), Some(Progress::Done { ok: false })));
    }

    /// A step allowed to fail is reported, and the rest still runs.
    #[test]
    fn a_step_that_may_fail_does_not_stop_the_plan() {
        let mut p = plan();
        p.steps[2].may_fail = true;
        p.steps.push(step("unmount", false));
        let mut seen = Vec::new();
        let mut n = 0;
        let ok = run_with(&p, Mode::DryRun, &mut |e| seen.push(e), &mut |_| {
            n += 1;
            if n == 3 {
                Err("no such package".into())
            } else {
                Ok(Vec::new())
            }
        });
        assert!(ok, "an optional failure is not a failed install");
        assert_eq!(n, 4, "the step after it did not run");
        assert!(
            seen.iter()
                .any(|e| matches!(e, Progress::Output(l) if l.contains("no such package")))
        );
    }

    #[test]
    fn a_finished_run_says_so_exactly_once() {
        let (_, seen) = collect(Mode::DryRun, &[true, true, true]);
        let dones = seen
            .iter()
            .filter(|p| matches!(p, Progress::Done { .. }))
            .count();
        assert_eq!(dones, 1);
    }

    #[test]
    fn each_step_reports_the_command_it_would_run() {
        let (_, seen) = collect(Mode::DryRun, &[true, true, true]);
        assert!(
            seen.iter()
                .any(|p| matches!(p, Progress::Starting { command, .. } if !command.is_empty()))
        );
    }

    /// A dry run must never reach the safety re-check, because it never touches
    /// anything — and must never be refused for a disk it is not going to write.
    #[test]
    fn a_dry_run_is_never_refused() {
        let (ok, seen) = collect(Mode::DryRun, &[true, true, true]);
        assert!(ok);
        assert!(!seen.iter().any(|p| matches!(p, Progress::Refused(_))));
    }

    #[test]
    fn the_failing_step_is_the_one_reported_as_failed() {
        let (_, seen) = collect(Mode::DryRun, &[true, false, true]);
        let failed: Vec<usize> = seen
            .iter()
            .filter_map(|p| match p {
                Progress::Finished { index, ok: false } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(failed, [1]);
    }
}

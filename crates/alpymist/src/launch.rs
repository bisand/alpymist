//! Starting a program and leaving it: its own process group, nothing
//! inherited on stdin or stdout, so this can exit at once and nothing it
//! started goes with it.

use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};

/// Start `argv` in the home directory, and do not wait.
pub fn spawn(argv: &[String]) -> std::io::Result<()> {
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("nothing to run"))?;
    let mut command = Command::new(program);
    command
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(home) = std::env::var_os("HOME") {
        command.current_dir(home);
    }
    command.spawn().map(drop)
}

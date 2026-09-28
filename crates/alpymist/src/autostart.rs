//! `alpymist autostart`: start what Settings › Startup says starts at login.
//!
//! Run once by Hyprland, from the configuration the package ships, so an
//! account made before this existed starts its programs too. Each is started
//! and left: its own process group, nothing inherited on stdin or stdout, so
//! this can exit at once and nothing it started goes with it. A program that
//! cannot be started is said on stderr, which is Hyprland's log, and the rest
//! still start.

use alpymist_settings::{Env, startup};
use std::process::Command;

/// What D-Bus is told about the session before anything is started, as
/// `hyprland.conf` tells it: this can run first, and a program that starts a
/// portal before then gets one that knows nothing of Hyprland.
const HANDED_OVER: &[&str] = &[
    "WAYLAND_DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "HYPRLAND_INSTANCE_SIGNATURE",
];

/// Start every program turned on, or with `dry_run` say what they are.
pub fn run(env: &Env, dry_run: bool) {
    let programs: Vec<_> = startup::programs(env)
        .into_iter()
        .filter(startup::Program::enabled)
        .collect();
    if !dry_run
        && !programs.is_empty()
        && let Err(e) = Command::new("dbus-update-activation-environment")
            .args(HANDED_OVER)
            .status()
    {
        eprintln!("alpymist autostart: dbus-update-activation-environment: {e}");
    }
    for p in programs {
        let Some(argv) = p.argv() else {
            eprintln!("alpymist autostart: {} has nothing to run", p.file);
            continue;
        };
        if dry_run {
            println!("{}\t{}", p.file, argv.join(" "));
            continue;
        }
        if let Err(e) = crate::launch::spawn(&argv) {
            eprintln!("alpymist autostart: {}: {}: {e}", p.file, argv[0]);
        }
    }
}

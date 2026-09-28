//! `alpymist clipboard …`: the keys, the history's daemon, and the history
//! itself for the menu and scripts. The work is in `alpymist-clipboard`.

use alpymist_clipboard::keys::{self, Which};
use alpymist_clipboard::protocol::Request;
use alpymist_clipboard::{client, config::Config, daemon};
use clap::Subcommand;

/// What to do with the clipboard.
#[derive(Subcommand)]
pub enum Action {
    /// Copy what is selected in the focused window (Super+C).
    Copy,
    /// Paste into the focused window (Super+V).
    Paste,
    /// Start keeping a history, if Settings › Clipboard says to; Hyprland
    /// runs this at login. A history already kept is started again.
    Start,
    /// Stop keeping a history, and forget what this session held.
    Stop,
    /// List the history, newest first: id, whether pinned, and a line.
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Put an entry back on the clipboard and paste it (the menu's picker).
    Restore {
        /// The entry, as `list` numbers it.
        id: u64,
    },
    /// Keep an entry whatever happens.
    Pin {
        /// The entry.
        id: u64,
    },
    /// Let a pinned entry go again.
    Unpin {
        /// The entry.
        id: u64,
    },
    /// Forget one entry.
    Forget {
        /// The entry.
        id: u64,
    },
    /// Forget everything but what is pinned.
    Clear,
    /// The screen locked: forget all but what is pinned, if Settings says so.
    #[command(hide = true)]
    Lock,
    /// Read Settings › Clipboard again.
    #[command(hide = true)]
    Reload,
    /// Keep the history, in the foreground.
    #[command(hide = true)]
    Daemon,
    /// Take what was just copied: what `wl-paste --watch` runs.
    #[command(hide = true)]
    Store,
}

/// Do it.
pub fn run(action: &Action) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        Action::Copy => keys::send(Which::Copy)?,
        Action::Paste => keys::send(Which::Paste)?,
        Action::Start => start()?,
        Action::Stop => {
            if client::running() {
                client::tell(&Request::Stop)?;
            }
        }
        Action::List { json } => {
            let list = client::list()?;
            if *json {
                println!("{}", serde_json::to_string(&list)?);
            } else {
                for e in list {
                    println!(
                        "{}\t{}\t{}",
                        e.id,
                        if e.pinned { "pinned" } else { "" },
                        e.label
                    );
                }
            }
        }
        Action::Restore { id } => keys::restore(*id)?,
        Action::Pin { id } => expect(client::tell(&Request::Pin(*id))?, *id)?,
        Action::Unpin { id } => expect(client::tell(&Request::Unpin(*id))?, *id)?,
        Action::Forget { id } => expect(client::tell(&Request::Forget(*id))?, *id)?,
        // No history kept, nothing to forget or tell.
        Action::Clear => quietly(&Request::Clear)?,
        Action::Lock => quietly(&Request::Lock)?,
        Action::Reload => quietly(&Request::Reload)?,
        Action::Daemon => daemon::run()?,
        Action::Store => daemon::store()?,
    }
    Ok(())
}

/// Tell the daemon, if one is running.
fn quietly(request: &Request) -> Result<(), String> {
    if client::running() {
        client::tell(request)?;
    }
    Ok(())
}

fn expect(found: bool, id: u64) -> Result<(), String> {
    if found {
        Ok(())
    } else {
        Err(format!("no entry {id} in the history"))
    }
}

/// Start the daemon on its own, and return: it stops any other itself, and
/// with no history in Settings it does no more than that.
fn start() -> Result<(), String> {
    if !Config::load().history {
        if client::running() {
            client::tell(&Request::Stop)?;
        }
        return Ok(());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let argv = vec![
        me.to_string_lossy().into_owned(),
        "clipboard".to_owned(),
        "daemon".to_owned(),
    ];
    crate::launch::spawn(&argv).map_err(|e| format!("the clipboard daemon: {e}"))
}

//! The history's keeper: one process a session, holding the history in
//! memory, answering on a socket in the session's runtime directory.
//!
//! It is fed by `wl-paste --watch`, which runs `alpymist clipboard store`
//! every time something is copied; that asks what types are on offer, takes
//! the text or the image, and hands it over. wl-clipboard says when the
//! program that copied marked it secret — password managers do
//! (`x-kde-passwordManagerHint`) — and that is never taken.
//!
//! Started by Hyprland at login and by Settings when the history is turned
//! on; a second start stops the first. Without a history in Settings it does
//! not start at all. When the session goes, `wl-paste` goes with it, and so
//! does this.

use crate::config::{self, Config};
use crate::history::{self, History};
use crate::protocol::Request;
use std::io::{BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Stdio};

/// Keep the history until told to stop, or until the session ends.
///
/// # Errors
/// There is no session, or the socket could not be made.
pub fn run() -> Result<(), String> {
    let mut settings = Config::load();
    let socket = config::socket_path().ok_or("no session: XDG_RUNTIME_DIR is not set")?;
    // Whichever daemon is there stops: this one has the settings as they are.
    if crate::client::running() {
        let _ = crate::client::tell(&Request::Stop);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    if !settings.history {
        forget_file(&settings);
        return Ok(());
    }
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).map_err(|e| format!("{}: {e}", socket.display()))?;
    restrict(&socket);

    let mut history = if settings.remember {
        History::load(&config::history_path())
    } else {
        History::default()
    };
    history.trim(settings.limit());

    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut watcher = Command::new("wl-paste")
        .arg("--watch")
        .arg(&me)
        .args(["clipboard", "store"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("wl-paste: {e}"))?;
    // When the watcher ends, which is when the session does, this does too.
    let pid = watcher.id();
    std::thread::spawn(move || {
        let _ = watcher.wait();
        let _ = crate::client::tell(&Request::Stop);
    });

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        match serve(stream, &mut history, &mut settings) {
            Served::Changed if settings.remember => {
                if let Err(e) = history.save(&config::history_path()) {
                    eprintln!("alpymist clipboard: keeping the history: {e}");
                }
            }
            Served::Stop => break,
            _ => {}
        }
    }
    let _ = Command::new("kill").arg(pid.to_string()).status();
    let _ = std::fs::remove_file(&socket);
    Ok(())
}

/// What answering a request did.
enum Served {
    Unchanged,
    Changed,
    Stop,
}

fn serve(mut stream: UnixStream, history: &mut History, settings: &mut Config) -> Served {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    let request = match Request::read(&mut BufReader::new(&stream), history::MAX_IMAGE) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stream, "error {e}");
            return Served::Unchanged;
        }
    };
    let answer = |stream: &mut UnixStream, ok: bool| {
        let _ = writeln!(stream, "{}", if ok { "ok" } else { "none" });
    };
    match request {
        Request::Add { mime, data } => {
            let changed = history.add(&mime, data, settings.limit());
            answer(&mut stream, true);
            return if changed {
                Served::Changed
            } else {
                Served::Unchanged
            };
        }
        Request::List => {
            let list: Vec<_> = history
                .entries()
                .iter()
                .map(history::Entry::summary)
                .collect();
            let _ = writeln!(
                stream,
                "{}",
                serde_json::to_string(&list).unwrap_or_default()
            );
        }
        Request::Get(id) => match history.get(id) {
            Some(e) => {
                let _ = writeln!(stream, "{} {}", e.mime, e.data.len());
                let _ = stream.write_all(&e.data);
            }
            None => answer(&mut stream, false),
        },
        Request::Pin(id) | Request::Unpin(id) => {
            let ok = history.pin(id, matches!(request, Request::Pin(_)));
            answer(&mut stream, ok);
            return if ok {
                Served::Changed
            } else {
                Served::Unchanged
            };
        }
        Request::Forget(id) => {
            let ok = history.forget(id);
            answer(&mut stream, ok);
            return if ok {
                Served::Changed
            } else {
                Served::Unchanged
            };
        }
        Request::Clear => {
            history.clear();
            answer(&mut stream, true);
            return Served::Changed;
        }
        Request::Lock => {
            if settings.clear_on_lock {
                history.clear();
            }
            answer(&mut stream, true);
            return Served::Changed;
        }
        Request::Reload => {
            *settings = Config::load();
            history.trim(settings.limit());
            forget_file(settings);
            answer(&mut stream, true);
            return Served::Changed;
        }
        Request::Stop => {
            answer(&mut stream, true);
            return Served::Stop;
        }
    }
    Served::Unchanged
}

/// A history no longer to be remembered is not left on disk.
fn forget_file(settings: &Config) {
    if !settings.remember {
        let _ = std::fs::remove_file(config::history_path());
    }
}

/// Only this account may speak to the socket. The runtime directory is its
/// alone already; this is the socket saying so too.
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

/// What `wl-paste --watch` runs: take what was just copied, unless it was
/// marked secret, and give it to the daemon.
///
/// # Errors
/// There is no daemon to give it to.
pub fn store() -> Result<(), String> {
    // wl-clipboard sets this for the command it runs: `sensitive` when the
    // copy was marked secret, `clear` or `nil` when there is nothing.
    if std::env::var("CLIPBOARD_STATE").is_ok_and(|s| s != "data") {
        return Ok(());
    }
    let listed = String::from_utf8_lossy(&output(&["wl-paste", "--list-types"])?).into_owned();
    let types: Vec<&str> = listed
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    let Some(chosen) = choose(&types) else {
        return Ok(());
    };
    let data = output(&["wl-paste", "--no-newline", "--type", chosen])?;
    crate::client::tell(&Request::Add {
        mime: chosen.to_owned(),
        data,
    })
    .map(drop)
}

/// Text types, the one to take first.
const TEXT: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

/// Which of the offered types to keep: text before an image, and nothing at
/// all when a password manager says it is a secret (for a wl-clipboard too
/// old to say so itself).
fn choose<'a>(types: &[&'a str]) -> Option<&'a str> {
    if types.contains(&"x-kde-passwordManagerHint") {
        return None;
    }
    TEXT.iter()
        .find_map(|t| types.iter().find(|o| **o == *t).copied())
        .or_else(|| types.iter().find(|t| **t == "image/png").copied())
        .or_else(|| types.iter().find(|t| t.starts_with("image/")).copied())
}

fn output(argv: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", argv[0]))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("{} failed", argv[0]))
    }
}

#[cfg(test)]
mod tests {
    use super::{Served, choose, serve};
    use crate::config::Config;
    use crate::history::{History, Summary};
    use crate::protocol::Request;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;

    /// Ask the daemon's `serve` one thing, as a client would, and read back
    /// its whole answer.
    fn ask(history: &mut History, settings: &mut Config, request: &Request) -> (Vec<u8>, bool) {
        let (mut client, daemon) = UnixStream::pair().unwrap();
        request.write(&mut client).unwrap();
        client.flush().unwrap();
        let served = serve(daemon, history, settings);
        let mut answer = Vec::new();
        client.read_to_end(&mut answer).unwrap();
        (answer, matches!(served, Served::Stop))
    }

    #[test]
    fn what_is_added_is_listed_got_pinned_and_forgotten_at_a_lock() {
        let mut history = History::default();
        let mut settings = Config {
            history: true,
            ..Config::default()
        };
        for text in ["first", "second"] {
            let add = Request::Add {
                mime: "text/plain".into(),
                data: text.as_bytes().to_vec(),
            };
            assert_eq!(ask(&mut history, &mut settings, &add).0, b"ok\n");
        }
        let (list, _) = ask(&mut history, &mut settings, &Request::List);
        let list: Vec<Summary> = serde_json::from_slice(&list).unwrap();
        let labels: Vec<&str> = list.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["second", "first"]);

        let (got, _) = ask(&mut history, &mut settings, &Request::Get(list[1].id));
        let mut reader = BufReader::new(got.as_slice());
        let mut head = String::new();
        reader.read_line(&mut head).unwrap();
        assert_eq!(head, "text/plain 5\n");
        let mut body = Vec::new();
        reader.read_to_end(&mut body).unwrap();
        assert_eq!(body, b"first");

        ask(&mut history, &mut settings, &Request::Pin(list[1].id));
        ask(&mut history, &mut settings, &Request::Lock);
        assert_eq!(history.entries().len(), 1, "only the pinned one is left");
        assert_eq!(
            ask(&mut history, &mut settings, &Request::Get(999)).0,
            b"none\n"
        );
        let (_, stopped) = ask(&mut history, &mut settings, &Request::Stop);
        assert!(stopped);
    }

    #[test]
    fn text_first_then_an_image_and_never_a_secret() {
        assert_eq!(
            choose(&["image/png", "text/plain", "text/plain;charset=utf-8"]),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(choose(&["image/jpeg", "image/png"]), Some("image/png"));
        assert_eq!(choose(&["image/webp"]), Some("image/webp"));
        assert_eq!(choose(&["application/x-something"]), None);
        assert_eq!(choose(&["text/plain", "x-kde-passwordManagerHint"]), None);
    }
}

//! Asking the daemon: what the menu, Settings, the lock screen and
//! `alpymist clipboard` use.

use crate::history::Summary;
use crate::protocol::Request;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// How long an answer may take. The daemon answers from memory.
const WAIT: Duration = Duration::from_secs(2);

fn connect() -> Result<UnixStream, String> {
    let path = crate::config::socket_path().ok_or("no session to find the clipboard in")?;
    let stream =
        UnixStream::connect(&path).map_err(|_| "the clipboard history is off".to_owned())?;
    stream.set_read_timeout(Some(WAIT)).ok();
    stream.set_write_timeout(Some(WAIT)).ok();
    Ok(stream)
}

fn ask(request: &Request) -> Result<BufReader<UnixStream>, String> {
    let mut stream = connect()?;
    request.write(&mut stream).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    Ok(BufReader::new(stream))
}

fn line(reader: &mut impl BufRead) -> Result<String, String> {
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim_end().to_owned())
}

/// Whether a daemon is keeping a history in this session.
#[must_use]
pub fn running() -> bool {
    connect().is_ok()
}

/// Send a request that is answered `ok` or `none`. Returns whether it was
/// `ok`.
///
/// # Errors
/// No daemon, or it did not answer.
pub fn tell(request: &Request) -> Result<bool, String> {
    Ok(line(&mut ask(request)?)? == "ok")
}

/// Every entry, newest first.
///
/// # Errors
/// No daemon, or it did not answer.
pub fn list() -> Result<Vec<Summary>, String> {
    serde_json::from_str(&line(&mut ask(&Request::List)?)?).map_err(|e| e.to_string())
}

/// One entry's type and bytes.
///
/// # Errors
/// No daemon, no such entry, or it did not answer.
pub fn get(id: u64) -> Result<(String, Vec<u8>), String> {
    let mut reader = ask(&Request::Get(id))?;
    let head = line(&mut reader)?;
    let (mime, len) = head
        .rsplit_once(' ')
        .and_then(|(m, l)| Some((m.to_owned(), l.parse::<usize>().ok()?)))
        .ok_or_else(|| format!("no entry {id}"))?;
    let mut data = vec![0; len];
    reader.read_exact(&mut data).map_err(|e| e.to_string())?;
    Ok((mime, data))
}

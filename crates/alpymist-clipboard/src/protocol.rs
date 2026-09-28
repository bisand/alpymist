//! What goes down the daemon's socket: one request a connection, a line
//! naming it, and for an entry its bytes after that line.
//!
//! ```text
//! add MIME LEN\n<bytes>    something was copied        → ok
//! list\n                   the entries, as JSON        → [...]\n
//! get ID\n                 one entry                   → MIME LEN\n<bytes> | none
//! pin ID | unpin ID | forget ID | clear | lock | reload | stop  → ok | none
//! ```

use std::io::{BufRead, Write};

/// A request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Something was copied.
    Add {
        /// Its type.
        mime: String,
        /// Its bytes.
        data: Vec<u8>,
    },
    /// Every entry's summary.
    List,
    /// One entry's bytes.
    Get(u64),
    /// Pin one.
    Pin(u64),
    /// Unpin one.
    Unpin(u64),
    /// Forget one.
    Forget(u64),
    /// Forget all but what is pinned.
    Clear,
    /// The screen locked: forget all but what is pinned, if Settings says so.
    Lock,
    /// Read the settings again.
    Reload,
    /// Forget everything this session holds, and exit.
    Stop,
}

impl Request {
    /// Send it.
    ///
    /// # Errors
    /// Writing failed.
    pub fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        match self {
            Self::Add { mime, data } => {
                writeln!(out, "add {mime} {}", data.len())?;
                out.write_all(data)
            }
            Self::List => writeln!(out, "list"),
            Self::Get(id) => writeln!(out, "get {id}"),
            Self::Pin(id) => writeln!(out, "pin {id}"),
            Self::Unpin(id) => writeln!(out, "unpin {id}"),
            Self::Forget(id) => writeln!(out, "forget {id}"),
            Self::Clear => writeln!(out, "clear"),
            Self::Lock => writeln!(out, "lock"),
            Self::Reload => writeln!(out, "reload"),
            Self::Stop => writeln!(out, "stop"),
        }
    }

    /// Read one. `max` bounds what an `add` may carry.
    ///
    /// # Errors
    /// Not a request, or reading failed.
    pub fn read(input: &mut impl BufRead, max: usize) -> Result<Self, String> {
        let mut line = String::new();
        input.read_line(&mut line).map_err(|e| e.to_string())?;
        let words: Vec<&str> = line.split_whitespace().collect();
        let id = |w: Option<&&str>| {
            w.and_then(|w| w.parse::<u64>().ok())
                .ok_or_else(|| format!("no entry in `{}`", line.trim()))
        };
        Ok(match words.first().copied() {
            Some("add") => {
                let (Some(mime), Some(len)) = (words.get(1), words.get(2)) else {
                    return Err("add needs a type and a length".into());
                };
                let len: usize = len.parse().map_err(|_| "a length".to_owned())?;
                if len > max {
                    return Err(format!("{len} bytes is more than is kept"));
                }
                let mut data = vec![0; len];
                input.read_exact(&mut data).map_err(|e| e.to_string())?;
                Self::Add {
                    mime: (*mime).to_owned(),
                    data,
                }
            }
            Some("list") => Self::List,
            Some("get") => Self::Get(id(words.get(1))?),
            Some("pin") => Self::Pin(id(words.get(1))?),
            Some("unpin") => Self::Unpin(id(words.get(1))?),
            Some("forget") => Self::Forget(id(words.get(1))?),
            Some("clear") => Self::Clear,
            Some("lock") => Self::Lock,
            Some("reload") => Self::Reload,
            Some("stop") => Self::Stop,
            _ => return Err(format!("no such request: `{}`", line.trim())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn every_request_reads_back_as_written() {
        for r in [
            Request::Add {
                mime: "text/plain".into(),
                data: b"two\nlines".to_vec(),
            },
            Request::List,
            Request::Get(7),
            Request::Pin(1),
            Request::Unpin(1),
            Request::Forget(2),
            Request::Clear,
            Request::Lock,
            Request::Reload,
            Request::Stop,
        ] {
            let mut bytes = Vec::new();
            r.write(&mut bytes).unwrap();
            assert_eq!(Request::read(&mut bytes.as_slice(), 100), Ok(r));
        }
        assert!(Request::read(&mut "add text/plain 999\n".as_bytes(), 100).is_err());
        assert!(Request::read(&mut "get x\n".as_bytes(), 100).is_err());
    }
}

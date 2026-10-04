//! Ask a PAM service about a user, by hand: `ask SERVICE USER`, with the
//! password as the first line of standard input. Says what PAM said, and what
//! it asked on the way. For trying a stack out; nothing ships it.

use alpymist_pam::{Conversation, Error};
use std::io::BufRead as _;

struct Terminal;

impl Conversation for Terminal {
    fn prompt(&self, request: &str) -> Result<String, Error> {
        eprintln!("asked: {request}");
        Err(Error::Conversation)
    }

    fn masked_prompt(&self, request: &str) -> Result<String, Error> {
        eprintln!("asked, hidden: {request}");
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|_| Error::Conversation)?;
        Ok(line.trim_end_matches('\n').to_owned())
    }

    fn error(&self, message: &str) {
        eprintln!("error: {message}");
    }

    fn info(&self, message: &str) {
        eprintln!("info: {message}");
    }
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(service), Some(user)) = (args.next(), args.next()) else {
        eprintln!("usage: ask SERVICE USER < password");
        return 2.into();
    };
    #[cfg(target_os = "linux")]
    let result = alpymist_pam::authenticate(&service, &user, &Terminal);
    #[cfg(not(target_os = "linux"))]
    let result: Result<(), Error> = {
        let _ = (service, user, Terminal);
        Err(Error::System)
    };
    match result {
        Ok(()) => {
            println!("yes");
            0.into()
        }
        Err(e) => {
            println!("no: {e}");
            1.into()
        }
    }
}

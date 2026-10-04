//! A channel that wakes the event loop.
//!
//! A widget's own threads — reading a daemon, running a command that blocks —
//! send what they find through a [`Sender`]. It is `std`'s channel with a pipe
//! beside it: a byte written for every message, so the loop, which sleeps in
//! `poll`, wakes when there is something to take.

use rustix::pipe::{PipeFlags, pipe_with};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::mpsc;

/// The sending end: clone it into each thread.
#[derive(Debug)]
pub struct Sender<E> {
    messages: mpsc::Sender<E>,
    wake: Arc<OwnedFd>,
}

impl<E> Clone for Sender<E> {
    fn clone(&self) -> Self {
        Self {
            messages: self.messages.clone(),
            wake: Arc::clone(&self.wake),
        }
    }
}

impl<E> Sender<E> {
    /// Send `event` to the loop.
    ///
    /// # Errors
    /// When the receiving end is gone, which is the program on its way out;
    /// the event comes back.
    pub fn send(&self, event: E) -> Result<(), mpsc::SendError<E>> {
        self.messages.send(event)?;
        // A full pipe means the loop already has plenty to wake up for.
        let _ = rustix::io::write(&*self.wake, &[1]);
        Ok(())
    }
}

/// The receiving end, for [`crate::Wayland::watch`] and then
/// [`Receiver::take`].
#[derive(Debug)]
pub struct Receiver<E> {
    messages: mpsc::Receiver<E>,
    woken: OwnedFd,
}

impl<E> Receiver<E> {
    /// What becomes readable when something has been sent.
    #[must_use]
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.woken.as_fd()
    }

    /// Everything sent so far.
    #[must_use]
    pub fn take(&self) -> Vec<E> {
        // The pipe first: a message sent after this is read leaves a byte
        // behind and the loop comes round again, where the other order could
        // leave a message with nothing to wake for it.
        let mut bytes = [0u8; 256];
        while matches!(rustix::io::read(&self.woken, &mut bytes), Ok(n) if n == bytes.len()) {}
        self.messages.try_iter().collect()
    }
}

/// A channel into the event loop.
///
/// # Panics
/// When the system has no file descriptors left for a pipe, at start-up.
#[must_use]
pub fn channel<E>() -> (Sender<E>, Receiver<E>) {
    let (messages, received) = mpsc::channel();
    let (woken, wake) = pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK)
        .expect("a pipe to wake the event loop with");
    (
        Sender {
            messages,
            wake: Arc::new(wake),
        },
        Receiver {
            messages: received,
            woken,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    fn readable<E>(receiver: &Receiver<E>) -> bool {
        let mut fds = [PollFd::new(&receiver.woken, PollFlags::IN)];
        let now = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        poll(&mut fds, Some(&now)).unwrap() == 1
    }

    #[test]
    fn a_message_wakes_and_taking_it_settles() {
        let (sender, receiver) = channel();
        assert!(!readable(&receiver));
        sender.send(7).unwrap();
        sender.clone().send(8).unwrap();
        assert!(readable(&receiver));
        assert_eq!(receiver.take(), [7, 8]);
        assert!(!readable(&receiver));
        assert!(receiver.take().is_empty());
    }

    #[test]
    fn more_messages_than_a_pipe_holds_all_arrive() {
        let (sender, receiver) = channel();
        for n in 0..100_000u32 {
            sender.send(n).unwrap();
        }
        assert_eq!(receiver.take().len(), 100_000);
        assert!(!readable(&receiver));
    }

    #[test]
    fn sending_to_nobody_gives_the_message_back() {
        let (sender, receiver) = channel();
        drop(receiver);
        assert_eq!(sender.send(3).unwrap_err().0, 3);
    }
}

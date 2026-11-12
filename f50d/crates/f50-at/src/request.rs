//! What callers put in a lane's queue, and the ticket that makes "busy after `queue_wait`" exact.
//!
//! A queued request carries a ticket that is `Queued` until the channel task takes it (`Taken`) or the caller gives
//! up waiting (`Abandoned`); one compare-and-swap on each side decides which came first. So a caller is told `busy`
//! only when its command never reached the modem, and a command the task took is always answered (within its
//! timeout), never silently dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use f50_core::{Error, Result};
use tokio::sync::{mpsc, oneshot};

use crate::Reply;

const QUEUED: u8 = 0;
const TAKEN: u8 = 1;
const ABANDONED: u8 = 2;

/// The queue a lane's requests wait in.
pub(crate) const QUEUE_MAX: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct Ticket(Arc<AtomicU8>);

impl Ticket {
    pub(crate) fn new() -> Self {
        Ticket(Arc::new(AtomicU8::new(QUEUED)))
    }

    /// The channel task's side: true when the caller still waits, and from now on it will be answered.
    pub(crate) fn take(&self) -> bool {
        self.0.compare_exchange(QUEUED, TAKEN, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    /// The caller's side: true when the request was never taken, and now never will be.
    fn abandon(&self) -> bool {
        self.0.compare_exchange(QUEUED, ABANDONED, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
}

/// One command for the wire.
#[derive(Debug)]
pub(crate) struct Command {
    pub(crate) cmd: String,
    pub(crate) body: Option<String>,
    pub(crate) timeout: Duration,
    pub(crate) reply: oneshot::Sender<Result<Reply>>,
}

/// What waits in a lane.
#[derive(Debug)]
pub(crate) enum Job {
    Command(Command),
    /// The control channel to one owner until the sender it gets back is dropped.
    Session {
        owner: &'static str,
        reply: oneshot::Sender<Result<mpsc::Sender<Command>>>,
    },
}

#[derive(Debug)]
pub(crate) struct Queued {
    pub(crate) ticket: Ticket,
    pub(crate) job: Job,
}

fn stopped() -> Error {
    Error::busy("the AT channel is stopping")
}

/// Puts `job` in `lane` and waits for the answer: `busy` when it is not taken within `queue_wait`.
pub(crate) async fn submit<T>(
    lane: &mpsc::Sender<Queued>,
    job: Job,
    rx: oneshot::Receiver<T>,
    queue_wait: Duration,
) -> Result<T> {
    let ticket = Ticket::new();
    lane.try_send(Queued { ticket: ticket.clone(), job }).map_err(|e| match e {
        mpsc::error::TrySendError::Full(_) => Error::busy("the AT channel is busy (too many commands waiting)"),
        mpsc::error::TrySendError::Closed(_) => stopped(),
    })?;
    let mut rx = rx;
    tokio::select! {
        r = &mut rx => return r.map_err(|_| stopped()),
        () = tokio::time::sleep(queue_wait) => {}
    }
    if ticket.abandon() {
        return Err(Error::busy("the AT channel is busy; the command was not sent"));
    }
    // taken a moment ago: the channel task answers within the command's own timeout
    rx.await.map_err(|_| stopped())
}

/// Checks one command line: printable ASCII, no CR or LF (a second command), not empty.
pub(crate) fn check_line(what: &str, s: &str, max: usize) -> Result<()> {
    if s.is_empty() || s.len() > max || !s.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(Error::new(
            f50_core::Code::BadValue,
            format!("{what}: one line of printable ASCII, at most {max} bytes"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_is_decided_once() {
        let t = Ticket::new();
        assert!(t.take());
        assert!(!t.abandon());
        let t = Ticket::new();
        assert!(t.abandon());
        assert!(!t.take());
    }

    #[test]
    fn lines() {
        assert!(check_line("cmd", "AT+CSQ", 16).is_ok());
        assert!(check_line("cmd", "", 16).is_err());
        assert!(check_line("cmd", "AT\rAT", 16).is_err());
        assert!(check_line("cmd", "AT\x1a", 16).is_err());
        assert!(check_line("cmd", "AT+CSQ", 3).is_err());
    }
}

//! The one reader of a channel: a blocking loop on a dedicated thread that turns bytes into lines.
//!
//! **Why a thread and not `AsyncFd`.** The SIPC driver's `poll()` is the one readiness mechanism measured to work on
//! these channels (bash's and busybox ash's `read -t`, docs/research/FINDINGS.md 13c); epoll's edge-triggered mode on
//! this vendor driver and its handling of `O_NONBLOCK` are not measured, and a read that ends up in `sbuf_read`
//! blocks the calling thread until the modem talks. A thread of its own (taken from the runtime's blocking pool
//! through the daemon's `TaskTracker`) runs exactly the measured `poll(POLLIN, 250 ms)` + `read` loop, can never stall
//! an async worker, and checks its stop token between polls, so shutdown and a reopen end it within a quarter second.
//!
//! The lines go to the channel task over a bounded queue (`blocking_send`: when the task is behind, the bytes wait in
//! the modem's sbuf ring rather than being dropped). Nothing is ever discarded here: a URC that arrives mid-command
//! reaches the channel task like any other line.

use std::fs::File;
use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::sys::{self, Ready};

/// The longest line kept; a longer run of bytes without an end of line is passed on in pieces.
const LINE_MAX: usize = 8 * 1024;
/// How often the stop token is looked at while the channel is quiet.
const POLL_SLICE: Duration = Duration::from_millis(250);
/// Read errors in a row after which the channel is reported closed.
const ERRORS_MAX: u32 = 25;

/// What the reader hands the channel task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// One non-empty line, without its line ending.
    Line(String),
    /// The `> ` prompt of AT+CMGS/AT+CMGW, which has no line ending (docs/research/FINDINGS.md 46).
    Prompt,
    /// The channel hung up or failed; the reader has ended.
    Closed,
}

/// Bytes to lines. CR and LF both end a line (the modem frames every line as `\r\n...\r\n`); empty lines vanish.
#[derive(Debug, Default)]
pub(crate) struct Splitter {
    pending: Vec<u8>,
}

impl Splitter {
    pub(crate) fn feed(&mut self, bytes: &[u8], out: &mut Vec<Event>) {
        for &b in bytes {
            if b == b'\r' || b == b'\n' {
                self.flush(out);
            } else {
                self.pending.push(b);
                if self.pending.len() >= LINE_MAX {
                    self.flush(out);
                }
            }
        }
        // the prompt is "\r\n> " and nothing after it until we send the body
        if self.pending.trim_ascii() == b">" {
            self.pending.clear();
            out.push(Event::Prompt);
        }
    }

    fn flush(&mut self, out: &mut Vec<Event>) {
        let line = String::from_utf8_lossy(self.pending.trim_ascii()).into_owned();
        self.pending.clear();
        if !line.is_empty() {
            out.push(Event::Line(line));
        }
    }
}

/// The reader loop. Runs until `stop`, until the channel task is gone, or until the channel closes.
pub(crate) fn run(file: Arc<File>, tx: mpsc::Sender<Event>, stop: CancellationToken, name: &'static str) {
    let mut split = Splitter::default();
    let mut buf = [0u8; 4096];
    let mut out = Vec::new();
    let mut errors = 0u32;
    loop {
        if stop.is_cancelled() || tx.is_closed() {
            return;
        }
        let ready = match sys::wait(&file, libc::POLLIN, POLL_SLICE) {
            Ok(Ready::No) => continue,
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(channel = name, error = %e, "the AT channel cannot be polled");
                let _ = tx.blocking_send(Event::Closed);
                return;
            }
        };
        match (&*file).read(&mut buf) {
            Ok(0) if ready == Ready::HangUp => {
                tracing::warn!(channel = name, "the AT channel hung up");
                let _ = tx.blocking_send(Event::Closed);
                return;
            }
            // readable and nothing there: a spurious wake-up, not an end of file
            Ok(0) => std::thread::sleep(Duration::from_millis(20)),
            Ok(n) => {
                errors = 0;
                split.feed(&buf[..n], &mut out);
                for ev in out.drain(..) {
                    if tx.blocking_send(ev).is_err() {
                        return;
                    }
                }
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
            Err(e) => {
                errors += 1;
                if errors >= ERRORS_MAX {
                    tracing::warn!(channel = name, error = %e, "the AT channel keeps failing to read");
                    let _ = tx.blocking_send(Event::Closed);
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&[u8]]) -> Vec<Event> {
        let mut s = Splitter::default();
        let mut out = Vec::new();
        for c in chunks {
            s.feed(c, &mut out);
        }
        out
    }

    fn line(s: &str) -> Event {
        Event::Line(s.into())
    }

    #[test]
    fn lines_across_reads() {
        assert_eq!(feed(&[b"\r\n+CSQ: 2", b"0,99\r\n\r\nOK\r\n"]), vec![line("+CSQ: 20,99"), line("OK")]);
        assert_eq!(feed(&[b"OK"]), vec![]);
        assert_eq!(feed(&[b"  \r\n\r\n"]), vec![]);
    }

    #[test]
    fn prompt_without_line_end() {
        assert_eq!(feed(&[b"\r\n> "]), vec![Event::Prompt]);
        assert_eq!(feed(&[b"\r\n>"]), vec![Event::Prompt]);
        assert_eq!(feed(&[b"\r\n> x\r\n"]), vec![line("> x")]);
    }

    #[test]
    fn long_lines_are_cut() {
        let long = vec![b'A'; LINE_MAX + 10];
        let out = feed(&[&long, b"\r\n"]);
        assert_eq!(out.len(), 2);
    }
}

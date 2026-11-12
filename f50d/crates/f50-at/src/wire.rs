//! One open channel: the descriptor, its reader thread and the lines it reads; the exchanges that put a command on
//! the wire and take its reply off it.
//!
//! A channel is opened once for the life of the process. Closing a descriptor while the modem runs leaves the
//! channel silent until a reboot, and only one reader of a channel gets its data (docs/research/FINDINGS.md 13b), so
//! the only close outside shutdown is the recovery in [`crate::channel`], under the rule 13b measured.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use f50_core::{Code, Error, Result};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::channel::{Liveness, Source};
use crate::reader::{self, Event};
use crate::{Final, Reply, Urc, codes, sys};

/// Lines waiting between the reader thread and the channel task.
const LINES_QUEUE: usize = 1024;
/// The wait for the PDU line after a +CMT/+CDS header (mu300-atd's `cmt_frame`).
const BODY_WAIT: Duration = Duration::from_secs(2);
/// The most a write may take.
const WRITE_MAX: Duration = Duration::from_secs(5);
/// The longest wait for the `> ` prompt of AT+CMGS/AT+CMGW.
const PROMPT_MAX: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct Wire {
    pub(crate) name: &'static str,
    file: Arc<File>,
    lines: mpsc::Receiver<Event>,
    stop: CancellationToken,
    reader: tokio::task::JoinHandle<()>,
    tracker: TaskTracker,
    live: Arc<Liveness>,
    source: Source,
    /// When the last line arrived (or the channel was opened).
    pub(crate) last_rx: Instant,
    /// The reader has ended (hang-up, failure): nothing will ever be read again from this descriptor.
    pub(crate) closed: bool,
}

/// What a drained line becomes.
#[derive(Debug)]
enum Stray {
    Urc(Urc),
    /// A late final code, an echo, or an information line nobody waits for any more.
    Dropped,
}

impl Wire {
    /// Opens `path` and starts its reader on the tracker's blocking pool.
    pub(crate) fn open(
        path: &Path,
        name: &'static str,
        tracker: &TaskTracker,
        shutdown: &CancellationToken,
        live: Arc<Liveness>,
        source: Source,
        fresh: bool,
    ) -> Result<Wire> {
        let file =
            Arc::new(crate::keeper::open(path, fresh).map_err(|e| Error::io(format!("open {}", path.display()), &e))?);
        let (tx, lines) = mpsc::channel(LINES_QUEUE);
        let stop = shutdown.child_token();
        let (f, s) = (file.clone(), stop.clone());
        let reader = tracker.spawn_blocking(move || reader::run(f, tx, s, name));
        let tracker = tracker.clone();
        Ok(Wire { name, file, lines, stop, reader, tracker, live, source, last_rx: Instant::now(), closed: false })
    }

    /// Stops the reader and closes the descriptor. Only for the recovery rule and shutdown.
    pub(crate) async fn close(self) {
        self.stop.cancel();
        let _ = self.reader.await;
        drop(self.file);
    }

    /// The next event; `None` once the reader has ended.
    pub(crate) async fn next(&mut self) -> Option<Event> {
        if self.closed {
            return std::future::pending().await;
        }
        match self.lines.recv().await {
            Some(Event::Closed) | None => {
                self.closed = true;
                None
            }
            Some(ev) => {
                self.last_rx = Instant::now();
                self.live.heard(self.source);
                Some(ev)
            }
        }
    }

    /// The next event before `deadline`; `Err` on the deadline, `Ok(None)` when the channel closed.
    async fn next_until(&mut self, deadline: Instant) -> std::result::Result<Option<Event>, ()> {
        if self.closed {
            tokio::time::sleep_until(deadline).await;
            return Err(());
        }
        timeout_at(deadline, self.next()).await.map_err(|_| ())
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<()> {
        let file = self.file.clone();
        let deadline = std::time::Instant::now() + WRITE_MAX;
        let r = self.tracker.spawn_blocking(move || sys::write_all(&file, &bytes, deadline)).await;
        match r {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::io("write to the AT channel", &e)),
            Err(e) => Err(Error::failed(format!("write to the AT channel: {e}"))),
        }
    }

    /// Takes a line that arrived while nobody asked: a URC goes to `urcs` (with its PDU line), the rest is dropped.
    pub(crate) async fn stray(&mut self, line: String, urcs: &broadcast::Sender<Urc>) {
        if let Stray::Urc(u) = self.classify_stray(line).await {
            let _ = urcs.send(u);
        }
    }

    async fn classify_stray(&mut self, line: String) -> Stray {
        if codes::is_echo(&line) || codes::final_code(&line).is_some() {
            // a late answer to a command that timed out, or an echo (which may carry a secret): never published
            tracing::debug!(channel = self.name, "a stray final code or echo on the AT channel, dropped");
            return Stray::Dropped;
        }
        // Late answers matter (+CUSD seconds after its OK, +CMTI): they are published like any URC, whatever their
        // name (docs/research/FINDINGS.md 26d).
        let body = if codes::takes_body(&line) { self.body().await } else { None };
        Stray::Urc(Urc { line, body })
    }

    /// The line after a +CMT/+CDS header, within [`BODY_WAIT`].
    pub(crate) async fn body(&mut self) -> Option<String> {
        let deadline = Instant::now() + BODY_WAIT;
        loop {
            match self.next_until(deadline).await {
                Ok(Some(Event::Line(l))) => return Some(l),
                Ok(Some(Event::Prompt | Event::Closed)) => {}
                Ok(None) | Err(()) => return None,
            }
        }
    }

    /// Throws away (publishing the URCs) what the modem sent while nobody was asking, so that a late line is never
    /// read as the next command's reply: until the channel has been quiet for `quiet`, at most `max`
    /// (mu300-atd's `drain`; docs/research/FINDINGS.md 15). Quiet is counted from the last line read, and the channel
    /// is read all the time, so after an idle spell this costs nothing.
    pub(crate) async fn drain(&mut self, quiet: Duration, max: Duration, urcs: &broadcast::Sender<Urc>) {
        let end = Instant::now() + max;
        loop {
            let deadline = (self.last_rx + quiet).min(end);
            if Instant::now() >= deadline {
                return;
            }
            match self.next_until(deadline).await {
                Ok(Some(Event::Line(l))) => self.stray(l, urcs).await,
                Ok(Some(Event::Prompt | Event::Closed)) => {}
                Ok(None) | Err(()) => return,
            }
        }
    }

    /// Writes `cmd` (with `body` after the prompt) and collects its reply until a final code or `timeout`.
    pub(crate) async fn exchange(
        &mut self,
        cmd: &str,
        body: Option<&str>,
        timeout: Duration,
        urcs: &broadcast::Sender<Urc>,
    ) -> Result<Reply> {
        if self.closed {
            return Err(Error::failed("the AT channel has closed"));
        }
        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        self.write(format!("{cmd}\r").into_bytes()).await?;
        if let Some(body) = body {
            let prompt_by = (Instant::now() + PROMPT_MAX).min(deadline);
            match self.collect(cmd, prompt_by, true, &mut lines, urcs).await {
                Collected::Prompt => {}
                Collected::Final(status) => return Ok(Reply { lines, status }),
                Collected::Closed => return Err(Error::failed("the AT channel has closed")),
                Collected::Timeout => {
                    // never leave the modem waiting at the prompt for a body that never comes: ESC aborts the
                    // submission (27.005 3.5.1); the channel counts as unclean, so the next command drains first
                    let _ = self.write(vec![0x1b]).await;
                    return Err(Error::new(Code::Timeout, "the modem did not ask for the message body"));
                }
            }
            let mut b = body.as_bytes().to_vec();
            b.push(0x1a);
            self.write(b).await?;
        }
        match self.collect(cmd, deadline, false, &mut lines, urcs).await {
            Collected::Final(status) => Ok(Reply { lines, status }),
            Collected::Closed => Err(Error::failed("the AT channel has closed")),
            Collected::Prompt | Collected::Timeout => Err(Error::timeout("the AT command")),
        }
    }

    /// Reads the reply of `cmd`: information lines into `lines`, URCs to `urcs`, until a final code (or the prompt,
    /// when `want_prompt`). A +CMT frame that arrives inside a reply is taken out of it (a client parsing a listing
    /// would read the PDU as one of its own lines; docs/research/FINDINGS.md 51).
    async fn collect(
        &mut self,
        cmd: &str,
        deadline: Instant,
        want_prompt: bool,
        lines: &mut Vec<String>,
        urcs: &broadcast::Sender<Urc>,
    ) -> Collected {
        loop {
            let line = match self.next_until(deadline).await {
                Err(()) => return Collected::Timeout,
                Ok(None) => return Collected::Closed,
                Ok(Some(Event::Prompt)) if want_prompt => return Collected::Prompt,
                Ok(Some(Event::Prompt | Event::Closed)) => continue,
                Ok(Some(Event::Line(l))) => l,
            };
            if codes::is_echo(&line) {
                continue;
            }
            if let Some(status) = codes::final_code(&line) {
                return Collected::Final(status);
            }
            if codes::takes_body(&line) || codes::is_urc_during(&line, cmd) {
                let body = if codes::takes_body(&line) { self.body().await } else { None };
                let _ = urcs.send(Urc { line, body });
                continue;
            }
            lines.push(line);
        }
    }
}

#[derive(Debug)]
enum Collected {
    Final(Final),
    Prompt,
    Timeout,
    Closed,
}

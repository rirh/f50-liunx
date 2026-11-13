//! The task that owns one command channel: it opens the device (waiting for the modem), serves the lanes' queues one
//! command at a time, reads the channel all the time in between, keeps the health of the channel, and recovers a
//! descriptor that a modem restart left dead.
//!
//! - **Lanes.** The control channel's task serves the control queue first and the poll queue only when no control
//!   command waits (`biased` select), so a poll never delays a control command by more than the one poll already on
//!   the wire. With a poll device of its own, polls go to that device's task and never touch the control channel.
//! - **Sessions.** A session request taken from the control queue turns the task over to its owner: only the
//!   session's commands are served (polls that share the device wait too) until its sender is dropped or it sits idle
//!   for [`SESSION_IDLE`].
//! - **Late replies.** Before every command the channel is drained until quiet (0.1 s after a clean reply, 1 s
//!   otherwise; for up to 25 s after a command that got no answer, 3 s otherwise), as f50-atd did, so a late `OK`
//!   is never read as the next command's reply (docs/research/FINDINGS.md 15, 13f).
//! - **Health.** A channel is up once it answers `AT`, and down after two unanswered commands in a row or when its
//!   reader ends; while down it is probed with `AT` every [`PROBE_EVERY`].
//! - **Recovery.** A descriptor open across a modem restart reads nothing for ever, and closing a good one while the
//!   modem runs kills the channel until a reboot (docs/research/FINDINGS.md 13b). So the descriptor is replaced only
//!   when five commands in a row went unanswered *and* no other channel has heard the modem for 240 s, at most once
//!   every five minutes, or when its reader has ended (nothing can ever be read from it again).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use f50_core::{Code, Error, Result};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::reader::Event;
use crate::request::{Command, Job, Queued};
use crate::wire::Wire;
use crate::{Reply, Urc};

/// The first open is retried this often while the modem starts its channels (f50-atd's 0.2 s nap) ...
const OPEN_RETRY: Duration = Duration::from_millis(200);
/// ... for this long (`F50_AT_OPEN_WAIT`, docs/research/FINDINGS.md 138), and every [`OPEN_RETRY_SLOW`] after.
const OPEN_WAIT: Duration = Duration::from_secs(240);
const OPEN_RETRY_SLOW: Duration = Duration::from_secs(2);
/// Unanswered commands in a row that make a channel down.
const DOWN_AFTER: u32 = 2;
/// How often a down channel is asked `AT`, and how long it has to answer.
const PROBE_EVERY: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// The reopen rule (docs/research/FINDINGS.md 13b): unanswered commands, silence elsewhere, spacing.
const REOPEN_SILENT: u32 = 5;
const REOPEN_QUIET: Duration = Duration::from_secs(240);
const REOPEN_SPACING: Duration = Duration::from_secs(300);
/// A session that sends nothing for this long is ended, so a leaked guard cannot keep the modem from everyone.
pub(crate) const SESSION_IDLE: Duration = Duration::from_secs(120);

/// Where the evidence that the modem is alive comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Urc = 0,
    Control = 1,
    Poll = 2,
}

/// When each channel last heard the modem: the evidence the reopen rule needs.
#[derive(Debug)]
pub(crate) struct Liveness {
    origin: Instant,
    /// Milliseconds since `origin` plus one; 0 = never.
    last: [AtomicU64; 3],
}

impl Liveness {
    pub(crate) fn new() -> Self {
        Liveness { origin: Instant::now(), last: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)] }
    }

    pub(crate) fn heard(&self, from: Source) {
        let ms = u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX - 1) + 1;
        self.last[from as usize].store(ms, Ordering::Relaxed);
    }

    /// Has any channel but `me` heard the modem within `within`?
    fn heard_elsewhere(&self, me: Source, within: Duration) -> bool {
        let now = u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX - 1) + 1;
        let window = u64::try_from(within.as_millis()).unwrap_or(u64::MAX);
        self.last
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != me as usize)
            .map(|(_, a)| a.load(Ordering::Relaxed))
            .any(|t| t != 0 && now.saturating_sub(t) < window)
    }
}

/// How a channel tells the rest of the crate whether it is up.
#[derive(Debug)]
pub(crate) enum Health {
    /// The control channel: `At::ready`.
    Watch(watch::Sender<bool>),
    /// The poll channel: polls go to the control channel's fallback queue while it is down.
    Flag(Arc<AtomicBool>),
}

impl Health {
    fn set(&self, up: bool) {
        match self {
            Health::Watch(tx) => {
                tx.send_if_modified(|v| std::mem::replace(v, up) != up);
            }
            Health::Flag(f) => f.store(up, Ordering::Relaxed),
        }
    }
}

/// One command channel's task.
#[derive(Debug)]
pub(crate) struct Channel {
    pub(crate) name: &'static str,
    pub(crate) source: Source,
    pub(crate) path: PathBuf,
    /// The lane served first.
    pub(crate) primary: mpsc::Receiver<Queued>,
    /// The poll lane, when it shares this channel (or while the poll channel is down).
    pub(crate) secondary: Option<mpsc::Receiver<Queued>>,
    pub(crate) health: Health,
    pub(crate) urcs: broadcast::Sender<Urc>,
    pub(crate) live: Arc<Liveness>,
    pub(crate) tracker: TaskTracker,
    pub(crate) shutdown: CancellationToken,
}

#[derive(Debug)]
struct State {
    wire: Option<Wire>,
    up: bool,
    /// The last reply ended in a final code.
    clean: bool,
    /// Commands in a row the modem did not answer.
    silent: u32,
    last_reopen: Option<Instant>,
}

async fn recv_opt(rx: Option<&mut mpsc::Receiver<Queued>>) -> Option<Queued> {
    match rx {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

fn not_open() -> Error {
    Error::busy("the modem's AT channel is not open yet")
}

impl Channel {
    pub(crate) async fn run(mut self) {
        let mut st = State { wire: None, up: false, clean: false, silent: 0, last_reopen: None };
        self.health.set(false);
        let mut probe = tokio::time::interval(PROBE_EVERY);
        probe.set_missed_tick_behavior(MissedTickBehavior::Delay);
        while !self.shutdown.is_cancelled() {
            if st.wire.is_none() {
                self.open_wait(&mut st).await;
                continue;
            }
            self.step(&mut st, &mut probe).await;
        }
        if let Some(w) = st.wire.take() {
            w.close().await;
        }
    }

    /// `fresh`: a real close and open of the device (the recovery rule), not the keeper's held descriptor.
    fn open(&self, fresh: bool) -> Result<Wire> {
        Wire::open(&self.path, self.name, &self.tracker, &self.shutdown, self.live.clone(), self.source, fresh)
    }

    /// Opens the device, retrying while the modem has not started it (ENODEV, or no node yet). A failed open was never
    /// seen to harm a channel; closing an open one is what loses it (docs/research/FINDINGS.md 138). Meanwhile every
    /// queued command is answered `busy` at once.
    async fn open_wait(&mut self, st: &mut State) {
        let start = Instant::now();
        let mut waiting = false;
        let mut gave_up = false;
        loop {
            match self.open(false) {
                Ok(mut w) => {
                    if waiting {
                        tracing::info!(channel = self.name, path = %self.path.display(), after_s = start.elapsed().as_secs(), "AT channel ready");
                    }
                    // the slow drain once, as soon as the channel is ours (docs/research/FINDINGS.md 40)
                    w.drain(Duration::from_secs(1), Duration::from_secs(3), &self.urcs).await;
                    st.wire = Some(w);
                    st.clean = true;
                    st.silent = 0;
                    return;
                }
                Err(e) if !waiting => {
                    tracing::info!(channel = self.name, path = %self.path.display(), error = %e, "the AT channel is not ready (the modem has not started its AT channels); waiting");
                    waiting = true;
                }
                Err(_) if !gave_up && start.elapsed() >= OPEN_WAIT => {
                    tracing::warn!(channel = self.name, path = %self.path.display(), "the AT channel did not become ready in 240 s: the modem has not started; still trying");
                    gave_up = true;
                }
                Err(_) => {}
            }
            let nap = if start.elapsed() < OPEN_WAIT { OPEN_RETRY } else { OPEN_RETRY_SLOW };
            let until = Instant::now() + nap;
            loop {
                tokio::select! {
                    biased;
                    () = self.shutdown.cancelled() => return,
                    () = tokio::time::sleep_until(until) => break,
                    Some(q) = self.primary.recv() => refuse(q),
                    Some(q) = recv_opt(self.secondary.as_mut()) => refuse(q),
                }
            }
        }
    }

    async fn step(&mut self, st: &mut State, probe: &mut tokio::time::Interval) {
        let Some(wire) = st.wire.as_mut() else { return };
        let up = st.up;
        tokio::select! {
            biased;
            () = self.shutdown.cancelled() => {}
            Some(q) = self.primary.recv() => self.handle(st, q).await,
            Some(q) = recv_opt(self.secondary.as_mut()) => self.handle(st, q).await,
            ev = wire.next() => self.idle_event(st, ev).await,
            _ = probe.tick(), if !up => self.probe(st).await,
        }
    }

    async fn idle_event(&mut self, st: &mut State, ev: Option<Event>) {
        match ev {
            Some(Event::Line(l)) => {
                if let Some(w) = st.wire.as_mut() {
                    w.stray(l, &self.urcs).await;
                }
            }
            Some(Event::Prompt | Event::Closed) => {}
            None => {
                tracing::warn!(channel = self.name, "the AT channel's reader has ended");
                self.set_up(st, false);
                if self.reopen_due(st) {
                    self.reopen(st).await;
                }
            }
        }
    }

    fn set_up(&self, st: &mut State, up: bool) {
        st.up = up;
        self.health.set(up);
    }

    async fn handle(&mut self, st: &mut State, q: Queued) {
        if !q.ticket.take() {
            return; // the caller gave up waiting: it was told busy, and nothing was sent
        }
        match q.job {
            Job::Command(c) => {
                if c.reply.is_closed() {
                    return;
                }
                let r = self.command(st, &c.cmd, c.body.as_deref(), c.timeout).await;
                let _ = c.reply.send(r);
            }
            Job::Session { owner, reply } => {
                let (tx, rx) = mpsc::channel(1);
                if reply.send(Ok(tx)).is_ok() {
                    self.session(st, owner, rx).await;
                }
            }
        }
    }

    /// The control channel to one owner until it lets go.
    async fn session(&mut self, st: &mut State, owner: &'static str, mut rx: mpsc::Receiver<Command>) {
        let mut last = Instant::now();
        loop {
            let Some(wire) = st.wire.as_mut() else { return };
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return,
                c = rx.recv() => {
                    let Some(c) = c else { return };
                    if !c.reply.is_closed() {
                        let r = self.command(st, &c.cmd, c.body.as_deref(), c.timeout).await;
                        let _ = c.reply.send(r);
                    }
                    last = Instant::now();
                }
                ev = wire.next() => self.idle_event(st, ev).await,
                () = tokio::time::sleep_until(last + SESSION_IDLE) => {
                    tracing::warn!(channel = self.name, owner, "an AT session sent nothing for 120 s; ended");
                    return;
                }
            }
        }
    }

    /// One command on the wire, with the drain before it and the bookkeeping after it.
    async fn command(&mut self, st: &mut State, cmd: &str, body: Option<&str>, timeout: Duration) -> Result<Reply> {
        let r = self.exchange(st, cmd, body, timeout).await;
        if r.is_ok() {
            st.silent = 0;
            self.set_up(st, true);
            return r;
        }
        st.silent += 1;
        if st.silent >= DOWN_AFTER {
            self.set_up(st, false);
        }
        if self.reopen_due(st) && self.reopen(st).await && body.is_none() {
            // the modem was gone, so the command never ran: ask once more on the new descriptor (as f50-atd did)
            let again = self.exchange(st, cmd, None, timeout).await;
            if again.is_ok() {
                st.silent = 0;
                self.set_up(st, true);
            }
            return again;
        }
        r
    }

    async fn exchange(&mut self, st: &mut State, cmd: &str, body: Option<&str>, timeout: Duration) -> Result<Reply> {
        let Some(wire) = st.wire.as_mut() else { return Err(not_open()) };
        let (quiet, max) = match (st.silent > 0, st.clean) {
            (true, _) => (Duration::from_secs(1), Duration::from_secs(25)),
            (false, true) => (Duration::from_millis(100), Duration::from_secs(3)),
            (false, false) => (Duration::from_secs(1), Duration::from_secs(3)),
        };
        wire.drain(quiet, max, &self.urcs).await;
        st.clean = false;
        let r = wire.exchange(cmd, body, timeout, &self.urcs).await;
        st.clean = r.is_ok();
        r
    }

    /// `AT` on a channel that is down; an answer makes it up. A probe never counts towards the reopen rule.
    async fn probe(&mut self, st: &mut State) {
        let silent = st.silent;
        if self.exchange(st, "AT", None, PROBE_TIMEOUT).await.is_ok() {
            st.silent = 0;
            self.set_up(st, true);
            tracing::info!(channel = self.name, "the modem answers on the AT channel");
        } else {
            st.silent = silent;
        }
    }

    fn reopen_due(&self, st: &State) -> bool {
        let Some(wire) = st.wire.as_ref() else { return false };
        let spaced = st.last_reopen.is_none_or(|t| t.elapsed() >= REOPEN_SPACING);
        let gone = st.silent >= REOPEN_SILENT && !self.live.heard_elsewhere(self.source, REOPEN_QUIET);
        spaced && (wire.closed || gone)
    }

    /// Replaces the descriptor; false when the device cannot be opened again (the task then waits for it).
    async fn reopen(&mut self, st: &mut State) -> bool {
        tracing::warn!(channel = self.name, path = %self.path.display(), "no reply and no sign of the modem elsewhere: the modem looks gone; reopening the AT channel");
        st.last_reopen = Some(Instant::now());
        self.set_up(st, false);
        if let Some(w) = st.wire.take() {
            w.close().await;
        }
        match self.open(true) {
            Ok(mut w) => {
                w.drain(Duration::from_secs(1), Duration::from_secs(3), &self.urcs).await;
                st.wire = Some(w);
                st.silent = 0;
                st.clean = true;
                true
            }
            Err(e) => {
                tracing::warn!(channel = self.name, error = %e, "cannot reopen the AT channel; waiting for it");
                false
            }
        }
    }
}

/// Answers a request the task cannot serve (the channel is not open): `busy`, nothing sent.
fn refuse(q: Queued) {
    if !q.ticket.take() {
        return;
    }
    match q.job {
        Job::Command(c) => {
            let _ = c.reply.send(Err(not_open()));
        }
        Job::Session { reply, .. } => {
            let _ = reply.send(Err(not_open()));
        }
    }
}

/// The error for a command whose session the task ended.
pub(crate) fn session_ended() -> Error {
    Error::new(Code::Busy, "the AT session has ended")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liveness_ignores_the_asking_channel() {
        let l = Liveness::new();
        assert!(!l.heard_elsewhere(Source::Control, REOPEN_QUIET));
        l.heard(Source::Control);
        assert!(!l.heard_elsewhere(Source::Control, REOPEN_QUIET));
        assert!(l.heard_elsewhere(Source::Poll, REOPEN_QUIET));
        l.heard(Source::Urc);
        assert!(l.heard_elsewhere(Source::Control, REOPEN_QUIET));
        assert!(!l.heard_elsewhere(Source::Control, Duration::ZERO));
    }
}

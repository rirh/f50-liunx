//! The receive hook: an optional program (`/etc/f50/sms-hook`, the shell pool's) run for every received message
//! the pool takes, with `SMS_FROM`, `SMS_DATE`, `SMS_TEXT` and `SMS_ID` in its environment (forwarding to a phone,
//! a webhook...). Its output and its exit status are ignored; it runs from the pool's copy, so it never depends on
//! the SIM still holding the message.
//!
//! One task runs the hooks one after another from a bounded queue: a burst of messages never starts a burst of
//! programs, and a full queue drops (and logs) rather than holding the receiver.

use std::path::PathBuf;
use std::time::Duration;

use f50_core::exec::{Cmd, executable};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::pool::Message;

/// Messages waiting for the hook.
const QUEUE: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct HookEvent {
    from: String,
    date: String,
    text: String,
    id: u64,
}

/// The hook's queue; its task runs on `tracker` until `shutdown`.
pub(crate) fn start(
    path: PathBuf,
    timeout: Duration,
    tracker: &TaskTracker,
    shutdown: CancellationToken,
) -> mpsc::Sender<HookEvent> {
    let (tx, rx) = mpsc::channel(QUEUE);
    tracker.spawn(run(path, timeout, rx, shutdown));
    tx
}

/// Hands newly pooled received messages to the hook.
pub(crate) fn offer(tx: &mpsc::Sender<HookEvent>, msgs: &[Message]) {
    for m in msgs {
        let ev = HookEvent { from: m.peer.clone(), date: m.time.clone(), text: m.text.clone(), id: m.id };
        if tx.try_send(ev).is_err() {
            tracing::warn!(id = m.id, "the SMS hook's queue is full; the hook does not run for this message");
        }
    }
}

async fn run(path: PathBuf, timeout: Duration, mut rx: mpsc::Receiver<HookEvent>, shutdown: CancellationToken) {
    loop {
        let ev = tokio::select! {
            () = shutdown.cancelled() => break,
            ev = rx.recv() => match ev {
                Some(ev) => ev,
                None => break,
            },
        };
        if !executable(&path) {
            continue;
        }
        let cmd = Cmd::new(&path)
            .env("SMS_FROM", &ev.from)
            .env("SMS_DATE", &ev.date)
            .env("SMS_TEXT", &ev.text)
            .env("SMS_ID", format!("{:06}", ev.id))
            .timeout(timeout)
            .max_output(4096);
        // dropped at shutdown: the program's whole process group goes with it
        tokio::select! {
            () = shutdown.cancelled() => break,
            r = cmd.run() => {
                if let Err(e) = r {
                    tracing::warn!(error = %e, "the SMS hook could not be started");
                }
            }
        }
    }
}

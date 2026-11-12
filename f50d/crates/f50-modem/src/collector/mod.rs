//! The signal and cell collector: AT rounds on the poll lane, published as an `Arc<RadioSnapshot>` on a watch
//! channel. Ported from `unisoc-modem/cell`.
//!
//! The one hard rule the V50 panel learned in the field: a synchronous AT round inside a web request starves the AT
//! channel, the dial watchdog then sees a dead modem and the box reboots itself (FINDINGS, V50 archive "45g"). So no
//! reader ever waits for the modem: methods serve the latest snapshot with its age. Two tiers share one loop: the
//! fast round (CFUN, CEREG, C5GREG, CESQ and the two SPENGMD serving reads) every `fast_interval`, the full round
//! (also the operator, the neighbours, the bearer's QoS, the identity and the own number) every `full_interval`.
//! A round has a deadline: a modem that stopped answering used to hold a collector for minutes. The loop pauses
//! while a radio sequence of ours runs (`restarting`): mid-restart the modem answers garbage (and the shell's
//! readers once crashed rpcd on such output).

pub mod engmd;
pub mod ident;
pub mod operator;
pub mod parse;
mod round;
mod slow;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use f50_at::{Lane, Reply, SendOpts};
use serde_json::{Value, json};

use crate::RadioSnapshot;
use crate::shared::Inner;

/// What the full tier read last; a fast round shows it beside its own readings.
#[derive(Debug, Clone)]
struct Slow {
    operator: Value,
    cops_act: Option<u64>,
    qos: Value,
    neigh: Value,
    ts: u64,
}

impl Default for Slow {
    fn default() -> Self {
        Slow { operator: Value::Null, cops_act: None, qos: Value::Null, neigh: Value::Null, ts: 0 }
    }
}

struct Collector {
    inner: Arc<Inner>,
    slow: Slow,
    last_full: Option<Instant>,
    /// The modem was not answering at the last look (published once as such).
    down: bool,
    /// The last time a first lock reading was started (one a minute while there is none).
    lock_try: Option<Instant>,
}

/// One round's commands, each given at most what is left of the round's deadline.
struct Round<'a> {
    inner: &'a Inner,
    deadline: Instant,
}

impl Round<'_> {
    async fn send(&self, cmd: &str, secs: u64) -> Option<Reply> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left < Duration::from_secs(1) {
            return None;
        }
        let timeout = Duration::from_secs(secs).min(left);
        let opts = SendOpts { lane: Lane::Poll, timeout, queue_wait: Duration::from_secs(2).min(left) };
        self.inner.at.send(cmd, opts).await.ok()
    }

    /// The first value of `prefix` in the answer.
    async fn value(&self, cmd: &str, prefix: &str, secs: u64) -> Option<String> {
        self.send(cmd, secs).await?.value(prefix).map(str::to_owned)
    }

    /// The first engineering line of the answer (a line with a "-" that is not the echo).
    async fn engmd(&self, cmd: &str) -> Option<String> {
        let r = self.send(cmd, 2).await?;
        r.lines.iter().find(|l| !l.starts_with("AT") && l.contains('-')).cloned()
    }
}

/// Starts the collector on the service's tracker.
pub(crate) fn spawn(inner: &Arc<Inner>) {
    let c = Collector { inner: inner.clone(), slow: Slow::default(), last_full: None, down: false, lock_try: None };
    inner.tracker.spawn(c.run());
}

impl Collector {
    async fn run(mut self) {
        let inner = self.inner.clone();
        let mut ready = inner.at.ready();
        let mut tick = tokio::time::interval(inner.cfg.fast_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = inner.shutdown.cancelled() => break,
                _ = tick.tick() => {}
            }
            if inner.is_restarting() {
                continue;
            }
            let is_ready = *ready.borrow_and_update();
            if !is_ready {
                self.publish_down("the modem is not answering (radio off, modem restarting, or no AT channel)");
                continue;
            }
            let full = self.last_full.is_none_or(|t| t.elapsed() >= inner.cfg.full_interval);
            let limit = if full { inner.cfg.full_deadline } else { inner.cfg.fast_deadline };
            tokio::select! {
                () = inner.shutdown.cancelled() => break,
                () = self.round(full, Instant::now() + limit) => {}
            }
            if full {
                self.last_full = Some(Instant::now());
            }
            if lock_cache_missing(&inner) && self.lock_try.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
                self.lock_try = Some(Instant::now());
                crate::lock::read::spawn_refresh(&inner);
            }
        }
    }

    fn publish(&self, snap: RadioSnapshot) {
        self.inner.snap_tx.send_replace(Arc::new(snap));
    }

    /// "No answer at all" is a modem or channel problem, not "no signal".
    fn publish_down(&mut self, why: &str) {
        if self.down {
            return;
        }
        self.down = true;
        self.publish(RadioSnapshot {
            taken: Some(Instant::now()),
            detail: json!({ "ts": f50_core::time::unix_now(), "error": why }),
            ..RadioSnapshot::default()
        });
    }

    /// The radio is on: the late lock replay need not wait for the early one any more (boot-replay), and the card
    /// is asked once how it is.
    fn saw_radio_on(&self) {
        if self.inner.early_pending.swap(false, Ordering::SeqCst) {
            self.inner.sim_check.notify_one();
        }
    }
}

fn lock_cache_missing(inner: &Inner) -> bool {
    crate::shared::lock(&inner.lock_cache).is_none() && !inner.lock_refreshing.load(Ordering::SeqCst)
}

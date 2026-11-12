//! The service's shared state: what the collector, the radio sequences, the lock code and the objects all reach.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use f50_at::{At, Final, Reply, SendOpts};
use f50_core::jobs::Jobs;
use f50_core::paths::Paths;
use f50_core::{Code, Result};
use f50_platform::Platform;
use serde_json::Value;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::collector::ident::IdentState;
use crate::collector::operator::OperatorCache;
use crate::lock::read::LockReading;
use crate::sim::SimState;
use crate::{ModemConfig, RadioSnapshot, Slot};

/// Locks a std mutex, taking the data of a poisoned one (a panic elsewhere must not stop the modem service).
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct Inner {
    pub cfg: ModemConfig,
    pub at: At,
    pub platform: Platform,
    pub jobs: Jobs,
    pub tracker: TaskTracker,
    pub shutdown: CancellationToken,
    /// The card this boot uses (pinned at start; FINDINGS 74: reopening the SIPC channels is not a hot switch).
    pub slot: Slot,
    pub snap_tx: watch::Sender<Arc<RadioSnapshot>>,
    /// The radio lock itself and the name of its holder (for the `busy` message).
    pub radio: Arc<tokio::sync::Mutex<()>>,
    pub radio_owner: Arc<Mutex<Option<&'static str>>>,
    /// Radio sequences of this crate in flight (a lock apply, a reset, a radio switch, a modem restart).
    pub restarting: AtomicUsize,
    /// The radio has not come on yet at this boot: the late lock replay waits for the early one (boot-replay).
    pub early_pending: AtomicBool,
    pub sim_state: Mutex<Option<SimState>>,
    pub sim_check: Notify,
    pub ident: Mutex<IdentState>,
    pub operator: Mutex<OperatorCache>,
    pub lock_cache: Mutex<Option<Arc<LockReading>>>,
    pub lock_refreshing: AtomicBool,
    /// The last lock reset's result (`lock get` -> `reset`).
    pub reset_result: Mutex<Option<Value>>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Modem").field("slot", &self.slot).finish_non_exhaustive()
    }
}

impl Inner {
    pub fn new(
        cfg: ModemConfig,
        at: At,
        platform: Platform,
        jobs: Jobs,
        tracker: TaskTracker,
        shutdown: CancellationToken,
        slot: Slot,
    ) -> Self {
        let (snap_tx, _) = watch::channel(Arc::new(RadioSnapshot::default()));
        Inner {
            cfg,
            at,
            platform,
            jobs,
            tracker,
            shutdown,
            slot,
            snap_tx,
            radio: Arc::new(tokio::sync::Mutex::new(())),
            radio_owner: Arc::new(Mutex::new(None)),
            restarting: AtomicUsize::new(0),
            early_pending: AtomicBool::new(true),
            sim_state: Mutex::new(None),
            sim_check: Notify::new(),
            ident: Mutex::new(IdentState::default()),
            operator: Mutex::new(OperatorCache::default()),
            lock_cache: Mutex::new(None),
            lock_refreshing: AtomicBool::new(false),
            reset_result: Mutex::new(None),
        }
    }

    pub fn paths(&self) -> &Paths {
        self.platform.paths()
    }

    /// One command on the control lane (user actions and radio sequences).
    pub async fn ctl(&self, cmd: &str, secs: u64) -> Result<Reply> {
        self.at.send(cmd, SendOpts::control(Duration::from_secs(secs))).await
    }

    /// One command on the poll lane (background readings).
    pub async fn poll(&self, cmd: &str, secs: u64) -> Result<Reply> {
        self.at.send(cmd, SendOpts::poll(Duration::from_secs(secs))).await
    }

    /// The first value of `prefix` in the answer to a read on the poll lane, blanks removed.
    pub async fn read_field(&self, cmd: &str, prefix: &str, secs: u64) -> Option<String> {
        let r = self.poll(cmd, secs).await.ok()?;
        r.value(prefix).map(|v| v.chars().filter(|c| !c.is_whitespace()).collect())
    }

    /// Marks a radio sequence of ours as running until the mark is dropped.
    pub fn restart_mark(self: &Arc<Self>) -> RestartMark {
        self.restarting.fetch_add(1, Ordering::SeqCst);
        RestartMark(self.clone())
    }

    pub fn is_restarting(&self) -> bool {
        self.restarting.load(Ordering::SeqCst) > 0
    }
}

/// While it lives, `Modem::restarting()` is true and the collector pauses.
pub(crate) struct RestartMark(Arc<Inner>);

impl Drop for RestartMark {
    fn drop(&mut self) {
        self.0.restarting.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How a write ended: refused by the modem, or taken (silence included: several vendor setters apply the value and
/// omit the final OK; the read-back decides, #356).
pub(crate) fn refused(r: &Result<Reply>) -> Option<String> {
    match r {
        Ok(reply) => match &reply.status {
            Final::Error => Some("ERROR".into()),
            Final::CmeError(n) => Some(format!("+CME ERROR: {n}")),
            Final::CmsError(n) => Some(format!("+CMS ERROR: {n}")),
            Final::Other(s) if s.contains("ERROR") => Some(s.clone()),
            _ => reply.lines.iter().find(|l| l.starts_with("ERROR") || l.contains("CME ERROR")).cloned(),
        },
        // a lane that stayed taken: nothing was sent, which is no write either
        Err(e) if e.code() == Code::Busy => Some(e.message().to_owned()),
        Err(_) => None,
    }
}

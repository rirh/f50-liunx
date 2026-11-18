//! Delay probes through the engine (`POST /v1/probe`), shared by every caller: a node's own test, a group test and
//! the supervisor's failover.
//!
//! A `url` probe makes the engine start a temporary Xray instance for that one request (`f50-proxy`'s
//! `internal/probe`), so a group of 40 members tested at once meant 40 instances on a 512 MiB router. Here:
//!
//! - at most [`PROBE_CONCURRENCY`] probes are with the engine at a time, whoever asked (the rest wait their turn);
//! - two identical probes in flight (the same node, chain, method and URL: the same request body) are one request,
//!   and every caller gets its answer.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_core::{Error, Result};
use serde_json::Value;
use tokio::sync::{OnceCell, OwnedSemaphorePermit, Semaphore};

use crate::client::Engine;
use crate::request::PROBE_TIMEOUT_MS;

/// Probes with the engine at once, across every caller. Small: each `url` probe is a temporary Xray instance.
pub(crate) const PROBE_CONCURRENCY: usize = 3;
/// An idle group leaves one slot available for a node test or failover.
pub(crate) const GROUP_PROBE_CONCURRENCY: usize = PROBE_CONCURRENCY - 1;

/// How long one probe may take with the engine (its own timeout and some slack); the wait for a turn is not in it.
const PROBE_WAIT: Duration = Duration::from_millis(PROBE_TIMEOUT_MS + 4000);
/// Bounds an interactive probe, including waiting for a turn or a shared result.
const INTERACTIVE_WAIT: Duration = Duration::from_secs(18);

#[derive(Debug, Default)]
struct Inflight {
    result: Arc<OnceCell<Result<Value>>>,
    callers: usize,
}

struct ProbeCall<'a> {
    probes: &'a Probes,
    key: String,
}

impl Drop for ProbeCall<'_> {
    fn drop(&mut self) {
        let mut table = self.probes.table();
        if let Some(entry) = table.get_mut(&self.key) {
            entry.callers -= 1;
            if entry.callers == 0 {
                table.remove(&self.key);
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Probes {
    slots: Semaphore,
    idle_group: Arc<Semaphore>,
    /// The probes under way by request body, removed when their last caller leaves.
    inflight: Mutex<HashMap<String, Inflight>>,
}

impl Default for Probes {
    fn default() -> Self {
        Probes {
            slots: Semaphore::new(PROBE_CONCURRENCY),
            idle_group: Arc::new(Semaphore::new(1)),
            inflight: Mutex::new(HashMap::new()),
        }
    }
}

impl Probes {
    pub fn begin_idle_group(&self) -> Result<OwnedSemaphorePermit> {
        self.idle_group.clone().try_acquire_owned().map_err(|_| Error::busy("another group test is running"))
    }

    /// The engine's answer to `body` (`{"ms": n}`), sharing a turn with every other probe and an identical probe
    /// already under way.
    pub async fn run(&self, engine: &Engine, body: &Value) -> Result<Value> {
        tokio::time::timeout(INTERACTIVE_WAIT, self.queued(engine, body))
            .await
            .map_err(|_| Error::busy("the proxy probe queue is busy"))?
    }

    /// The delay in milliseconds, `None` when the node did not answer.
    pub async fn ms(&self, engine: &Engine, body: &Value) -> Option<i64> {
        self.queued(engine, body).await.ok().and_then(|v| v["ms"].as_i64()).filter(|ms| *ms > 0)
    }

    async fn queued(&self, engine: &Engine, body: &Value) -> Result<Value> {
        self.run_with(body, || engine.post("/v1/probe", body, PROBE_WAIT)).await
    }

    async fn run_with<F, Fut>(&self, body: &Value, probe: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value>>,
    {
        let call = ProbeCall { probes: self, key: body.to_string() };
        let cell = {
            let mut table = self.table();
            let entry = table.entry(call.key.clone()).or_default();
            entry.callers += 1;
            entry.result.clone()
        };
        cell
            .get_or_init(|| async {
                let _turn = self.slots.acquire().await.map_err(|_| Error::busy("the proxy probe queue is closed"))?;
                probe().await
            })
            .await
            .clone()
    }

    fn table(&self) -> std::sync::MutexGuard<'_, HashMap<String, Inflight>> {
        self.inflight.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(start_paused = true)]
    async fn bounded_and_deduplicated() {
        let probes = Arc::new(Probes::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let now = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut set = tokio::task::JoinSet::new();
        // 10 distinct nodes, each asked for twice at once
        for i in 0..20 {
            let (probes, calls, now, peak) = (probes.clone(), calls.clone(), now.clone(), peak.clone());
            set.spawn(async move {
                let body = json!({ "link": format!("node-{}", i % 10) });
                probes
                    .run_with(&body, || async {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(n, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        now.fetch_sub(1, Ordering::SeqCst);
                        Ok(json!({ "ms": 10 + i % 10 }))
                    })
                    .await
            });
        }
        let mut answers = Vec::new();
        while let Some(r) = set.join_next().await {
            answers.push(r.expect("join").expect("probe")["ms"].as_i64().expect("ms"));
        }
        assert_eq!(answers.len(), 20);
        assert_eq!(calls.load(Ordering::SeqCst), 10, "identical probes in flight are one");
        assert_eq!(peak.load(Ordering::SeqCst), PROBE_CONCURRENCY, "never more than the cap at once");
        assert!(probes.table().is_empty(), "answered probes are forgotten");
    }

    #[tokio::test(start_paused = true)]
    async fn a_later_probe_asks_again() {
        let probes = Probes::default();
        let calls = AtomicUsize::new(0);
        let body = json!({ "link": "x" });
        for _ in 0..2 {
            probes
                .run_with(&body, || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({ "ms": 1 }))
                })
                .await
                .expect("probe");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

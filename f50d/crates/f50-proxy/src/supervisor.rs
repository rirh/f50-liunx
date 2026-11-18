//! The supervisor: every 5 s it reads the engine's status and brings the run back to what the profile says.
//!
//! - Desired on and the engine not running our request (the engine restarted, f50d restarted, the first boot):
//!   the start request again, then the firewall (`Inner::apply`); after a failed start not before
//!   [`RETRY_AFTER`](crate::service::RETRY_AFTER).
//! - Desired off and anything still up: the firewall removed, `/v1/stop`.
//! - The engine not answering: the interception is taken down at once (it would black-hole the LAN), and after a
//!   few ticks its procd service is started (it skips its start until f50d has written the token).
//! - Subscriptions older than `subscription_refresh_hours` are fetched again (a failed one not more than every
//!   30 minutes).
//! - Auto failover (one-node's `failover.rs`): the engine's health says the network is up and the selected node
//!   down (two missed checks in a row, the engine's own rule): the other nodes of the same subscription (the local
//!   nodes for a local one) are probed in parallel with the supervisor and the fastest that answers is selected.
//!   The next attempt waits 60 s after a switch and 120 s when none answered.

use std::collections::HashMap;
use std::time::Duration;

use f50_core::time::unix_now;
use f50_platform::services::Action;
use serde_json::Value;
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::Proxy;
use crate::client::is_unreachable;
use crate::nodes::{self, Candidate};
use crate::probes::PROBE_CONCURRENCY;
use crate::profile::Profile;
use crate::request;
use crate::service::{Phase, SHORT};

/// Ticks without an answer before the engine's service is started.
const UNREACHABLE_TICKS: u32 = 3;
/// And between two such starts.
const SERVICE_START_EVERY: Duration = Duration::from_secs(300);
const REFRESH_RETRY_SECS: u64 = 1800;
const FAILOVER_PROBE_WAIT: Duration = Duration::from_secs(25);
const FAILOVER_AFTER_SWITCH: Duration = Duration::from_secs(60);
const FAILOVER_AFTER_NONE: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct Watch {
    unreachable: u32,
    service_started: Option<Instant>,
    /// The last refresh attempt of each subscription (Unix seconds).
    refresh_tried: HashMap<String, u64>,
    failover_after: Instant,
    failover: JoinSet<Option<Instant>>,
}

pub(crate) async fn run(proxy: Proxy, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(proxy.inner.cfg.supervise_every);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut w = Watch {
        unreachable: 0,
        service_started: None,
        refresh_tried: HashMap::new(),
        failover_after: Instant::now(),
        failover: JoinSet::new(),
    };
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        let step = async {
            once(&proxy, &mut w).await;
            refresh_due(&proxy, &mut w).await;
        };
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = step => {}
        }
    }
}

async fn once(proxy: &Proxy, w: &mut Watch) {
    if let Some(Ok(Some(after))) = w.failover.try_join_next() {
        w.failover_after = after;
    }
    let inner = &proxy.inner;
    let status = inner.engine_status(SHORT).await;
    // a change under way holds ops: this tick leaves it alone
    let Ok(ops) = inner.ops.try_lock() else { return };
    let (enabled, phase, started, firewall, retry_at) = {
        let st = inner.lock();
        (st.profile.enabled, st.phase, st.started, st.firewall, st.retry_at)
    };
    let status = match status {
        Ok(s) => {
            w.unreachable = 0;
            s
        }
        Err(e) => {
            w.unreachable += 1;
            if firewall {
                if started {
                    tracing::warn!(error = %e, "the proxy engine stopped answering: interception removed");
                }
                inner.firewall_down().await;
            }
            {
                let mut st = inner.lock();
                st.started = false;
                if st.profile.enabled && st.phase == Phase::Running {
                    st.phase = Phase::Starting;
                }
            }
            drop(ops);
            if is_unreachable(&e) && w.unreachable >= UNREACHABLE_TICKS {
                start_service(proxy, w).await;
            }
            return;
        }
    };
    let running = status["running"] == Value::Bool(true);
    if enabled {
        if !running || !started || phase != Phase::Running {
            if retry_at.is_some_and(|t| Instant::now() < t) {
                return;
            }
            if running && !started {
                tracing::info!("the proxy engine runs a request of an earlier f50d: starting it again");
            } else if !running && started {
                tracing::warn!("the proxy engine is not running its connection: starting it again");
            }
            let _ = inner.apply(&ops).await;
            return;
        }
        drop(ops);
        if w.failover.is_empty() && Instant::now() >= w.failover_after && node_down(&status) {
            let proxy = proxy.clone();
            w.failover.spawn(async move { failover(&proxy, &status).await });
        }
    } else if running || firewall || phase != Phase::Off {
        let _ = inner.apply(&ops).await;
    }
}

async fn start_service(proxy: &Proxy, w: &mut Watch) {
    if w.service_started.is_some_and(|t| t.elapsed() < SERVICE_START_EVERY) {
        return;
    }
    w.service_started = Some(Instant::now());
    let services = proxy.inner.platform.services();
    let name = proxy.inner.cfg.service.as_str();
    if !services.installed(name) {
        tracing::debug!("the proxy engine's service is not installed");
        return;
    }
    tracing::info!("the proxy engine does not answer: starting its service");
    services.run_logged(name, Action::Start).await;
}

async fn refresh_due(proxy: &Proxy, w: &mut Watch) {
    let now = unix_now();
    let due: Vec<(String, String)> = {
        let st = proxy.inner.lock();
        let hours = u64::from(st.profile.settings.subscription_refresh_hours);
        if hours == 0 || now < 1_000_000_000 {
            // never, or the clock is not set yet (every subscription would look ancient)
            return;
        }
        st.profile
            .subscriptions
            .iter()
            .filter(|s| !s.url.is_empty())
            .filter(|s| now.saturating_sub(s.updated_at.unwrap_or(0)) >= hours * 3600)
            .filter(|s| now.saturating_sub(w.refresh_tried.get(&s.id).copied().unwrap_or(0)) >= REFRESH_RETRY_SECS)
            .map(|s| (s.id.clone(), s.url.clone()))
            .collect()
    };
    for (id, url) in due {
        w.refresh_tried.insert(id.clone(), now);
        match proxy.refresh_one(&id, &url).await {
            Ok(_) => tracing::info!(subscription = %id, "subscription refreshed"),
            Err(e) => tracing::warn!(subscription = %id, error = %e, "subscription refresh failed"),
        }
    }
}

/// The fastest reply within the recovery budget, including time spent waiting for a probe slot.
async fn fastest(proxy: &Proxy, url: &str, candidates: Vec<Candidate>) -> Option<(Candidate, i64)> {
    let mut candidates = candidates.into_iter();
    let mut set = JoinSet::new();
    let deadline = Instant::now() + FAILOVER_PROBE_WAIT;
    let mut best: Option<(Candidate, i64)> = None;
    loop {
        for c in candidates.by_ref().take(PROBE_CONCURRENCY - set.len()) {
            let inner = proxy.inner.clone();
            let body = request::probe_body(&c.link, &c.via, "url", url);
            set.spawn(async move {
                let ms = inner.probes.ms(&inner.engine, &body).await;
                (c, ms)
            });
        }
        let Ok(Some(r)) = tokio::time::timeout_at(deadline, set.join_next()).await else { break };
        let Ok((c, ms)) = r else { continue };
        {
            let mut st = proxy.inner.lock();
            if st.profile.settings.probe_url != url || !same_node(&st.profile, &c) {
                continue;
            }
            st.delays.insert(c.name.clone(), ms);
        }
        if let Some(ms) = ms
            && best.as_ref().is_none_or(|(_, b)| ms < *b)
        {
            best = Some((c, ms));
        }
    }
    best
}

fn node_down(status: &Value) -> bool {
    let health = &status["health"];
    status["running"] == Value::Bool(true)
        && health["online"] == Value::Bool(true)
        && health["node_ok"] == Value::Bool(false)
}

fn same_node(p: &Profile, candidate: &Candidate) -> bool {
    p.node(&candidate.id).filter(|n| n.usable()).and_then(|n| nodes::candidate(p, n)).as_ref() == Some(candidate)
}

async fn failover(proxy: &Proxy, status: &Value) -> Option<Instant> {
    let (current, others, url) = {
        let mut st = proxy.inner.lock();
        let p = &st.profile;
        if !p.enabled || !p.settings.auto_failover {
            return None;
        }
        let current = nodes::candidate(p, request::selected(p).ok()?)?;
        if status["node"].as_str() != Some(current.name.as_str()) {
            return None;
        }
        let others = nodes::candidates(p, &current.id, &st.delays);
        let url = p.settings.probe_url.clone();
        st.delays.insert(current.name.clone(), None);
        (current, others, url)
    };
    if others.is_empty() {
        return Some(Instant::now() + FAILOVER_AFTER_NONE);
    }
    tracing::warn!(node = %current.name, others = others.len(), "failover: the node does not answer; testing the others");
    let Some((pick, ms)) = fastest(proxy, &url, others).await else {
        tracing::warn!(node = %current.name, "failover: no other node answered in time; staying");
        return Some(Instant::now() + FAILOVER_AFTER_NONE);
    };

    let ops = proxy.inner.ops.lock().await;
    {
        let st = proxy.inner.lock();
        let p = &st.profile;
        if !p.enabled
            || !p.settings.auto_failover
            || p.settings.probe_url != url
            || p.selected.as_deref() != Some(current.id.as_str())
            || !same_node(p, &current)
            || !same_node(p, &pick)
        {
            return None;
        }
    }
    let latest = proxy.inner.engine_status(SHORT).await.ok()?;
    if !node_down(&latest) || latest["node"] != status["node"] || latest["since"] != status["since"] {
        return None;
    }
    let before = proxy.inner.fingerprint();
    let saved = proxy
        .inner
        .commit(&ops, |p| {
            p.selected = Some(pick.id.clone());
            Ok(())
        })
        .await;
    match saved {
        Ok(()) => {
            tracing::warn!(from = %current.name, to = %pick.name, ms, "failover: switched node");
            if let Err(e) = proxy.inner.reapply(&ops, &before).await {
                tracing::warn!(error = %e, "failover: the proxy did not restart on the new node");
            }
        }
        Err(e) => tracing::warn!(error = %e, "failover: the new selection was not saved"),
    }
    Some(Instant::now() + FAILOVER_AFTER_SWITCH)
}

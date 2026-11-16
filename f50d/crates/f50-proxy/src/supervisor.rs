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
//!   nodes for a local one) are probed at once and the fastest that answers is selected. The next attempt waits
//!   60 s after a switch and 120 s when none answered.

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
use crate::request::{self, PROBE_TIMEOUT_MS};
use crate::service::{Phase, SHORT};

/// Ticks without an answer before the engine's service is started.
const UNREACHABLE_TICKS: u32 = 3;
/// And between two such starts.
const SERVICE_START_EVERY: Duration = Duration::from_secs(300);
const REFRESH_RETRY_SECS: u64 = 1800;
const FAILOVER_AFTER_SWITCH: Duration = Duration::from_secs(60);
const FAILOVER_AFTER_NONE: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct Watch {
    unreachable: u32,
    service_started: Option<Instant>,
    /// The last refresh attempt of each subscription (Unix seconds).
    refresh_tried: HashMap<String, u64>,
    failover_after: Instant,
}

pub(crate) async fn run(proxy: Proxy, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(proxy.inner.cfg.supervise_every);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut w =
        Watch { unreachable: 0, service_started: None, refresh_tried: HashMap::new(), failover_after: Instant::now() };
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
        failover(proxy, w, &status).await;
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

/// The fastest of the candidates that answers, all probed at once.
async fn fastest(proxy: &Proxy, url: &str, candidates: Vec<Candidate>) -> Option<(Candidate, i64)> {
    let mut set = JoinSet::new();
    for c in candidates {
        let engine = proxy.inner.engine.clone();
        let body = request::probe_body(&c.link, &c.via, "url", url);
        set.spawn(async move {
            let r = engine.post("/v1/probe", &body, Duration::from_millis(PROBE_TIMEOUT_MS + 4000)).await;
            let ms = r.ok().and_then(|v| v["ms"].as_i64()).filter(|ms| *ms > 0);
            (c, ms)
        });
    }
    let mut best: Option<(Candidate, i64)> = None;
    while let Some(r) = set.join_next().await {
        let Ok((c, ms)) = r else { continue };
        proxy.inner.record_delay(&c.name, ms);
        if let Some(ms) = ms
            && best.as_ref().is_none_or(|(_, b)| ms < *b)
        {
            best = Some((c, ms));
        }
    }
    best
}

async fn failover(proxy: &Proxy, w: &mut Watch, status: &Value) {
    let health = &status["health"];
    let down = health["online"] == Value::Bool(true) && health["node_ok"] == Value::Bool(false);
    if !down || Instant::now() < w.failover_after {
        return;
    }
    let (current, others, url) = {
        let st = proxy.inner.lock();
        let p = &st.profile;
        if !p.settings.auto_failover {
            return;
        }
        let Some(cur) = p.selected.clone() else { return };
        let name = p.node(&cur).map(|n| n.name.clone()).unwrap_or_default();
        ((cur.clone(), name), nodes::candidates(p, &cur, &st.delays), p.settings.probe_url.clone())
    };
    proxy.inner.record_delay(&current.1, None);
    if others.is_empty() {
        w.failover_after = Instant::now() + FAILOVER_AFTER_NONE;
        return;
    }
    tracing::warn!(node = %current.1, others = others.len(), "failover: the node does not answer; testing the others");
    let Some((pick, ms)) = fastest(proxy, &url, others).await else {
        tracing::warn!(node = %current.1, "failover: no other node answers; staying");
        w.failover_after = Instant::now() + FAILOVER_AFTER_NONE;
        return;
    };
    w.failover_after = Instant::now() + FAILOVER_AFTER_SWITCH;
    let ops = proxy.inner.ops.lock().await;
    let before = proxy.inner.fingerprint();
    let r = proxy
        .inner
        .commit(&ops, |p| {
            // the user may have picked another node, or turned it off, meanwhile
            if p.enabled && p.selected.as_deref() == Some(current.0.as_str()) && p.node(&pick.id).is_some() {
                p.selected = Some(pick.id.clone());
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await;
    match r {
        Ok(true) => {
            tracing::warn!(from = %current.1, to = %pick.name, ms, "failover: switched node");
            if let Err(e) = proxy.inner.reapply(&ops, &before).await {
                tracing::warn!(error = %e, "failover: the proxy did not restart on the new node");
            }
        }
        Ok(false) => {}
        Err(e) => tracing::warn!(error = %e, "failover: the new selection was not saved"),
    }
}

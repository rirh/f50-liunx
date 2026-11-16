//! What the methods, the jobs and the supervisor share, and the two transitions: on (engine started, routes, nft)
//! and off (nft, routes, engine stopped).
//!
//! The profile and the runtime state sit behind one std mutex, never held across an await. Everything that
//! changes the profile or the engine's run holds `ops` (a tokio mutex) for the whole change, so two changes never
//! interleave their writes or their starts; engine calls that only read (status, connections, logs) do not take it.
//! A change is made on a copy, written, and only then becomes the profile: a failed write changes nothing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use f50_core::jobs::Jobs;
use f50_core::{Error, Result, fsx};
use f50_platform::Platform;
use serde_json::Value;
use tokio::time::Instant;

use crate::ProxyConfig;
use crate::client::Engine;
use crate::firewall::{self, Spec};
use crate::profile::Profile;
use crate::request::{self, Ports};

/// The engine's start: it may fetch rule sets before it answers.
pub(crate) const START_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const SHORT: Duration = Duration::from_secs(5);
/// After a failed start, the supervisor tries again this much later.
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Where the run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Off,
    Starting,
    Running,
    Error,
}

#[derive(Debug)]
pub(crate) struct State {
    pub profile: Profile,
    pub phase: Phase,
    /// Why it is not running, for people.
    pub error: Option<String>,
    /// Our start request is what the engine runs (false after the engine or f50d restarted).
    pub started: bool,
    /// The routes and the nft table are in place.
    pub firewall: bool,
    /// The last delay test of each node by name: `Some(ms)`, `None` when it failed.
    pub delays: HashMap<String, Option<i64>>,
    /// No automatic start before this (after a failed one).
    pub retry_at: Option<Instant>,
}

/// The token that says `ops` is held.
pub(crate) type Ops<'a> = tokio::sync::MutexGuard<'a, ()>;

#[derive(Debug)]
pub(crate) struct Inner {
    pub cfg: ProxyConfig,
    pub platform: Platform,
    pub jobs: Jobs,
    pub engine: Engine,
    /// `<state>/proxy`.
    pub dir: PathBuf,
    /// The applied nft file.
    pub nft_file: PathBuf,
    pub state: Mutex<State>,
    pub ops: tokio::sync::Mutex<()>,
}

impl Inner {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn profile(&self) -> Profile {
        self.lock().profile.clone()
    }

    pub fn ports(&self) -> Ports {
        Ports { mixed: self.cfg.mixed_port, tproxy: self.cfg.tproxy_port, mark: self.cfg.mark }
    }

    /// Changes the profile: `f` on a copy, the copy written (0600), then it is the profile.
    pub async fn commit<R>(&self, _ops: &Ops<'_>, f: impl FnOnce(&mut Profile) -> Result<R>) -> Result<R> {
        let mut next = self.profile();
        let r = f(&mut next)?;
        let data = next.to_stored()?;
        let path = self.dir.join("profile.json");
        tokio::task::spawn_blocking(move || fsx::write_atomic(&path, &data, 0o600))
            .await
            .map_err(|e| Error::failed(e.to_string()))??;
        self.lock().profile = next;
        Ok(r)
    }

    /// What the running proxy depends on: the start request and the firewall's settings. A change of it while
    /// enabled restarts the engine with the new request.
    pub fn fingerprint(&self) -> Value {
        let p = self.profile();
        serde_json::json!({
            "request": request::start(&p, self.ports()).ok(),
            "intercept_router": p.settings.intercept_router,
            "bypass_macs": p.settings.bypass_macs,
        })
    }

    /// Restarts the run when it is enabled and `before` (a [`fingerprint`](Self::fingerprint)) changed, or it is
    /// not running; stops it when it is disabled and still up.
    pub async fn reapply(&self, ops: &Ops<'_>, before: &Value) -> Result<()> {
        let (enabled, phase, firewall) = {
            let st = self.lock();
            (st.profile.enabled, st.phase, st.firewall)
        };
        if enabled && (phase != Phase::Running || &self.fingerprint() != before) {
            return self.apply(ops).await;
        }
        if !enabled && (phase != Phase::Off || firewall) {
            return self.apply(ops).await;
        }
        Ok(())
    }

    /// Brings the run to what the profile says: started (again, with the current request) or stopped.
    pub async fn apply(&self, _ops: &Ops<'_>) -> Result<()> {
        let p = self.profile();
        if !p.enabled {
            self.stop_all().await;
            let mut st = self.lock();
            st.phase = Phase::Off;
            st.error = None;
            st.started = false;
            return Ok(());
        }
        self.lock().phase = Phase::Starting;
        let r = self.start_all(&p).await;
        let mut st = self.lock();
        match &r {
            Ok(()) => {
                st.phase = Phase::Running;
                st.error = None;
                st.started = true;
                st.retry_at = None;
            }
            Err(e) => {
                tracing::warn!(error = %e, "the proxy did not start");
                st.phase = Phase::Error;
                st.error = Some(e.message().to_owned());
                st.started = false;
                st.retry_at = Some(Instant::now() + RETRY_AFTER);
            }
        }
        r
    }

    /// Engine start → routes and rules → nft; a failed step undoes the ones before it.
    async fn start_all(&self, p: &Profile) -> Result<()> {
        let req = match request::start(p, self.ports()) {
            Ok(r) => r,
            Err(e) => {
                self.stop_all().await;
                return Err(e);
            }
        };
        if let Err(e) = self.engine.post("/v1/start", &req, START_TIMEOUT).await {
            // the engine's old instance may be gone with the failed start: nothing may stay intercepted
            self.stop_all().await;
            return Err(e.context("the proxy engine"));
        }
        if let Err(e) = firewall::routes_up().await {
            firewall::routes_down().await;
            let _ = self.engine.post("/v1/stop", &Value::Null, SHORT).await;
            return Err(e.context("policy routing"));
        }
        let spec = self.spec(p).await;
        if let Err(e) = firewall::nft(&self.nft_file, &firewall::ruleset(&spec)).await {
            firewall::routes_down().await;
            let _ = self.engine.post("/v1/stop", &Value::Null, SHORT).await;
            self.lock().firewall = false;
            return Err(e);
        }
        self.lock().firewall = true;
        tracing::info!(node = req["name"].as_str().unwrap_or(""), routing = p.routing.as_str(), "proxy running");
        Ok(())
    }

    async fn spec(&self, p: &Profile) -> Spec {
        Spec {
            lan_if: firewall::lan_interfaces(&self.platform, &self.cfg.lan_interface, &self.cfg.lan_bridge).await,
            bypass_macs: p.settings.bypass_macs.clone(),
            intercept_router: p.settings.intercept_router,
            tproxy_port: self.cfg.tproxy_port,
            mark: self.cfg.mark,
        }
    }

    /// The interception taken down (nft, then the routes); the engine keeps running.
    pub async fn firewall_down(&self) {
        if let Err(e) = firewall::nft(&self.nft_file, &firewall::removal()).await {
            tracing::warn!(error = %e, "the proxy's nft table was not removed");
        }
        firewall::routes_down().await;
        self.lock().firewall = false;
    }

    /// nft → rules and routes → the engine stopped. Every step runs; failures are logged.
    pub async fn stop_all(&self) {
        self.firewall_down().await;
        match self.engine.post("/v1/stop", &Value::Null, SHORT).await {
            Ok(_) => {}
            Err(e) if crate::client::is_unreachable(&e) => {}
            Err(e) => tracing::warn!(error = %e, "the proxy engine did not stop"),
        }
    }

    /// The engine's status now, `None` when it does not answer.
    pub async fn engine_status(&self, timeout: Duration) -> Result<Value> {
        self.engine.get("/v1/status", timeout).await
    }

    /// Whether the engine runs a connection now.
    pub async fn engine_running(&self) -> bool {
        self.engine_status(SHORT).await.is_ok_and(|s| s["running"] == Value::Bool(true))
    }

    /// The active config checked again (the nodes changed: its groups' members, its warnings); on failure the old
    /// check stays.
    pub async fn recheck(&self, ops: &Ops<'_>) {
        let (id, text, body) = {
            let st = self.lock();
            let c = st.profile.active();
            (c.id.clone(), c.text.clone(), request::rules_check(&st.profile, &c.text))
        };
        let Ok(info) = self.engine.post("/v1/rules/check", &body, Duration::from_secs(20)).await else { return };
        let r = self
            .commit(ops, |p| {
                if let Some(c) = p.configs.iter_mut().find(|c| c.id == id && c.text == text) {
                    c.info = Some(info);
                }
                Ok(())
            })
            .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, "the proxy profile was not saved");
        }
    }

    pub fn record_delay(&self, name: &str, ms: Option<i64>) {
        self.lock().delays.insert(name.to_owned(), ms);
    }

    /// The engine's log level for its next start (`<state>/proxy/log_level`, read by its init script).
    pub fn write_log_level(&self, level: &str) -> Result<()> {
        fsx::write_atomic(&self.dir.join("log_level"), format!("{level}\n").as_bytes(), 0o600)
    }
}

/// Unix seconds of an RFC 3339 time (`2026-11-16T10:00:00.123+08:00`, `…Z`), as Go encodes `time.Time`; `None` for
/// Go's zero time and anything before 1970.
pub(crate) fn rfc3339_secs(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        rest = &frac[n..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        r if r.len() == 6 && (r.starts_with('+') || r.starts_with('-')) => {
            let sign = if r.starts_with('-') { -1 } else { 1 };
            sign * (r.get(1..3)?.parse::<i64>().ok()? * 3600 + r.get(4..6)?.parse::<i64>().ok()? * 60)
        }
        _ => return None,
    };
    // days from the civil date (Howard Hinnant's algorithm)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let t = days * 86_400 + h * 3600 + mi * 60 + se - offset;
    u64::try_from(t).ok().filter(|t| *t > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_times() {
        assert_eq!(rfc3339_secs("2025-11-16T08:00:00Z"), Some(1_763_280_000));
        assert_eq!(rfc3339_secs("2025-11-16T16:00:00.123456789+08:00"), Some(1_763_280_000));
        assert_eq!(rfc3339_secs("2025-11-16T03:00:00-05:00"), Some(1_763_280_000));
        assert_eq!(rfc3339_secs("1970-01-01T00:00:00Z"), None);
        assert_eq!(rfc3339_secs("0001-01-01T00:00:00Z"), None, "Go's zero time");
        assert_eq!(rfc3339_secs("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(rfc3339_secs("yesterday"), None);
        assert_eq!(rfc3339_secs("2025-11-16T08:00:00"), None);
    }
}

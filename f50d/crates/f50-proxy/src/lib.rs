//! Proxy and split routing (代理与分流) on the F50 (`f50.proxy`), against the contract `f50-proxy/DESIGN.md` §2.
//!
//! The engine (`/opt/f50/bin/f50-proxy`, Xray-core with one-node's rules, its own procd service) does the
//! proxying; this crate takes the role one-node's Tauri shell (`pc/src-tauri`) has on a PC:
//!
//! - `client`: the engine's loopback API (HTTP/1.1 over a `TcpStream`, Bearer token) and the token file;
//! - `profile`: subscriptions, nodes, configs, group picks and settings on flash (`<state>/proxy/profile.json`);
//! - `nodes`: unique names, front-node chains, a subscription update carried over by name, failover candidates;
//! - `request`: the engine's start request (mode `gateway`) and the other request bodies;
//! - `firewall`: the nft table `inet f50_proxy` (tproxy) and the policy routing, only while running;
//! - `service`: the shared state and the on/off transitions; `status`: the `status` reply;
//! - `ops`, `subs`, `groups`: the methods; `supervisor`: the 5 s loop (restart, refresh, failover);
//! - `api`: the `f50.proxy` object.
//!
//! Not run on a device yet: the engine's `gateway` mode, the nft ruleset as the F50's kernel takes it, and the
//! interplay with fw4 and dnsmasq are unverified without the hardware.

mod api;
mod client;
mod firewall;
mod groups;
mod nodes;
mod ops;
mod profile;
mod request;
mod service;
mod status;
mod subs;
mod supervisor;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use f50_core::jobs::Jobs;
use f50_core::rpc::Object;
use f50_core::{Result, fsx};
use f50_platform::Platform;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::client::Engine;
use crate::service::{Inner, Phase, SHORT, State};

/// How the proxy runs. [`Default`] is the contract's.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// The state directory; `None`: `<state>/proxy` (`/etc/f50d/proxy`).
    pub dir: Option<PathBuf>,
    /// The engine's loopback API.
    pub api: SocketAddr,
    /// The engine's mixed (SOCKS5/HTTP) and tproxy ports, and the mark of its own sockets.
    pub mixed_port: u16,
    pub tproxy_port: u16,
    pub mark: u32,
    pub supervise_every: Duration,
    /// The LAN's netifd interface and the bridge assumed when netifd does not say.
    pub lan_interface: String,
    pub lan_bridge: String,
    /// The engine's procd service.
    pub service: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        ProxyConfig {
            dir: None,
            api: SocketAddr::from(([127, 0, 0, 1], 17891)),
            mixed_port: 17890,
            tproxy_port: 17892,
            mark: 0xff,
            supervise_every: Duration::from_secs(5),
            lan_interface: "lan".into(),
            lan_bridge: "br-lan".into(),
            service: "f50-proxy".into(),
        }
    }
}

/// The proxy. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Proxy {
    inner: Arc<Inner>,
}

impl Proxy {
    /// Reads the profile, makes the engine's token when there is none, and starts the supervisor on `tracker`
    /// (it stops at `shutdown`). Nothing is intercepted until the supervisor has started the engine.
    pub fn start(
        cfg: ProxyConfig,
        platform: Platform,
        jobs: Jobs,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Proxy> {
        let paths = platform.paths().clone();
        let dir = cfg.dir.clone().unwrap_or_else(|| paths.state().join("proxy"));
        fsx::private_dir(&dir)?;
        let (token, made) = client::ensure_token(&dir.join("token"))?;
        if made {
            tracing::info!("the proxy engine's token was made");
        }
        let profile = profile::load(&dir.join("profile.json"));
        if fsx::read_opt(&dir.join("log_level")).ok().flatten().is_none() {
            let _ = fsx::write_atomic(
                &dir.join("log_level"),
                format!("{}\n", profile.settings.log_level).as_bytes(),
                0o600,
            );
        }
        let phase = if profile.enabled { Phase::Starting } else { Phase::Off };
        let inner = Arc::new(Inner {
            engine: Engine::new(cfg.api, token),
            nft_file: paths.run().join("proxy.nft"),
            cfg,
            platform,
            jobs,
            dir,
            state: Mutex::new(State {
                profile,
                phase,
                error: None,
                started: false,
                // a table an earlier f50d left (it was killed) is removed by the first stop or replaced by the start
                firewall: true,
                delays: HashMap::new(),
                retry_at: None,
            }),
            ops: tokio::sync::Mutex::new(()),
        });
        let proxy = Proxy { inner };
        tracker.spawn(supervisor::run(proxy.clone(), shutdown));
        Ok(proxy)
    }

    /// The `f50.proxy` object.
    pub fn objects(&self) -> Vec<Object> {
        vec![api::object(self)]
    }

    /// Takes the interception down (nft table, policy routing) for f50d's exit; the engine keeps running and the
    /// next f50d starts it again.
    pub async fn shutdown(&self) {
        // after a change under way (it may be putting the table up); bounded, the table goes regardless
        let ops = tokio::time::timeout(Duration::from_secs(10), self.inner.ops.lock()).await.ok();
        if self.inner.lock().firewall {
            self.inner.firewall_down().await;
        }
        drop(ops);
    }

    /// The engine's mixed (SOCKS5/HTTP) inbound on loopback while the proxy runs, for f50d's own requests that
    /// should go through it; `None` while it is off, starting or failed.
    pub fn local_proxy(&self) -> Option<SocketAddr> {
        (self.inner.lock().phase == Phase::Running)
            .then(|| SocketAddr::from(([127, 0, 0, 1], self.inner.cfg.mixed_port)))
    }

    pub(crate) fn profile_view(&self) -> Value {
        json!(self.inner.lock().profile)
    }

    pub(crate) async fn status(&self) -> Value {
        let engine = self.inner.engine_status(SHORT.min(Duration::from_secs(2))).await.ok();
        let snap = {
            let st = self.inner.lock();
            status::Snapshot {
                profile: st.profile.clone(),
                phase: st.phase,
                error: st.error.clone(),
                delays: st.delays.clone(),
            }
        };
        status::view(&snap, engine.as_ref())
    }
}

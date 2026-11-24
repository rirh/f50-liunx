//! 本地 SOCKS5 (one-node's per-node local ports, `pc/src-tauri/src/state/local_socks.rs`): a node may have its own
//! SOCKS5 port; what reaches it leaves through that node (with its front nodes), whatever the proxy is doing.
//!
//! - The engine runs the ports (`/v1/local-socks`, the whole set every time; unchanged ports keep running). On the
//!   router they listen on every address (`serve --local-socks-listen 0.0.0.0`): fw4 rejects input from the WAN zone,
//!   so the LAN and the USB host reach them, the cellular side does not. Their sockets carry the engine's mark, so
//!   the interception of the router's own traffic never catches them.
//! - The supervisor sends the set again when it changed or the engine's does not match it (the engine restarted).
//! - A port is unique across the nodes and is none of the engine's own; with none given, the first free one from
//!   1080 is taken.

use serde_json::{Value, json};

use f50_core::{Code, Error, Result};

use crate::Proxy;
use crate::nodes;
use crate::profile::LocalSocks;
use crate::service::{Inner, SHORT};

/// Where an automatic port starts, and how far it looks.
const AUTO_FROM: u16 = 1080;
const AUTO_SPAN: u16 = 100;

/// `(id, port)` of each entry, sorted: what the engine's answer is compared by.
fn keys(ports: &Value) -> Vec<(String, u64)> {
    let mut k: Vec<(String, u64)> = ports
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| (e["id"].as_str().unwrap_or_default().to_owned(), e["port"].as_u64().unwrap_or(0)))
        .collect();
    k.sort();
    k
}

impl Inner {
    /// Brings the engine's ports to the profile's: sent when they differ from what was last sent, when the
    /// engine's set does not match (it restarted), or `force`. Answers the engine's `{ports, listen}`; `Null` when
    /// there is nothing to run and nothing was asked.
    pub async fn sync_socks(&self, force: bool) -> Result<Value> {
        let mut sent = self.socks_sent.lock().await;
        let want = Value::Array(nodes::socks_entries(&self.lock().profile));
        if !force && sent.as_ref() == Some(&want) && keys(&want).is_empty() {
            return Ok(Value::Null);
        }
        if !force && sent.as_ref() == Some(&want) {
            let now = self.engine.get("/v1/local-socks", SHORT).await?;
            if keys(&now["ports"]) == keys(&want) {
                return Ok(now);
            }
            tracing::info!("the proxy engine lost its local SOCKS5 ports: sending them again");
        }
        let r = self.engine.post("/v1/local-socks", &json!({ "ports": want }), SHORT).await?;
        *sent = Some(want);
        Ok(r)
    }

    /// Whether the engine can bind `port` now.
    async fn port_free(&self, port: u16) -> Result<bool> {
        let r = self.engine.post("/v1/local-socks/free-port", &json!({ "port": port }), SHORT).await?;
        Ok(r["free"] == Value::Bool(true))
    }

    fn reserved(&self, port: u16) -> bool {
        [self.cfg.mixed_port, self.cfg.tproxy_port, self.cfg.api.port()].contains(&port)
    }
}

impl Proxy {
    /// How the ports run: the engine's `{ports: [{id, port, running, error}], listen}`.
    pub(crate) async fn local_socks(&self) -> Result<Value> {
        let r = self.inner.sync_socks(false).await?;
        Ok(if r.is_null() { json!({ "ports": [] }) } else { r })
    }

    /// Turns node `id`'s port on (`port`: that one; none: its current one, else the first free from 1080) or off.
    /// Answers `{node, local_socks}`.
    pub(crate) async fn local_socks_set(&self, id: &str, enabled: bool, port: Option<u16>) -> Result<Value> {
        let ops = self.inner.ops.lock().await;
        let (current, taken) = {
            let st = self.inner.lock();
            let node = st.profile.node(id).ok_or_else(|| Error::not_found("node"))?;
            if enabled && let Some(problem) = &node.problem {
                return Err(Error::failed(format!("the node \"{}\" cannot be used: {problem}", node.name)));
            }
            let taken: Vec<(u16, String)> = st
                .profile
                .nodes()
                .filter(|n| n.id != id)
                .filter_map(|n| Some((n.local_socks?.port, n.name.clone())))
                .collect();
            (node.local_socks.map(|s| s.port), taken)
        };
        let next = if !enabled {
            None
        } else if let Some(port) = port {
            if let Some((_, name)) = taken.iter().find(|(p, _)| *p == port) {
                return Err(Error::field(Code::Failed, "port", format!("port {port} is the node \"{name}\"'s")));
            }
            if self.inner.reserved(port) {
                return Err(Error::field(Code::Failed, "port", format!("port {port} is the proxy engine's own")));
            }
            if Some(port) != current && !self.inner.port_free(port).await? {
                return Err(Error::field(Code::Failed, "port", format!("port {port} is in use")));
            }
            Some(port)
        } else if let Some(port) = current {
            Some(port)
        } else {
            let mut found = None;
            for port in AUTO_FROM..AUTO_FROM + AUTO_SPAN {
                if taken.iter().any(|(p, _)| *p == port) || self.inner.reserved(port) {
                    continue;
                }
                if self.inner.port_free(port).await? {
                    found = Some(port);
                    break;
                }
            }
            Some(found.ok_or_else(|| Error::failed("no free port from 1080 to 1179"))?)
        };
        if next != current {
            self.inner
                .commit(&ops, |p| {
                    let node = p.nodes_mut().find(|n| n.id == id).ok_or_else(|| Error::not_found("node"))?;
                    node.local_socks = next.map(|port| LocalSocks { port });
                    Ok(())
                })
                .await?;
        }
        drop(ops);
        // saved either way: the supervisor sends it again once the engine answers
        let ports = match self.inner.sync_socks(true).await {
            Ok(r) => r,
            Err(e) => json!({ "ports": [], "error": e.message() }),
        };
        let node = self.inner.profile().node(id).map(|n| json!(n)).ok_or_else(|| Error::not_found("node"))?;
        Ok(json!({ "node": node, "local_socks": ports }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_order_and_status() {
        let a = json!([{"id": "L1", "port": 1081, "running": true}, {"id": "s1-2", "port": 1080}]);
        let b = json!([{"id": "s1-2", "port": 1080, "link": "x"}, {"id": "L1", "port": 1081}]);
        assert_eq!(keys(&a), keys(&b));
        assert_ne!(keys(&a), keys(&json!([{"id": "L1", "port": 1081}])));
        assert!(keys(&Value::Null).is_empty());
    }
}

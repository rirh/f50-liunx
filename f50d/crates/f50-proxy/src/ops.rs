//! The methods that change the profile (the run, the settings, the nodes, the configs) and the engine's
//! read-throughs (probe, rules, connections, logs).
//!
//! Each change holds `ops`, is committed, then re-applied when the start request or the firewall's settings changed
//! while the proxy is on (`Inner::reapply`).

use std::time::Duration;

use f50_core::{Code, Error, Result};
use f50_platform::services::Action;
use serde_json::{Value, json};

use crate::Proxy;
use crate::firewall::mac_ok;
use crate::nodes;
use crate::profile::{BUILTIN_ID, Config, Node, Routing, next_id};
use crate::request;
use crate::service::SHORT;

const INSPECT_TIMEOUT: Duration = Duration::from_secs(15);
const RULES_TIMEOUT: Duration = Duration::from_secs(20);
const FETCH_TIMEOUT: Duration = Duration::from_secs(35);

/// `set`'s fields.
#[derive(Debug, Default)]
pub(crate) struct SetChange {
    pub enabled: Option<bool>,
    pub routing: Option<String>,
    pub selected: Option<String>,
    pub active_config: Option<String>,
}

/// `settings_set`'s fields.
#[derive(Debug, Default)]
pub(crate) struct SettingsChange {
    pub intercept_router: Option<bool>,
    pub bypass_macs: Option<String>,
    pub probe_url: Option<String>,
    pub subscription_refresh_hours: Option<u32>,
    pub rule_sets_url: Option<String>,
    pub auto_failover: Option<bool>,
    pub log_level: Option<String>,
}

/// A comma (or blank) separated list of MAC addresses, lower case, each once.
pub(crate) fn parse_macs(s: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for m in s.split([',', ' ', ';']).map(str::trim).filter(|m| !m.is_empty()) {
        let m = m.to_ascii_lowercase().replace('-', ":");
        if !mac_ok(&m) {
            return Err(Error::bad_value("bypass_macs"));
        }
        if !out.contains(&m) {
            out.push(m);
        }
    }
    Ok(out)
}

fn clean_name(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

impl Proxy {
    pub(crate) async fn set(&self, c: SetChange) -> Result<Value> {
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        let was_enabled = self.inner.lock().profile.enabled;
        self.inner
            .commit(&ops, |p| {
                if let Some(id) = &c.selected {
                    let n = p.node(id).ok_or_else(|| Error::not_found("node"))?;
                    if let Some(problem) = &n.problem {
                        return Err(Error::failed(format!("the node \"{}\" cannot be used: {problem}", n.name)));
                    }
                    p.selected = Some(id.clone());
                }
                if let Some(r) = c.routing.as_deref() {
                    p.routing = Routing::parse(r).ok_or_else(|| Error::bad_value("routing"))?;
                }
                if let Some(id) = &c.active_config {
                    p.config(id).ok_or_else(|| Error::not_found("config"))?;
                    p.active_config = id.clone();
                }
                if let Some(on) = c.enabled {
                    p.enabled = on;
                }
                Ok(())
            })
            .await?;
        let enabled = self.inner.lock().profile.enabled;
        if enabled != was_enabled {
            tracing::info!(enabled, "proxy switched");
            self.inner.apply(&ops).await?;
        } else {
            self.inner.reapply(&ops, &before).await?;
        }
        drop(ops);
        Ok(self.status().await)
    }

    pub(crate) async fn settings_set(&self, c: SettingsChange) -> Result<Value> {
        let macs = c.bypass_macs.as_deref().map(parse_macs).transpose()?;
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        let old_level = self.inner.lock().profile.settings.log_level.clone();
        let settings = self
            .inner
            .commit(&ops, |p| {
                let s = &mut p.settings;
                if let Some(v) = c.intercept_router {
                    s.intercept_router = v;
                }
                if let Some(v) = macs {
                    s.bypass_macs = v;
                }
                if let Some(v) = c.probe_url {
                    s.probe_url = v;
                }
                if let Some(v) = c.subscription_refresh_hours {
                    s.subscription_refresh_hours = v;
                }
                if let Some(v) = c.rule_sets_url {
                    s.rule_sets_url = v;
                }
                if let Some(v) = c.auto_failover {
                    s.auto_failover = v;
                }
                if let Some(v) = c.log_level {
                    s.log_level = v;
                }
                Ok(s.clone())
            })
            .await?;
        if settings.log_level != old_level {
            // the engine takes its level on the command line: its service starts again, idle, and the
            // supervisor sends the start request once it answers
            self.inner.write_log_level(&settings.log_level)?;
            self.inner.platform.services().run_logged(&self.inner.cfg.service, Action::Restart).await;
            self.inner.lock().started = false;
        }
        self.inner.reapply(&ops, &before).await?;
        drop(ops);
        Ok(json!(settings))
    }

    /// The engine's reading of share links (normalised link, name, problem), nothing saved.
    pub(crate) async fn inspect(&self, text: &str) -> Result<Vec<Value>> {
        let data =
            self.inner.engine.post("/v1/subscription", &json!({ "url": "", "text": text }), INSPECT_TIMEOUT).await?;
        let nodes = data["nodes"].as_array().cloned().unwrap_or_default();
        Ok(nodes
            .into_iter()
            .map(|mut n| {
                if let Some(o) = n.as_object_mut() {
                    o.remove("id");
                    o.remove("via");
                }
                n
            })
            .collect())
    }

    /// Adds (`id` absent) or replaces a node after the engine has read its link; `via` "" clears the front node.
    pub(crate) async fn node_save(
        &self,
        id: Option<String>,
        link: &str,
        name: Option<&str>,
        via: Option<&str>,
    ) -> Result<Value> {
        let parsed = self.inspect(link).await?;
        let mut node =
            parsed.first().and_then(Node::from_engine).ok_or_else(|| Error::failed("no node in the link"))?;
        if let Some(problem) = node.problem.take() {
            return Err(Error::failed(problem));
        }
        if let Some(n) = clean_name(name) {
            node.name = n;
        }
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        let via = via.map(str::to_owned);
        let saved_id = self
            .inner
            .commit(&ops, move |p| {
                let id = match id {
                    Some(id) if p.node(&id).is_some() => id,
                    Some(_) => return Err(Error::not_found("node")),
                    None => next_id("L", p.local_nodes.iter().map(|n| n.id.as_str())),
                };
                node.id = id.clone();
                // a subscription node keeps its front node unless one is given; its edit lasts until the next update
                node.via = match via.as_deref() {
                    Some("") => None,
                    Some(v) => Some(v.to_owned()),
                    None => p.node(&id).and_then(|n| n.via.clone()),
                };
                // its 本地 SOCKS5 port stays with it
                node.local_socks = p.node(&id).and_then(|n| n.local_socks);
                if node.via.is_some() {
                    nodes::chain(p, &node).map_err(|e| Error::field(e.code(), "via", e.message()))?;
                }
                if p.node(&id).is_some() {
                    if let Some(slot) = p.nodes_mut().find(|n| n.id == id) {
                        *slot = node;
                    }
                } else {
                    p.local_nodes.push(node);
                }
                if p.selected.is_none() {
                    p.selected = Some(id.clone());
                }
                nodes::unique_names(p);
                Ok(id)
            })
            .await?;
        self.inner.recheck(&ops).await;
        if let Err(e) = self.inner.reapply(&ops, &before).await {
            tracing::warn!(error = %e, "the proxy did not restart after a node change");
        }
        drop(ops);
        let p = self.inner.profile();
        p.node(&saved_id).map(|n| json!(n)).ok_or_else(|| Error::not_found("node"))
    }

    /// Removes a local node; the selection moves to the first usable node.
    pub(crate) async fn node_delete(&self, id: &str) -> Result<Value> {
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        self.inner
            .commit(&ops, |p| {
                if !p.local_nodes.iter().any(|n| n.id == id) {
                    return Err(match p.node(id) {
                        Some(_) => Error::forbidden("a subscription's nodes go with the subscription"),
                        None => Error::not_found("node"),
                    });
                }
                p.local_nodes.retain(|n| n.id != id);
                nodes::forget_via(p, id);
                if p.selected.as_deref() == Some(id) {
                    p.selected = p.first_usable();
                }
                Ok(())
            })
            .await?;
        self.inner.recheck(&ops).await;
        if let Err(e) = self.inner.reapply(&ops, &before).await {
            tracing::warn!(error = %e, "the proxy did not restart after a node change");
        }
        drop(ops);
        Ok(self.profile_view())
    }

    /// One request through node `id` (with its chain); the delay is remembered for the groups and the failover.
    pub(crate) async fn probe(&self, id: &str, method: &str) -> Result<Value> {
        let (body, name) = {
            let st = self.inner.lock();
            let body = request::probe(&st.profile, id, method)?;
            (body, st.profile.node(id).map(|n| n.name.clone()).unwrap_or_default())
        };
        let r = self.inner.probes.run(&self.inner.engine, &body).await;
        let ms = r.as_ref().ok().and_then(|v| v["ms"].as_i64()).filter(|ms| *ms > 0);
        if method == "url" && !r.as_ref().is_err_and(|e| e.code() == Code::Busy) {
            self.inner.record_delay(&name, ms);
        }
        match (r, ms) {
            (Ok(_), Some(ms)) => Ok(json!({ "ms": ms })),
            (Ok(_), None) => Err(Error::failed("the node did not answer")),
            (Err(e), _) => Err(e),
        }
    }

    pub(crate) async fn rules_check(&self, text: &str) -> Result<Value> {
        let body = request::rules_check(&self.inner.profile(), text);
        self.inner.engine.post("/v1/rules/check", &body, RULES_TIMEOUT).await
    }

    /// Saves a config (`id` absent: a new one) once the engine has checked its rules.
    pub(crate) async fn config_save(
        &self,
        id: Option<String>,
        name: &str,
        text: &str,
        url: Option<&str>,
    ) -> Result<Value> {
        if id.as_deref() == Some(BUILTIN_ID) {
            return Err(Error::forbidden("the built-in config cannot be changed; save a copy under another name"));
        }
        let info = self.rules_check(text).await?;
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        let (name, text, url) = (name.trim().to_owned(), text.to_owned(), url.map(str::trim).map(str::to_owned));
        let saved = self
            .inner
            .commit(&ops, move |p| {
                let id = match id {
                    Some(id) if p.config(&id).is_some() => id,
                    Some(_) => return Err(Error::not_found("config")),
                    None => next_id("c", p.configs.iter().map(|c| c.id.as_str())),
                };
                let old_url = p.config(&id).and_then(|c| c.url.clone());
                let c = Config {
                    id: id.clone(),
                    name: if name.is_empty() { id.clone() } else { name },
                    text,
                    url: match url {
                        Some(u) if u.is_empty() => None,
                        Some(u) => Some(u),
                        None => old_url,
                    },
                    updated_at: Some(f50_core::time::unix_now()),
                    builtin: false,
                    info: Some(info),
                };
                match p.configs.iter_mut().find(|x| x.id == id) {
                    Some(slot) => *slot = c.clone(),
                    None => p.configs.push(c.clone()),
                }
                Ok(c)
            })
            .await?;
        if let Err(e) = self.inner.reapply(&ops, &before).await {
            tracing::warn!(error = %e, "the proxy did not restart with the changed config");
        }
        drop(ops);
        Ok(json!(saved))
    }

    pub(crate) async fn config_delete(&self, id: &str) -> Result<Value> {
        if id == BUILTIN_ID {
            return Err(Error::forbidden("the built-in config cannot be deleted"));
        }
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        self.inner
            .commit(&ops, |p| {
                p.config(id).ok_or_else(|| Error::not_found("config"))?;
                p.configs.retain(|c| c.id != id);
                p.group_selections.remove(id);
                if p.active_config == id {
                    p.active_config = BUILTIN_ID.into();
                }
                Ok(())
            })
            .await?;
        if let Err(e) = self.inner.reapply(&ops, &before).await {
            tracing::warn!(error = %e, "the proxy did not restart with the built-in config");
        }
        drop(ops);
        Ok(self.profile_view())
    }

    /// A config's text from its link (through the running node when there is one).
    pub(crate) async fn config_fetch(&self, url: &str) -> Result<Value> {
        let data = self.inner.engine.post("/v1/rules/fetch", &json!({ "url": url }), FETCH_TIMEOUT).await?;
        let text = data["text"].as_str().ok_or_else(|| Error::failed("the link returned no config"))?;
        Ok(json!({ "text": text }))
    }

    pub(crate) async fn connections(&self) -> Result<Value> {
        self.inner.engine.get("/v1/connections", SHORT).await
    }

    pub(crate) async fn connections_close(&self, id: i64) -> Result<Value> {
        self.inner.engine.post("/v1/connections/close", &json!({ "id": id }), SHORT).await
    }

    /// The engine's log lines after `after` of run `epoch` (both checked words or numbers).
    pub(crate) async fn logs(&self, epoch: Option<&str>, after: Option<i64>) -> Result<Value> {
        let path = format!("/v1/logs?epoch={}&after={}", epoch.unwrap_or(""), after.unwrap_or(0).max(0));
        self.inner.engine.get(&path, SHORT).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macs() {
        assert_eq!(parse_macs("").expect("empty"), Vec::<String>::new());
        assert_eq!(
            parse_macs("AA:BB:CC:DD:EE:FF, 11-22-33-44-55-66,aa:bb:cc:dd:ee:ff").expect("macs"),
            ["aa:bb:cc:dd:ee:ff", "11:22:33:44:55:66"]
        );
        assert!(parse_macs("aa:bb:cc:dd:ee").is_err());
        assert!(parse_macs("aa:bb:cc:dd:ee:fg").is_err());
    }
}

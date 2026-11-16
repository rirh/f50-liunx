//! Subscriptions: added and updated as jobs (the engine fetches them, through the running node when there is one,
//! with its device headers), renamed, removed; refreshed by the supervisor every `subscription_refresh_hours`.
//!
//! A fetch never holds `ops`: only taking its nodes in does. An update replaces the subscription's nodes and
//! carries the selection and the front nodes over by name (`nodes::replace_subscription_nodes`); a failed one keeps
//! the nodes and records the error on the subscription.

use std::time::Duration;

use f50_core::jobs::{JobCtx, JobSpec};
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::Proxy;
use crate::nodes;
use crate::profile::{Node, Subscription, Usage, next_id};

/// The engine's fetch is bounded at 20 s; parsing a large subscription takes some more.
const FETCH_TIMEOUT: Duration = Duration::from_secs(45);
const JOB_TIMEOUT: Duration = Duration::from_secs(600);

/// A fetched subscription.
#[derive(Debug)]
pub(crate) struct Fetched {
    pub nodes: Vec<Node>,
    pub title: Option<String>,
    pub usage: Option<Usage>,
}

pub(crate) fn parse_fetched(data: &Value) -> Result<Fetched> {
    let nodes: Vec<Node> =
        data["nodes"].as_array().map(|a| a.iter().filter_map(Node::from_engine).collect()).unwrap_or_default();
    if nodes.is_empty() {
        return Err(Error::failed("the subscription has no nodes"));
    }
    Ok(Fetched {
        nodes,
        title: data["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned),
        usage: serde_json::from_value(data["usage"].clone()).ok(),
    })
}

/// The host of an http(s) URL, for a subscription without a name.
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.rsplit_once('@').map_or(host, |(_, h)| h).to_owned()
}

impl Proxy {
    async fn fetch(&self, url: &str) -> Result<Fetched> {
        let data =
            self.inner.engine.post("/v1/subscription", &json!({ "url": url, "text": "" }), FETCH_TIMEOUT).await?;
        parse_fetched(&data)
    }

    /// Takes the nodes of the subscription `id` in (when it still has `url`), or records why the fetch failed.
    async fn take_in(&self, id: &str, url: &str, r: &Result<Fetched>) -> Result<bool> {
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        let took = self
            .inner
            .commit(&ops, |p| {
                let Some(idx) = p.subscriptions.iter().position(|s| s.id == id && s.url == url) else {
                    return Ok(false);
                };
                match r {
                    Ok(f) => {
                        let s = &mut p.subscriptions[idx];
                        s.title = f.title.clone();
                        s.usage = f.usage;
                        s.updated_at = Some(unix_now());
                        s.error = None;
                        nodes::replace_subscription_nodes(p, idx, f.nodes.clone());
                        Ok(true)
                    }
                    Err(e) => {
                        p.subscriptions[idx].error = Some(e.message().to_owned());
                        Ok(false)
                    }
                }
            })
            .await?;
        if took {
            self.inner.recheck(&ops).await;
            if let Err(e) = self.inner.reapply(&ops, &before).await {
                tracing::warn!(error = %e, "the proxy did not restart with the updated subscription");
            }
        }
        Ok(took)
    }

    /// Fetches and takes in one subscription; `Ok(false)` when it changed or went meanwhile.
    pub(crate) async fn refresh_one(&self, id: &str, url: &str) -> Result<bool> {
        let r = self.fetch(url).await;
        let took = self.take_in(id, url, &r).await?;
        r.map(|_| took)
    }

    pub(crate) fn subscription_add(&self, url: String, name: Option<String>) -> Result<String> {
        let spec = JobSpec::new("f50.proxy subscription_add", "proxy", JOB_TIMEOUT);
        let me = self.clone();
        self.inner.jobs.start(spec, move |ctx: JobCtx| async move {
            ctx.log("fetching the subscription");
            let f = me.fetch(&url).await?;
            ctx.log(format!("{} nodes", f.nodes.len()));
            let ops = me.inner.ops.lock().await;
            let before = me.inner.fingerprint();
            let count = f.nodes.len();
            let id = me
                .inner
                .commit(&ops, |p| {
                    let id = next_id("s", p.subscriptions.iter().map(|s| s.id.as_str()));
                    let name = name.or_else(|| f.title.clone()).unwrap_or_else(|| host(&url));
                    p.subscriptions.push(Subscription {
                        id: id.clone(),
                        name,
                        url: url.clone(),
                        title: f.title.clone(),
                        usage: f.usage,
                        updated_at: Some(unix_now()),
                        error: None,
                        nodes: Vec::new(),
                    });
                    let idx = p.subscriptions.len() - 1;
                    nodes::replace_subscription_nodes(p, idx, f.nodes);
                    Ok(id)
                })
                .await?;
            me.inner.recheck(&ops).await;
            if let Err(e) = me.inner.reapply(&ops, &before).await {
                ctx.log(format!("the proxy did not restart: {}", e.message()));
            }
            drop(ops);
            Ok(json!({ "id": id, "nodes": count }))
        })
    }

    /// Updates one subscription or `all`, as a job.
    pub(crate) fn subscription_update(&self, id: &str) -> Result<String> {
        let targets: Vec<(String, String)> = {
            let st = self.inner.lock();
            st.profile
                .subscriptions
                .iter()
                .filter(|s| id == "all" || s.id == id)
                .map(|s| (s.id.clone(), s.url.clone()))
                .collect()
        };
        if targets.is_empty() && id != "all" {
            return Err(Error::not_found("subscription"));
        }
        let spec = JobSpec::new("f50.proxy subscription_update", "proxy", JOB_TIMEOUT);
        let me = self.clone();
        self.inner.jobs.start(spec, move |ctx: JobCtx| async move {
            let (mut updated, mut failed) = (0, 0);
            for (sid, url) in targets {
                if ctx.is_cancelled() {
                    break;
                }
                match me.refresh_one(&sid, &url).await {
                    Ok(_) => {
                        updated += 1;
                        ctx.log(format!("{sid}: updated"));
                    }
                    Err(e) => {
                        failed += 1;
                        ctx.log(format!("{sid}: {}", e.message()));
                    }
                }
            }
            Ok(json!({ "updated": updated, "failed": failed }))
        })
    }

    pub(crate) async fn subscription_set(&self, id: &str, name: Option<&str>, url: Option<&str>) -> Result<Value> {
        let ops = self.inner.ops.lock().await;
        self.inner
            .commit(&ops, |p| {
                let s =
                    p.subscriptions.iter_mut().find(|s| s.id == id).ok_or_else(|| Error::not_found("subscription"))?;
                if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
                    s.name = n.to_owned();
                }
                if let Some(u) = url
                    && u != s.url
                {
                    // the nodes stay until the next update fetches the new address
                    s.url = u.to_owned();
                    s.error = None;
                }
                Ok(())
            })
            .await?;
        drop(ops);
        Ok(self.profile_view())
    }

    /// Removes a subscription and its nodes; the selection moves to the first usable node left.
    pub(crate) async fn subscription_remove(&self, id: &str) -> Result<Value> {
        let ops = self.inner.ops.lock().await;
        let before = self.inner.fingerprint();
        self.inner
            .commit(&ops, |p| {
                let idx =
                    p.subscriptions.iter().position(|s| s.id == id).ok_or_else(|| Error::not_found("subscription"))?;
                let gone: Vec<String> = p.subscriptions.remove(idx).nodes.into_iter().map(|n| n.id).collect();
                for g in &gone {
                    nodes::forget_via(p, g);
                }
                if p.selected.as_ref().is_some_and(|s| gone.contains(s)) {
                    p.selected = p.first_usable();
                }
                nodes::unique_names(p);
                Ok(())
            })
            .await?;
        self.inner.recheck(&ops).await;
        if let Err(e) = self.inner.reapply(&ops, &before).await {
            tracing::warn!(error = %e, "the proxy did not restart after a subscription was removed");
        }
        drop(ops);
        Ok(self.profile_view())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetched_subscriptions() {
        let data = json!({
            "nodes": [{"id": "0", "name": "HK", "link": "vless://a"}, {"id": "1", "name": "x"}],
            "title": " 机场 ",
            "usage": {"upload": 1, "download": 2, "total": 10, "expire": 0},
        });
        let f = parse_fetched(&data).expect("fetched");
        assert_eq!(f.nodes.len(), 1, "a node without a link is dropped");
        assert_eq!(f.title.as_deref(), Some("机场"));
        assert_eq!(f.usage.map(|u| u.total), Some(10));
        let f = parse_fetched(&json!({"nodes": [{"link": "ss://x"}], "usage": null})).expect("fetched");
        assert_eq!(f.usage, None);
        assert!(parse_fetched(&json!({"nodes": []})).is_err());
    }

    #[test]
    fn hosts() {
        assert_eq!(host("https://sub.example.com/api?token=x"), "sub.example.com");
        assert_eq!(host("http://u:p@h:8080/x"), "h:8080");
    }
}

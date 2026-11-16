//! 策略组 (one-node's `pc/src-tauri/src/groups.rs`): `[{name, type, selected, nodes: [{name, delay_ms, alive}]}]`.
//!
//! While the engine runs it runs the groups (tests urltest/fallback members and picks) and they come from its
//! status; otherwise from the active config's last check, select groups on the saved pick, delays from the last
//! tests made here. A select group's pick is saved per config id and group name (`group_selections`).

use std::collections::HashMap;
use std::time::Duration;

use f50_core::{Error, Result};
use serde_json::{Value, json};
use tokio::task::JoinSet;

use crate::Proxy;
use crate::nodes;
use crate::profile::Profile;
use crate::request::{self, PROBE_TIMEOUT_MS};

/// The active config's checked groups.
fn checked(p: &Profile) -> Vec<Value> {
    p.active().info.as_ref().and_then(|i| i["groups"].as_array()).cloned().unwrap_or_default()
}

fn members(g: &Value) -> Vec<String> {
    g["nodes"].as_array().map(|n| n.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default()
}

/// The groups as they stand without the engine.
pub(crate) fn idle(p: &Profile, delays: &HashMap<String, Option<i64>>) -> Vec<Value> {
    let picks = p.selections();
    checked(p)
        .iter()
        .map(|g| {
            let members = members(g);
            let delay = |n: &str| delays.get(n).copied();
            let alive = |n: &str| delay(n).is_none_or(|d| d.is_some());
            let first = members.first().cloned().unwrap_or_default();
            let selected = match g["type"].as_str() {
                Some("select") => g["name"]
                    .as_str()
                    .and_then(|name| picks.get(name))
                    .filter(|pick| members.contains(pick))
                    .cloned()
                    .unwrap_or(first),
                Some("urltest") => members
                    .iter()
                    .filter_map(|n| delay(n).flatten().map(|ms| (ms, n)))
                    .min_by_key(|(ms, _)| *ms)
                    .map_or(first, |(_, n)| n.clone()),
                _ => members.iter().find(|n| alive(n)).cloned().unwrap_or(first),
            };
            json!({
                "name": g["name"],
                "type": g["type"],
                "selected": selected,
                "nodes": members.iter().map(|n| json!({
                    "name": n, "delay_ms": delay(n).flatten(), "alive": alive(n),
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

impl Proxy {
    fn idle_groups(&self) -> Value {
        let st = self.inner.lock();
        json!(idle(&st.profile, &st.delays))
    }

    /// Points a select group at a node and remembers the pick for the active config.
    pub(crate) async fn group_select(&self, group: &str, node: &str) -> Result<Value> {
        let running = self.inner.engine_running().await;
        let groups = if running {
            self.inner
                .engine
                .post("/v1/groups/select", &json!({ "group": group, "node": node }), Duration::from_secs(5))
                .await?
        } else {
            let p = self.inner.profile();
            let g = checked(&p)
                .into_iter()
                .find(|g| g["name"] == json!(group))
                .ok_or_else(|| Error::not_found(format!("group {group}")))?;
            if g["type"] != json!("select") {
                return Err(Error::failed(format!("the group {group} picks its node itself")));
            }
            if !members(&g).iter().any(|m| m == node) {
                return Err(Error::failed(format!("{node} is not in the group {group}")));
            }
            Value::Null
        };
        let ops = self.inner.ops.lock().await;
        let (g, n) = (group.to_owned(), node.to_owned());
        self.inner
            .commit(&ops, move |p| {
                let id = p.active_config.clone();
                p.group_selections.entry(id).or_default().insert(g, n);
                Ok(())
            })
            .await?;
        drop(ops);
        Ok(if groups.is_array() { groups } else { self.idle_groups() })
    }

    /// Tests a group's members: the engine's own test while it runs, else a probe through each member here.
    pub(crate) async fn group_test(&self, group: &str) -> Result<Value> {
        if self.inner.engine_running().await {
            return self
                .inner
                .engine
                .post("/v1/groups/test", &json!({ "group": group }), Duration::from_millis(PROBE_TIMEOUT_MS + 15_000))
                .await;
        }
        let bodies: Vec<(String, Value)> = {
            let st = self.inner.lock();
            let p = &st.profile;
            let g = checked(p)
                .into_iter()
                .find(|g| g["name"] == json!(group))
                .ok_or_else(|| Error::not_found(format!("group {group}")))?;
            let names = members(&g);
            p.nodes()
                .filter(|n| n.usable() && names.contains(&n.name))
                .filter_map(|n| {
                    let via = nodes::via_links(p, n).ok()?;
                    Some((n.name.clone(), request::probe_body(&n.link, &via, "url", &p.settings.probe_url)))
                })
                .collect()
        };
        let mut set = JoinSet::new();
        for (name, body) in bodies {
            let engine = self.inner.engine.clone();
            set.spawn(async move {
                let wait = Duration::from_millis(PROBE_TIMEOUT_MS + 4000);
                let ms = engine
                    .post("/v1/probe", &body, wait)
                    .await
                    .ok()
                    .and_then(|r| r["ms"].as_i64())
                    .filter(|ms| *ms > 0);
                (name, ms)
            });
        }
        while let Some(r) = set.join_next().await {
            if let Ok((name, ms)) = r {
                self.inner.record_delay(&name, ms);
            }
        }
        Ok(self.idle_groups())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Config;
    use std::collections::BTreeMap;

    #[test]
    fn idle_groups_follow_picks_and_delays() {
        let mut p = Profile::default();
        let info = json!({"groups": [
            {"name": "手选", "type": "select", "nodes": ["HK", "US"]},
            {"name": "自动", "type": "urltest", "nodes": ["HK", "US", "JP"]},
            {"name": "备用", "type": "fallback", "nodes": ["HK", "US"]},
        ]});
        p.configs.push(Config { id: "c1".into(), info: Some(info), ..Config::default() });
        p.active_config = "c1".into();
        p.group_selections.insert("c1".into(), BTreeMap::from([("手选".to_owned(), "US".to_owned())]));
        let delays =
            HashMap::from([("HK".to_owned(), None), ("US".to_owned(), Some(120)), ("JP".to_owned(), Some(90))]);
        let g = idle(&p, &delays);
        assert_eq!(g[0]["selected"], "US");
        assert_eq!(g[1]["selected"], "JP");
        assert_eq!(g[2]["selected"], "US", "the first alive");
        assert_eq!(g[0]["nodes"][0], json!({"name": "HK", "delay_ms": null, "alive": false}));
        assert_eq!(g[1]["nodes"][2], json!({"name": "JP", "delay_ms": 90, "alive": true}));
        p.group_selections.clear();
        assert_eq!(idle(&p, &HashMap::new())[0]["selected"], "HK");
    }
}

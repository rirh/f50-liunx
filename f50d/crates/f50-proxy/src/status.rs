//! `f50.proxy status`: the run as people see it (`f50-proxy/DESIGN.md` §2 "Status"), from the runtime state and the
//! engine's `/v1/status` (`{running, node, since, up, down, xray, mem, groups, health}`). Nothing private: no link,
//! no host, no client address.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::groups;
use crate::profile::Profile;
use crate::service::{Phase, rfc3339_secs};

/// What `status` is made of, read under the state lock.
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    pub profile: Profile,
    pub phase: Phase,
    pub error: Option<String>,
    pub delays: HashMap<String, Option<i64>>,
}

pub(crate) fn view(s: &Snapshot, engine: Option<&Value>) -> Value {
    let p = &s.profile;
    let running = engine.is_some_and(|e| e["running"] == Value::Bool(true));
    let state = if !p.enabled {
        "off"
    } else if s.phase == Phase::Error {
        "error"
    } else if running && s.phase == Phase::Running {
        "running"
    } else {
        "starting"
    };
    let error = match (state, &s.error) {
        ("error", Some(e)) => Some(e.clone()),
        ("starting", _) if engine.is_none() => Some("the proxy engine is not answering".to_owned()),
        _ => None,
    };
    let node = p.selected.as_deref().and_then(|id| p.node(id)).map(|n| json!({ "id": n.id, "name": n.name }));
    let e = engine.filter(|_| running);
    let num = |k: &str| e.and_then(|e| e[k].as_u64()).unwrap_or(0);
    let groups = match e.map(|e| &e["groups"]) {
        Some(Value::Array(g)) => Value::Array(g.clone()),
        Some(_) => json!([]),
        None => json!(groups::idle(p, &s.delays)),
    };
    json!({
        "enabled": p.enabled,
        "state": state,
        "error": error,
        "routing": p.routing.as_str(),
        "config": p.active_config,
        "node": node,
        "since": e.and_then(|e| e["since"].as_str()).and_then(rfc3339_secs),
        "up": num("up"),
        "down": num("down"),
        "mem": engine.and_then(|e| e["mem"].as_u64()).unwrap_or(0),
        "xray": engine.and_then(|e| e["xray"].as_str()),
        "engine": engine.and_then(|e| e["version"].as_str().or_else(|| e["engine"].as_str())),
        "intercept_router": p.settings.intercept_router,
        "groups": groups,
        "health": e.and_then(|e| e.get("health")).filter(|h| h.is_object()).cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Node;

    fn snap(enabled: bool, phase: Phase) -> Snapshot {
        let mut profile = Profile { enabled, selected: Some("L1".into()), ..Profile::default() };
        profile
            .local_nodes
            .push(Node { id: "L1".into(), name: "香港 01".into(), link: "x".into(), ..Node::default() });
        Snapshot { profile, phase, error: Some("boom".into()), delays: HashMap::new() }
    }

    #[test]
    fn states() {
        let engine = json!({"running": true, "since": "2025-11-16T08:00:00Z", "up": 10, "down": 20, "mem": 5,
            "xray": "26.10.29", "groups": [{"name": "g"}], "health": {"online": true, "node_ok": true}});
        let v = view(&snap(true, Phase::Running), Some(&engine));
        assert_eq!(v["state"], "running");
        assert_eq!(v["error"], Value::Null);
        assert_eq!(v["node"], json!({"id": "L1", "name": "香港 01"}));
        assert_eq!(v["since"], 1_763_280_000);
        assert_eq!((v["up"].as_u64(), v["down"].as_u64()), (Some(10), Some(20)));
        assert_eq!(v["groups"], json!([{"name": "g"}]));
        assert_eq!(v["health"]["node_ok"], true);
        assert_eq!(v["engine"], Value::Null);

        let v = view(&snap(false, Phase::Off), Some(&json!({"running": false, "mem": 3, "xray": "x"})));
        assert_eq!(v["state"], "off");
        assert_eq!(v["since"], Value::Null);
        assert_eq!(v["groups"], json!([]));
        assert_eq!(v["mem"], 3);

        let v = view(&snap(true, Phase::Error), None);
        assert_eq!((v["state"].as_str(), v["error"].as_str()), (Some("error"), Some("boom")));
        let v = view(&snap(true, Phase::Starting), None);
        assert_eq!(v["state"], "starting");
        assert!(v["error"].as_str().is_some_and(|e| e.contains("not answering")));
    }
}

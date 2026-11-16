//! The engine's requests built from the profile: the start request (one-node's `Inner::start`, mode `gateway`)
//! and the bodies of the rule check and the probe.

use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::nodes;
use crate::profile::{Node, Profile};

/// The engine's ports and the mark of its own sockets (`f50-proxy/DESIGN.md` §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ports {
    /// The mixed (SOCKS5 + HTTP) inbound on loopback.
    pub mixed: u16,
    /// The tproxy inbound the firewall sends LAN and router traffic to.
    pub tproxy: u16,
    /// `SO_MARK` of every outbound socket: the firewall lets it pass.
    pub mark: u32,
}

/// How long one probe may take (the engine's `timeout_ms`).
pub(crate) const PROBE_TIMEOUT_MS: u64 = 5000;

/// The selected node, usable.
pub(crate) fn selected(p: &Profile) -> Result<&Node> {
    let id = p.selected.as_deref().ok_or_else(|| Error::failed("no node is selected"))?;
    let n = p.node(id).ok_or_else(|| Error::not_found("node"))?;
    match &n.problem {
        Some(problem) => Err(Error::failed(format!("the node \"{}\" cannot be used: {problem}", n.name))),
        None => Ok(n),
    }
}

/// `POST /v1/start`: the selected node with its front nodes, every node the rules may name, the config in use
/// and its saved select-group picks.
pub(crate) fn start(p: &Profile, ports: Ports) -> Result<Value> {
    let node = selected(p)?;
    let via = nodes::via_links(p, node)?;
    Ok(json!({
        "link": node.link,
        "name": node.name,
        "via": via,
        "mode": "gateway",
        "routing": p.routing.as_str(),
        "rules_json": p.active().text,
        "rule_sets": p.settings.rule_sets_url,
        "port": ports.mixed,
        "tproxy_port": ports.tproxy,
        "mark": ports.mark,
        "nodes": nodes::named_nodes(p),
        "selections": p.selections(),
        "app_hosts": [],
    }))
}

/// `POST /v1/rules/check` of `text` against the nodes there are.
pub(crate) fn rules_check(p: &Profile, text: &str) -> Value {
    json!({ "text": text, "rule_sets": p.settings.rule_sets_url, "nodes": nodes::node_names(p) })
}

/// `POST /v1/probe` through node `id` (with its chain) to the 测速 address.
pub(crate) fn probe(p: &Profile, id: &str, method: &str) -> Result<Value> {
    let n = p.node(id).ok_or_else(|| Error::not_found("node"))?;
    Ok(probe_body(&n.link, &nodes::via_links(p, n)?, method, &p.settings.probe_url))
}

pub(crate) fn probe_body(link: &str, via: &[String], method: &str, url: &str) -> Value {
    json!({ "link": link, "via": via, "method": method, "url": url, "timeout_ms": PROBE_TIMEOUT_MS })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Config, Routing, Subscription};
    use std::collections::BTreeMap;

    fn node(id: &str, name: &str) -> Node {
        Node { id: id.into(), name: name.into(), link: format!("link-{id}"), ..Node::default() }
    }

    const PORTS: Ports = Ports { mixed: 17890, tproxy: 17892, mark: 255 };

    #[test]
    fn start_request_carries_node_chain_and_groups() {
        let mut p = Profile::default();
        p.subscriptions.push(Subscription {
            id: "s1".into(),
            nodes: vec![
                node("s1-1", "HK"),
                node("s1-2", "US"),
                Node { problem: Some("x".into()), ..node("s1-3", "B") },
            ],
            ..Subscription::default()
        });
        p.local_nodes = vec![node("L1", "front")];
        p.subscriptions[0].nodes[1].via = Some("L1".into());
        p.selected = Some("s1-2".into());
        p.routing = Routing::Global;
        p.configs.push(Config {
            id: "c1".into(),
            name: "mine".into(),
            text: "{\"final\":\"direct\"}".into(),
            ..Config::default()
        });
        p.active_config = "c1".into();
        p.group_selections.insert("c1".into(), BTreeMap::from([("手选".to_owned(), "HK".to_owned())]));
        p.group_selections.insert("default".into(), BTreeMap::from([("x".to_owned(), "US".to_owned())]));

        let r = start(&p, PORTS).expect("request");
        assert_eq!(r["link"], "link-s1-2");
        assert_eq!(r["name"], "US");
        assert_eq!(r["via"], json!(["link-L1"]));
        assert_eq!(r["mode"], "gateway");
        assert_eq!(r["routing"], "global");
        assert_eq!(r["rules_json"], "{\"final\":\"direct\"}");
        assert_eq!(r["port"], 17890);
        assert_eq!(r["tproxy_port"], 17892);
        assert_eq!(r["mark"], 255);
        assert_eq!(r["selections"], json!({"手选": "HK"}), "the picks of the config in use only");
        let names: Vec<_> = r["nodes"].as_array().expect("nodes").iter().map(|n| n["name"].clone()).collect();
        assert_eq!(names, [json!("HK"), json!("US"), json!("front")], "the broken node is left out");
        assert_eq!(r["nodes"][1]["via"], json!(["link-L1"]));
        assert_eq!(r["app_hosts"], json!([]));

        p.selected = Some("s1-3".into());
        assert!(start(&p, PORTS).is_err(), "a node with a problem is not started");
        p.selected = None;
        assert!(start(&p, PORTS).is_err());
    }

    #[test]
    fn default_config_is_the_builtin() {
        let p = Profile { local_nodes: vec![node("L1", "a")], selected: Some("L1".into()), ..Profile::default() };
        let r = start(&p, PORTS).expect("request");
        assert_eq!(r["rules_json"], crate::profile::BUILTIN_TEXT);
        assert_eq!(r["rule_sets"], crate::profile::DEFAULT_RULE_SETS);
        assert_eq!(r["selections"], json!({}));
        assert_eq!(rules_check(&p, "{}")["nodes"], json!(["a"]));
        assert_eq!(probe(&p, "L1", "url").expect("probe")["url"], crate::profile::DEFAULT_PROBE_URL);
    }
}

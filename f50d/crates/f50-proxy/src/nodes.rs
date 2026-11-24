//! The nodes as the engine needs them, and what keeps them consistent when they change.
//!
//! - Names are unique across every node (`HK`, `HK (2)`) before they reach the engine: rules and groups name nodes.
//! - 前置代理 (`via`, one-node's `pc/src-tauri/src/chain.rs`): a node may connect through another, which may have its
//!   own; the engine takes the chain as links, nearest first. A loop, a missing or broken front node is refused.
//! - A subscription refresh replaces its nodes (ids are positions, `<sub>-<n>`): the selection, every `via` and
//!   the 本地 SOCKS5 ports follow the node's name; one whose node did not come back goes.
//! - 本地 SOCKS5 (one-node's `localsocks`): the engine runs a SOCKS5 port per node that has one, on the LAN.

use std::collections::{HashMap, HashSet};

use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::profile::{LocalSocks, Node, Profile};

/// The longest chain of front nodes.
pub(crate) const MAX_HOPS: usize = 8;

/// Makes every node's name unique, subscriptions first, then the local nodes (`name (2)`, `name (3)`...).
pub(crate) fn unique_names(p: &mut Profile) {
    let mut seen: HashSet<String> = HashSet::new();
    for n in p.nodes_mut() {
        let base = n.name.trim();
        let base = if base.is_empty() { "node".to_owned() } else { base.to_owned() };
        let mut name = base.clone();
        let mut k = 2;
        while seen.contains(&name) {
            name = format!("{base} ({k})");
            k += 1;
        }
        seen.insert(name.clone());
        n.name = name;
    }
}

/// The nodes `node` connects through, nearest first.
pub(crate) fn chain<'a>(p: &'a Profile, node: &'a Node) -> Result<Vec<&'a Node>> {
    let mut hops: Vec<&Node> = Vec::new();
    let mut at = node;
    while let Some(id) = at.via.as_deref() {
        let front =
            p.node(id).ok_or_else(|| Error::failed(format!("the front node of \"{}\" no longer exists", at.name)))?;
        if front.id == node.id || hops.iter().any(|h| h.id == front.id) {
            let path: Vec<&str> =
                std::iter::once(node).chain(hops.iter().copied()).chain([front]).map(|n| n.name.as_str()).collect();
            return Err(Error::failed(format!("the front nodes form a loop: {}", path.join(" → "))));
        }
        if let Some(problem) = &front.problem {
            return Err(Error::failed(format!("the front node \"{}\" cannot be used: {problem}", front.name)));
        }
        if hops.len() == MAX_HOPS {
            return Err(Error::failed(format!("at most {MAX_HOPS} front nodes")));
        }
        hops.push(front);
        at = front;
    }
    Ok(hops)
}

/// `node`'s front nodes as the engine takes them: their links, nearest first.
pub(crate) fn via_links(p: &Profile, node: &Node) -> Result<Vec<String>> {
    Ok(chain(p, node)?.into_iter().map(|n| n.link.clone()).collect())
}

/// Every usable node as the engine takes them, `[{name, link, via}]`; a broken chain leaves its node out.
pub(crate) fn named_nodes(p: &Profile) -> Vec<Value> {
    p.nodes()
        .filter(|n| n.usable())
        .filter_map(|n| {
            let via = via_links(p, n).ok()?;
            Some(json!({ "name": n.name, "link": n.link, "via": via }))
        })
        .collect()
}

pub(crate) fn node_names(p: &Profile) -> Vec<String> {
    named_nodes(p).into_iter().filter_map(|n| n["name"].as_str().map(str::to_owned)).collect()
}

/// The engine's 本地 SOCKS5 set (`/v1/local-socks`): `[{id, name, link, via, port}]` of every usable node with a
/// port; a broken chain leaves its node out.
pub(crate) fn socks_entries(p: &Profile) -> Vec<Value> {
    p.nodes()
        .filter(|n| n.usable())
        .filter_map(|n| {
            let port = n.local_socks?.port;
            let via = via_links(p, n).ok()?;
            Some(json!({ "id": n.id, "name": n.name, "link": n.link, "via": via, "port": port }))
        })
        .collect()
}

/// Drops every `via` that pointed at `id`.
pub(crate) fn forget_via(p: &mut Profile, id: &str) {
    for n in p.nodes_mut() {
        if n.via.as_deref() == Some(id) {
            n.via = None;
        }
    }
}

/// Replaces subscription `idx`'s nodes with `fresh` (as the engine parsed them): ids `<sub>-<n>`, the `via`s and
/// the selection carried over by name, then the names made unique again.
pub(crate) fn replace_subscription_nodes(p: &mut Profile, idx: usize, fresh: Vec<Node>) {
    let sub_id = p.subscriptions[idx].id.clone();
    let old = std::mem::take(&mut p.subscriptions[idx].nodes);
    let old_name: HashMap<String, String> = old.iter().map(|n| (n.id.clone(), n.name.clone())).collect();
    let old_via: HashMap<String, String> = old.iter().filter_map(|n| Some((n.name.clone(), n.via.clone()?))).collect();
    let old_socks: HashMap<String, LocalSocks> =
        old.iter().filter_map(|n| Some((n.name.clone(), n.local_socks?))).collect();
    let mut new: Vec<Node> = fresh
        .into_iter()
        .enumerate()
        .map(|(i, mut n)| {
            n.id = format!("{sub_id}-{}", i + 1);
            n.via = None;
            n.local_socks = None;
            n
        })
        .collect();
    let mut new_id: HashMap<String, String> = HashMap::new();
    for n in &new {
        new_id.entry(n.name.clone()).or_insert_with(|| n.id.clone());
    }
    let prefix = format!("{sub_id}-");
    // a via into this subscription follows the name; one elsewhere stays when its node still exists
    let others: HashSet<String> = p.nodes().map(|n| n.id.clone()).collect();
    let remap = |via: &str| -> Option<String> {
        if via.starts_with(&prefix) {
            old_name.get(via).and_then(|name| new_id.get(name)).cloned()
        } else {
            others.contains(via).then(|| via.to_owned())
        }
    };
    let mut socks_kept: HashSet<String> = HashSet::new();
    for n in &mut new {
        // a name twice in the list: the port goes to the first only
        if socks_kept.insert(n.name.clone()) {
            n.local_socks = old_socks.get(&n.name).copied();
        }
        if let Some(v) = old_via.get(&n.name).and_then(|v| remap(v))
            && v != n.id
        {
            n.via = Some(v);
        }
    }
    for n in p.nodes_mut() {
        if let Some(v) = n.via.clone().filter(|v| v.starts_with(&prefix)) {
            n.via = remap(&v);
        }
    }
    if let Some(sel) = p.selected.clone().filter(|s| s.starts_with(&prefix)) {
        p.selected = old_name
            .get(&sel)
            .and_then(|name| new_id.get(name))
            .cloned()
            .or_else(|| new.iter().find(|n| n.usable()).map(|n| n.id.clone()));
    }
    p.subscriptions[idx].nodes = new;
    if p.selected.is_none() {
        p.selected = p.first_usable();
    }
    unique_names(p);
}

/// A node another one could be replaced by: `(id, name, link, via links)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub id: String,
    pub name: String,
    pub link: String,
    pub via: Vec<String>,
}

pub(crate) fn candidate(p: &Profile, n: &Node) -> Option<Candidate> {
    Some(Candidate { id: n.id.clone(), name: n.name.clone(), link: n.link.clone(), via: via_links(p, n).ok()? })
}

/// At most this many other nodes are tried at once for a replacement.
pub(crate) const MAX_CANDIDATES: usize = 12;

/// The usable nodes that could replace `current`: the same subscription's (the local nodes for a local one),
/// known-good by their last delay first, then untested, then the ones that failed last time.
pub(crate) fn candidates(p: &Profile, current: &str, delays: &HashMap<String, Option<i64>>) -> Vec<Candidate> {
    let pool: Vec<&Node> = match p.subscriptions.iter().find(|s| s.nodes.iter().any(|n| n.id == current)) {
        Some(s) => s.nodes.iter().collect(),
        None => p.local_nodes.iter().collect(),
    };
    let mut ranked: Vec<(u8, i64, Candidate)> = pool
        .into_iter()
        .filter(|n| n.usable() && n.id != current)
        .filter_map(|n| candidate(p, n))
        .map(|c| match delays.get(&c.name) {
            Some(Some(ms)) => (0, *ms, c),
            None => (1, 0, c),
            Some(None) => (2, 0, c),
        })
        .collect();
    ranked.sort_by_key(|(rank, ms, _)| (*rank, *ms));
    ranked.into_iter().take(MAX_CANDIDATES).map(|(_, _, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Subscription;

    pub(crate) fn node(id: &str, name: &str) -> Node {
        Node { id: id.into(), name: name.into(), link: format!("vless://{id}"), ..Node::default() }
    }

    fn profile() -> Profile {
        let mut p = Profile::default();
        p.subscriptions.push(Subscription {
            id: "s1".into(),
            name: "机场".into(),
            nodes: vec![node("s1-1", "HK"), node("s1-2", "US"), node("s1-3", "JP")],
            ..Subscription::default()
        });
        p.local_nodes = vec![node("L1", "home"), node("L2", "HK")];
        p
    }

    #[test]
    fn names_become_unique() {
        let mut p = profile();
        p.local_nodes.push(node("L3", "HK"));
        p.local_nodes.push(node("L4", " "));
        unique_names(&mut p);
        let names: Vec<_> = p.nodes().map(|n| n.name.clone()).collect();
        assert_eq!(names, ["HK", "US", "JP", "home", "HK (2)", "HK (3)", "node"]);
        unique_names(&mut p);
        let again: Vec<_> = p.nodes().map(|n| n.name.clone()).collect();
        assert_eq!(names, again, "stable");
    }

    #[test]
    fn chains_and_their_refusals() {
        let mut p = profile();
        p.subscriptions[0].nodes[0].via = Some("L1".into());
        p.local_nodes[0].via = Some("s1-2".into());
        let hk = p.node("s1-1").expect("hk").clone();
        assert_eq!(via_links(&p, &hk).expect("chain"), ["vless://L1", "vless://s1-2"]);

        p.subscriptions[0].nodes[1].via = Some("s1-1".into());
        let e = via_links(&p, &hk).expect_err("loop");
        assert!(e.message().contains("loop"), "{e}");
        assert!(named_nodes(&p).iter().all(|n| n["link"] != "vless://s1-1"), "a broken chain leaves its node out");

        p.subscriptions[0].nodes[1].via = None;
        p.subscriptions[0].nodes[1].problem = Some("unsupported".into());
        assert!(via_links(&p, &hk).is_err());
        forget_via(&mut p, "L1");
        assert_eq!(p.node("s1-1").and_then(|n| n.via.clone()), None);
    }

    #[test]
    fn refresh_carries_selection_and_via_by_name() {
        let mut p = profile();
        p.selected = Some("s1-2".into()); // US
        p.subscriptions[0].nodes[2].via = Some("s1-2".into()); // JP through US
        p.local_nodes[0].via = Some("s1-1".into()); // home through HK
        p.local_nodes[1].via = Some("s1-3".into()); // the local HK through JP, which goes away
        // reordered, JP gone, a new node
        let fresh = vec![node("0", "US"), node("1", "HK"), node("2", "SG")];
        replace_subscription_nodes(&mut p, 0, fresh);
        let ids: Vec<_> = p.subscriptions[0].nodes.iter().map(|n| (n.id.as_str(), n.name.as_str())).collect();
        assert_eq!(ids, [("s1-1", "US"), ("s1-2", "HK"), ("s1-3", "SG")]);
        assert_eq!(p.selected.as_deref(), Some("s1-1"), "the selection follows US");
        assert_eq!(p.local_nodes[0].via.as_deref(), Some("s1-2"), "home still goes through HK");
        assert_eq!(p.local_nodes[1].via, None, "its front node is gone");
        assert_eq!(p.local_nodes[1].name, "HK (2)");

        // the selected node gone: the subscription's first usable one
        p.selected = Some("s1-3".into());
        replace_subscription_nodes(&mut p, 0, vec![node("0", "US")]);
        assert_eq!(p.selected.as_deref(), Some("s1-1"));
    }

    #[test]
    fn socks_ports_follow_the_name_and_skip_broken_nodes() {
        let mut p = profile();
        p.subscriptions[0].nodes[0].local_socks = Some(LocalSocks { port: 1080 }); // HK
        p.subscriptions[0].nodes[2].local_socks = Some(LocalSocks { port: 1081 }); // JP, goes away
        p.local_nodes[0].local_socks = Some(LocalSocks { port: 1082 });
        p.local_nodes[0].via = Some("s1-1".into());
        let entries = socks_entries(&p);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[2],
            json!({"id": "L1", "name": "home", "link": "vless://L1", "via": ["vless://s1-1"], "port": 1082})
        );

        replace_subscription_nodes(&mut p, 0, vec![node("0", "US"), node("1", "HK"), node("2", "HK")]);
        let ports: Vec<_> = p.nodes().map(|n| (n.id.as_str(), n.local_socks.map(|s| s.port))).collect();
        assert_eq!(
            ports,
            [("s1-1", None), ("s1-2", Some(1080)), ("s1-3", None), ("L1", Some(1082)), ("L2", None)],
            "HK keeps 1080 (the first HK only), JP's port goes with it"
        );

        p.local_nodes[0].problem = Some("x".into());
        assert!(socks_entries(&p).iter().all(|e| e["id"] != "L1"), "an unusable node gets no port");
    }

    #[test]
    fn failover_candidates_stay_in_the_subscription() {
        let mut p = profile();
        p.subscriptions[0].nodes.push(node("s1-4", "KR"));
        p.subscriptions[0].nodes[2].problem = Some("x".into());
        let delays = HashMap::from([("US".to_owned(), None), ("KR".to_owned(), Some(80))]);
        let names: Vec<_> = candidates(&p, "s1-1", &delays).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["KR", "US"]);
        let names: Vec<_> = candidates(&p, "L1", &delays).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["HK"], "a local node is replaced by another local one");
    }
}

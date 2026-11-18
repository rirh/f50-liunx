//! The `f50.proxy` object (`f50-proxy/DESIGN.md` §2 "Methods").
//!
//! `status` alone is `Access::Read` (and the live topic `proxy`): it names the node and the groups but no link, host
//! or client address. Everything else returns or changes something private (links are credentials, connections
//! show the LAN's clients and the hosts they reach) and is `Access::Write`. Share links, subscription URLs and the
//! pasted text are `Field::secret`: never logged, never echoed in an error.

use std::future::Future;

use f50_core::Result;
use f50_core::rpc::{Access, Field, Object, ObjectBuilder, Params, Rule};
use serde::Serialize;
use serde_json::json;

use crate::Proxy;
use crate::ops::{SetChange, SettingsChange};
use crate::profile::{LOG_LEVELS, Routing};

const URL: Rule = Rule::Word { re: r"https?://[!-~]{1,2040}", max: 2048 };
const NAME: Rule = Rule::Text { max_bytes: 192, max_chars: 64 };
const NODE_ID: Rule = Rule::Word { re: r"s[0-9]{1,6}-[0-9]{1,6}|L[0-9]{1,6}", max: 16 };
const SUB_ID: Rule = Rule::Word { re: r"s[0-9]{1,6}", max: 8 };
const CONFIG_ID: Rule = Rule::Word { re: r"default|c[0-9]{1,6}", max: 8 };
/// A node name as groups name it.
const NODE_NAME: Rule = Rule::Text { max_bytes: 768, max_chars: 256 };
const LINK: Rule = Rule::Text { max_bytes: 16_384, max_chars: 16_384 };

const SET: &[Field] = &[
    Field::opt("enabled", Rule::Bool),
    Field::opt("routing", Rule::Enum(Routing::WORDS)),
    Field::opt("selected", NODE_ID),
    Field::opt("active_config", CONFIG_ID),
];
const SETTINGS_SET: &[Field] = &[
    Field::opt("intercept_router", Rule::Bool),
    Field::opt("bypass_macs", Rule::Word { re: r"[0-9A-Fa-f:, -]{0,1600}", max: 1600 }),
    Field::opt("probe_url", URL),
    Field::opt("subscription_refresh_hours", Rule::Int { min: 0, max: 168 }),
    Field::opt("rule_sets_url", URL),
    Field::opt("auto_failover", Rule::Bool),
    Field::opt("log_level", Rule::Enum(LOG_LEVELS)),
];
const SUB_ADD: &[Field] = &[Field::secret("url", URL, true), Field::opt("name", NAME)];
const SUB_UPDATE: &[Field] = &[Field::req("id", Rule::Word { re: r"all|s[0-9]{1,6}", max: 8 })];
const SUB_SET: &[Field] = &[Field::req("id", SUB_ID), Field::opt("name", NAME), Field::secret("url", URL, false)];
const SUB_ID_F: &[Field] = &[Field::req("id", SUB_ID)];
const NODE_INSPECT: &[Field] = &[Field::secret("text", Rule::Body { max_chars: 200_000 }, true)];
const NODE_SAVE: &[Field] = &[
    Field::opt("id", NODE_ID),
    Field::secret("link", LINK, true),
    Field::opt("name", NAME),
    Field::opt("via", Rule::Word { re: r"|s[0-9]{1,6}-[0-9]{1,6}|L[0-9]{1,6}", max: 16 }),
];
const NODE_DELETE: &[Field] = &[Field::req("id", Rule::Word { re: r"L[0-9]{1,6}", max: 8 })];
const PROBE: &[Field] = &[Field::req("id", NODE_ID), Field::opt("method", Rule::Enum(&["url", "tcp", "icmp"]))];
const GROUP_SELECT: &[Field] = &[Field::req("group", NODE_NAME), Field::req("node", NODE_NAME)];
const GROUP_TEST: &[Field] = &[Field::req("group", NODE_NAME)];
const CONFIG_SAVE: &[Field] = &[
    Field::opt("id", CONFIG_ID),
    Field::req("name", NAME),
    Field::req("text", Rule::Body { max_chars: 256_000 }),
    Field::opt("url", Rule::Word { re: r"|https?://[!-~]{1,2040}", max: 2048 }),
];
const CONFIG_DELETE: &[Field] = &[Field::req("id", CONFIG_ID)];
const CONFIG_FETCH: &[Field] = &[Field::req("url", URL)];
const RULES_CHECK: &[Field] = &[Field::req("text", Rule::Body { max_chars: 256_000 })];
const CONN_CLOSE: &[Field] = &[Field::req("id", Rule::Int { min: 0, max: 9_007_199_254_740_991 })];
const LOGS: &[Field] = &[
    Field::opt("epoch", Rule::Word { re: r"[0-9a-z]{0,32}", max: 32 }),
    Field::opt("after", Rule::Int { min: 0, max: 9_007_199_254_740_991 }),
];

/// One method whose handler gets the service and the checked request.
fn m<F, Fut, T>(
    b: ObjectBuilder,
    proxy: &Proxy,
    name: &'static str,
    access: Access,
    fields: &'static [Field],
    doc: &'static str,
    f: F,
) -> ObjectBuilder
where
    F: Fn(Proxy, Params) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T>> + Send + 'static,
    T: Serialize,
{
    let proxy = proxy.clone();
    b.method(name, access, fields, doc, move |p: Params| f(proxy.clone(), p))
}

fn s(p: &Params, k: &str) -> Option<String> {
    p.str(k).map(str::to_owned)
}

pub(crate) fn object(proxy: &Proxy) -> Object {
    use Access::{Read, Write};
    let b = Object::builder("f50.proxy");
    let b =
        m(b, proxy, "status", Read, &[], "The proxy's state, node, traffic counters and groups.", |x, _| async move {
            Ok(x.status().await)
        });
    let b = m(
        b,
        proxy,
        "profile",
        Write,
        &[],
        "The whole profile: subscriptions, nodes, configs and settings.",
        |x, _| async move { Ok(x.profile_view()) },
    );
    let b = m(
        b,
        proxy,
        "set",
        Write,
        SET,
        "Turns the proxy on or off, or changes its routing, node or config; applies at once.",
        |x, p| async move {
            let c = SetChange {
                enabled: p.bool("enabled"),
                routing: s(&p, "routing"),
                selected: s(&p, "selected"),
                active_config: s(&p, "active_config"),
            };
            x.set(c).await
        },
    );
    let b = m(
        b,
        proxy,
        "settings_set",
        Write,
        SETTINGS_SET,
        "Changes the proxy's settings; absent fields are kept.",
        |x, p| async move {
            let c = SettingsChange {
                intercept_router: p.bool("intercept_router"),
                bypass_macs: s(&p, "bypass_macs"),
                probe_url: s(&p, "probe_url"),
                subscription_refresh_hours: p.int("subscription_refresh_hours").and_then(|h| u32::try_from(h).ok()),
                rule_sets_url: s(&p, "rule_sets_url"),
                auto_failover: p.bool("auto_failover"),
                log_level: s(&p, "log_level"),
            };
            x.settings_set(c).await
        },
    );
    let b = m(
        b,
        proxy,
        "subscription_add",
        Write,
        SUB_ADD,
        "Adds a subscription and fetches its nodes as a job.",
        |x, p| async move {
            let r = x.subscription_add(
                p.need_str("url")?.to_owned(),
                s(&p, "name").map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()),
            );
            r.map(|job| json!({ "job": job }))
        },
    );
    let b = m(
        b,
        proxy,
        "subscription_update",
        Write,
        SUB_UPDATE,
        "Fetches one subscription or all again as a job.",
        |x, p| async move { x.subscription_update(p.need_str("id")?).map(|job| json!({ "job": job })) },
    );
    let b = m(
        b,
        proxy,
        "subscription_set",
        Write,
        SUB_SET,
        "Renames a subscription or changes its address.",
        |x, p| async move { x.subscription_set(p.need_str("id")?, p.str("name"), p.str("url")).await },
    );
    let b = m(
        b,
        proxy,
        "subscription_remove",
        Write,
        SUB_ID_F,
        "Removes a subscription and its nodes.",
        |x, p| async move { x.subscription_remove(p.need_str("id")?).await },
    );
    let b = m(
        b,
        proxy,
        "node_inspect",
        Write,
        NODE_INSPECT,
        "Reads share links without saving them: each node with its problem.",
        |x, p| async move { x.inspect(p.need_str("text")?).await.map(|nodes| json!({ "nodes": nodes })) },
    );
    let b = m(
        b,
        proxy,
        "node_save",
        Write,
        NODE_SAVE,
        "Adds a local node (no id) or replaces a node from its share link.",
        |x, p| async move { x.node_save(s(&p, "id"), p.need_str("link")?, p.str("name"), p.str("via")).await },
    );
    let b = m(b, proxy, "node_delete", Write, NODE_DELETE, "Deletes a local node.", |x, p| async move {
        x.node_delete(p.need_str("id")?).await
    });
    let b = m(
        b,
        proxy,
        "probe",
        Write,
        PROBE,
        "Times one request through a node (with its front nodes).",
        |x, p| async move { x.probe(p.need_str("id")?, p.str("method").unwrap_or("url")).await },
    );
    let b = m(
        b,
        proxy,
        "group_select",
        Write,
        GROUP_SELECT,
        "Points a select group at a node and remembers the pick.",
        |x, p| async move { x.group_select(p.need_str("group")?, p.need_str("node")?).await },
    );
    let b = m(
        b,
        proxy,
        "group_test",
        Write,
        GROUP_TEST,
        "Tests the delay of a group's nodes: the groups, or `{job}` for a large group while the proxy is off.",
        |x, p| async move { x.group_test(p.need_str("group")?).await },
    );
    let b = m(
        b,
        proxy,
        "config_save",
        Write,
        CONFIG_SAVE,
        "Saves a rule config (no id: a new one) once its rules check.",
        |x, p| async move { x.config_save(s(&p, "id"), p.need_str("name")?, p.need_str("text")?, p.str("url")).await },
    );
    let b = m(
        b,
        proxy,
        "config_delete",
        Write,
        CONFIG_DELETE,
        "Deletes a rule config; the active one falls back to the default.",
        |x, p| async move { x.config_delete(p.need_str("id")?).await },
    );
    let b = m(
        b,
        proxy,
        "config_fetch",
        Write,
        CONFIG_FETCH,
        "Downloads a rule config's text from its link.",
        |x, p| async move { x.config_fetch(p.need_str("url")?).await },
    );
    let b = m(
        b,
        proxy,
        "rules_check",
        Write,
        RULES_CHECK,
        "Checks a route document: counts, groups and warnings, or the first error.",
        |x, p| async move { x.rules_check(p.need_str("text")?).await },
    );
    let b = m(
        b,
        proxy,
        "connections",
        Write,
        &[],
        "The engine's open and recently closed connections.",
        |x, _| async move { x.connections().await },
    );
    let b = m(
        b,
        proxy,
        "connections_close",
        Write,
        CONN_CLOSE,
        "Closes one connection, or all (id 0).",
        |x, p| async move { x.connections_close(p.need_int("id")?).await },
    );
    let b = m(b, proxy, "logs", Write, LOGS, "The engine's log lines after a sequence number.", |x, p| async move {
        x.logs(p.str("epoch"), p.int("after")).await
    });
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::rpc::validate;

    #[test]
    fn fields_refuse_what_they_must() {
        assert!(
            validate(SET, &json!({"enabled": true, "routing": "rule", "selected": "s1-3", "active_config": "c2"}))
                .is_ok()
        );
        assert!(validate(SET, &json!({"selected": "L1"})).is_ok());
        for bad in [json!({"routing": "smart"}), json!({"selected": "x"}), json!({"active_config": "c"})] {
            assert!(validate(SET, &bad).is_err(), "{bad}");
        }
        assert!(validate(SETTINGS_SET, &json!({"bypass_macs": "", "subscription_refresh_hours": 0})).is_ok());
        assert!(validate(SETTINGS_SET, &json!({"bypass_macs": "aa:bb:cc:dd:ee:ff, 11-22-33-44-55-66"})).is_ok());
        assert!(validate(SETTINGS_SET, &json!({"subscription_refresh_hours": 169})).is_err());
        assert!(validate(SETTINGS_SET, &json!({"log_level": "trace"})).is_err());
        assert!(validate(SETTINGS_SET, &json!({"probe_url": "ftp://x"})).is_err());
        assert!(validate(SUB_ADD, &json!({"url": "https://sub.example.com/s?token=a&b=1", "name": "机场"})).is_ok());
        assert!(validate(SUB_ADD, &json!({"url": "https://x y"})).is_err());
        assert!(validate(SUB_UPDATE, &json!({"id": "all"})).is_ok());
        assert!(validate(NODE_SAVE, &json!({"link": "vless://u@h:443?x=1#香港", "via": ""})).is_ok());
        assert!(validate(NODE_SAVE, &json!({"link": "a\nb"})).is_err());
        assert!(validate(NODE_DELETE, &json!({"id": "s1-1"})).is_err());
        assert!(validate(NODE_INSPECT, &json!({"text": "ss://a\nvmess://b"})).is_ok());
        assert!(validate(CONFIG_SAVE, &json!({"name": "mine", "text": "{\n}", "url": ""})).is_ok());
        assert!(validate(CONN_CLOSE, &json!({"id": 0})).is_ok());
        assert!(validate(LOGS, &json!({"epoch": "abc123", "after": 5})).is_ok());
        assert!(validate(LOGS, &json!({"epoch": "a&b"})).is_err());
    }
}

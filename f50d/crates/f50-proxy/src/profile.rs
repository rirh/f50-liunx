//! The profile: everything the proxy remembers (`<state>/proxy/profile.json`, 0600: the links are credentials).
//!
//! The shape is the contract's (`f50-proxy/DESIGN.md` §2 "Profile") and is what `f50.proxy profile` returns whole.
//! The built-in config (`default`, one-node's `pc/src-tauri/src/configs.rs`) is `configs[0]` in memory and in every
//! reply, and is never written: a new f50d brings its new text. A profile that cannot be read is kept aside
//! (`profile.json.bad`) rather than overwritten by the defaults at the next save.

use std::collections::BTreeMap;
use std::path::Path;

use f50_core::{Error, Result, fsx};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const BUILTIN_ID: &str = "default";
pub(crate) const BUILTIN_NAME: &str = "默认规则";
/// one-node's 默认规则: the AI services through the node; the engine's own tail sends LAN and mainland sites
/// direct, other foreign sites through the node, and the rest to `final`.
pub(crate) const BUILTIN_TEXT: &str = r#"{
  "rules": [
    {"rule_set": ["geosite-anthropic"], "outbound": "proxy"},
    {"rule_set": ["geosite-openai"], "outbound": "proxy"},
    {"rule_set": ["geosite-google-gemini"], "outbound": "proxy"}
  ],
  "final": "proxy"
}
"#;

/// Plain HTTP: with the warm-up request of every delay test (f50-proxy `probe.HTTPDelay`) the timed request is one
/// round trip either way, and this matches what Shadowrocket-style clients show.
pub(crate) const DEFAULT_PROBE_URL: &str = "http://www.gstatic.com/generate_204";
/// The default before it, moved to the current one when a saved profile still holds it.
const OLD_DEFAULT_PROBE_URL: &str = "https://www.gstatic.com/generate_204";
pub(crate) const DEFAULT_RULE_SETS: &str = "https://marseo.eu.org/api/v1/one/rules/";

/// 智能分流 | 全局代理 | 全部直连.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Routing {
    #[default]
    Rule,
    Global,
    Direct,
}

impl Routing {
    pub const WORDS: &'static [&'static str] = &["rule", "global", "direct"];

    pub fn as_str(self) -> &'static str {
        match self {
            Routing::Rule => "rule",
            Routing::Global => "global",
            Routing::Direct => "direct",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rule" => Some(Routing::Rule),
            "global" => Some(Routing::Global),
            "direct" => Some(Routing::Direct),
            _ => None,
        }
    }
}

/// One node: the engine's `Node` plus our `id` and `via` (前置代理, a node id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Node {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub protocol: String,
    pub server: String,
    pub port: u32,
    pub link: String,
    pub summary: String,
    pub problem: Option<String>,
    pub warnings: Vec<String>,
    pub via: Option<String>,
    /// 本地 SOCKS5 (one-node's `local_socks`): the node's own SOCKS5 port on the router's LAN, while set.
    pub local_socks: Option<LocalSocks>,
}

/// A node's SOCKS5 port: what reaches it always leaves through that node (with its front nodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct LocalSocks {
    pub port: u16,
}

impl Node {
    /// A node as the engine parsed it (`/v1/subscription`): its `id` is the engine's list position and is replaced.
    pub fn from_engine(v: &Value) -> Option<Node> {
        let mut n: Node = serde_json::from_value(v.clone()).ok()?;
        n.problem = n.problem.filter(|p| !p.is_empty());
        n.via = None;
        n.local_socks = None;
        (!n.link.is_empty()).then_some(n)
    }

    pub fn usable(&self) -> bool {
        self.problem.is_none()
    }
}

/// The de-facto `Subscription-Userinfo`: bytes, and the expiry in Unix seconds (0: never).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Usage {
    pub upload: i64,
    pub download: i64,
    pub total: i64,
    pub expire: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub title: Option<String>,
    pub usage: Option<Usage>,
    pub updated_at: Option<u64>,
    pub error: Option<String>,
    pub nodes: Vec<Node>,
}

/// A rule config (配置): the route document (`{groups, apps, rules, rule_set, final}`) as typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Config {
    pub id: String,
    pub name: String,
    pub text: String,
    pub url: Option<String>,
    pub updated_at: Option<u64>,
    pub builtin: bool,
    /// The engine's last `/v1/rules/check` of `text`: `{final, groups, source, rules, warnings}`.
    pub info: Option<Value>,
}

impl Config {
    pub fn builtin() -> Config {
        Config {
            id: BUILTIN_ID.into(),
            name: BUILTIN_NAME.into(),
            text: BUILTIN_TEXT.into(),
            url: None,
            updated_at: None,
            builtin: true,
            info: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// The router's own traffic goes through the proxy too.
    pub intercept_router: bool,
    pub bypass_macs: Vec<String>,
    pub probe_url: String,
    /// 0: never.
    pub subscription_refresh_hours: u32,
    pub rule_sets_url: String,
    pub auto_failover: bool,
    /// error | warning | info | debug
    pub log_level: String,
}

pub(crate) const LOG_LEVELS: &[&str] = &["error", "warning", "info", "debug"];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            intercept_router: true,
            bypass_macs: Vec::new(),
            probe_url: DEFAULT_PROBE_URL.into(),
            subscription_refresh_hours: 12,
            rule_sets_url: DEFAULT_RULE_SETS.into(),
            auto_failover: true,
            log_level: "warning".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Profile {
    pub enabled: bool,
    pub routing: Routing,
    pub selected: Option<String>,
    pub subscriptions: Vec<Subscription>,
    pub local_nodes: Vec<Node>,
    pub configs: Vec<Config>,
    pub active_config: String,
    /// The saved pick of each select group, per config id.
    pub group_selections: BTreeMap<String, BTreeMap<String, String>>,
    pub settings: Settings,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            enabled: false,
            routing: Routing::Rule,
            selected: None,
            subscriptions: Vec::new(),
            local_nodes: Vec::new(),
            configs: vec![Config::builtin()],
            active_config: BUILTIN_ID.into(),
            group_selections: BTreeMap::new(),
            settings: Settings::default(),
        }
    }
}

impl Profile {
    /// Read from its JSON: the built-in config put first (fresh), ids that point nowhere let go.
    pub fn parse(text: &str) -> Result<Profile> {
        let mut p: Profile =
            serde_json::from_str(text).map_err(|e| Error::failed(format!("the proxy profile is unreadable: {e}")))?;
        p.normalise();
        Ok(p)
    }

    /// The built-in config first and current; an active config or a selection that is gone falls back.
    pub fn normalise(&mut self) {
        let info =
            self.configs.iter().find(|c| c.id == BUILTIN_ID && c.text == BUILTIN_TEXT).and_then(|c| c.info.clone());
        self.configs.retain(|c| c.id != BUILTIN_ID && !c.builtin);
        self.configs.insert(0, Config { info, ..Config::builtin() });
        if !self.configs.iter().any(|c| c.id == self.active_config) {
            self.active_config = BUILTIN_ID.into();
        }
        self.group_selections.retain(|id, _| self.configs.iter().any(|c| &c.id == id));
        if self.selected.as_deref().is_some_and(|id| self.node(id).is_none()) {
            self.selected = None;
        }
        if !LOG_LEVELS.contains(&self.settings.log_level.as_str()) {
            self.settings.log_level = "warning".into();
        }
        if self.settings.probe_url == OLD_DEFAULT_PROBE_URL {
            self.settings.probe_url = DEFAULT_PROBE_URL.into();
        }
        crate::nodes::unique_names(self);
    }

    /// What is written: everything but the built-in config.
    pub fn to_stored(&self) -> Result<Vec<u8>> {
        let mut p = self.clone();
        p.configs.retain(|c| !c.builtin);
        serde_json::to_vec_pretty(&p).map_err(|e| Error::failed(format!("profile not serialisable: {e}")))
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.subscriptions.iter().flat_map(|s| s.nodes.iter()).chain(self.local_nodes.iter())
    }

    pub fn nodes_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.subscriptions.iter_mut().flat_map(|s| s.nodes.iter_mut()).chain(self.local_nodes.iter_mut())
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes().find(|n| n.id == id)
    }

    pub fn config(&self, id: &str) -> Option<&Config> {
        self.configs.iter().find(|c| c.id == id)
    }

    #[allow(clippy::redundant_closure)] // the closure shortens the static borrow to self's
    pub fn active(&self) -> &Config {
        // normalise keeps the active id pointing at a config; the built-in one is always there
        self.config(&self.active_config).or(self.configs.first()).unwrap_or_else(|| builtin_ref())
    }

    /// The first usable node, subscriptions first.
    pub fn first_usable(&self) -> Option<String> {
        self.nodes().find(|n| n.usable()).map(|n| n.id.clone())
    }

    /// The saved select-group picks of the config in use.
    pub fn selections(&self) -> BTreeMap<String, String> {
        self.group_selections.get(&self.active_config).cloned().unwrap_or_default()
    }
}

fn builtin_ref() -> &'static Config {
    static B: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    B.get_or_init(Config::builtin)
}

/// The next free `<prefix><n>` among `ids` (one past the highest).
pub(crate) fn next_id<'a>(prefix: &str, ids: impl Iterator<Item = &'a str>) -> String {
    let n = ids.filter_map(|id| id.strip_prefix(prefix)?.parse::<u32>().ok()).max().unwrap_or(0);
    format!("{prefix}{}", n + 1)
}

/// The profile in `path`; the defaults when there is none. An unreadable one is moved aside first.
pub(crate) fn load(path: &Path) -> Profile {
    match fsx::read_opt(path) {
        Ok(None) => Profile::default(),
        Ok(Some(text)) => Profile::parse(&text).unwrap_or_else(|e| {
            let aside = path.with_extension("json.bad");
            tracing::warn!(error = %e, aside = %aside.display(), "the proxy profile is unreadable: kept aside");
            let _ = std::fs::rename(path, &aside);
            Profile::default()
        }),
        Err(e) => {
            tracing::warn!(error = %e, "the proxy profile could not be read; starting from the defaults");
            Profile::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn old_default_probe_url_moves_to_the_current_one() {
        let old =
            Profile::parse(r#"{"settings":{"probe_url":"https://www.gstatic.com/generate_204"}}"#).expect("parse");
        assert_eq!(old.settings.probe_url, DEFAULT_PROBE_URL);
        let own = Profile::parse(r#"{"settings":{"probe_url":"https://cp.cloudflare.com/"}}"#).expect("parse");
        assert_eq!(own.settings.probe_url, "https://cp.cloudflare.com/", "a URL the user chose stays");
    }

    #[test]
    fn defaults_and_round_trip() {
        let p = Profile::default();
        assert_eq!(p.configs[0].id, "default");
        assert!(p.configs[0].builtin);
        let v = serde_json::to_value(&p).expect("json");
        assert_eq!(v["routing"], json!("rule"));
        assert_eq!(v["selected"], Value::Null);
        assert_eq!(v["settings"]["subscription_refresh_hours"], json!(12));
        assert_eq!(v["settings"]["rule_sets_url"], json!(DEFAULT_RULE_SETS));
        assert_eq!(v["configs"][0]["url"], Value::Null);

        let stored = String::from_utf8(p.to_stored().expect("stored")).expect("utf8");
        let back: Value = serde_json::from_str(&stored).expect("json");
        assert_eq!(back["configs"], json!([]), "the built-in config is never written");
        let again = Profile::parse(&stored).expect("parse");
        assert_eq!(again, p);
    }

    #[test]
    fn parse_tolerates_partial_and_repairs() {
        let text = json!({
            "enabled": true,
            "routing": "global",
            "selected": "s1-9",
            "active_config": "c7",
            "subscriptions": [{"id": "s1", "name": "机场", "url": "https://x/sub", "nodes": [
                {"id": "s1-1", "name": "HK", "link": "vless://a", "port": 443},
                {"id": "s1-2", "name": "HK", "link": "vless://b", "problem": "bad"}
            ]}],
            "configs": [{"id": "default", "text": "old"}, {"id": "c1", "name": "mine", "text": "{}"}],
            "group_selections": {"c1": {"g": "HK"}, "gone": {"g": "x"}},
            "settings": {"log_level": "loud"}
        })
        .to_string();
        let p = Profile::parse(&text).expect("parse");
        assert!(p.enabled);
        assert_eq!(p.routing, Routing::Global);
        assert_eq!(p.selected, None, "a selection that points nowhere goes");
        assert_eq!(p.active_config, "default");
        assert_eq!(p.configs.len(), 2);
        assert_eq!(p.configs[0].text, BUILTIN_TEXT, "the built-in text is the daemon's");
        assert!(p.group_selections.contains_key("c1") && !p.group_selections.contains_key("gone"));
        assert_eq!(p.settings.log_level, "warning");
        assert!(p.settings.auto_failover, "absent settings take their defaults");
        let names: Vec<_> = p.nodes().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["HK", "HK (2)"]);
        assert_eq!(p.first_usable().as_deref(), Some("s1-1"));
        assert!(Profile::parse("{nope").is_err());
    }

    #[test]
    fn ids_count_up() {
        assert_eq!(next_id("s", ["s1", "s3", "x9"].into_iter()), "s4");
        assert_eq!(next_id("L", std::iter::empty()), "L1");
    }

    #[test]
    fn engine_nodes_lose_their_position_id() {
        let v = json!({"id": "0", "name": "a", "kind": "reality", "protocol": "vless", "server": "h", "port": 443,
            "link": "vless://x", "summary": "VLESS", "problem": "", "warnings": ["w"]});
        let n = Node::from_engine(&v).expect("node");
        assert_eq!(n.problem, None);
        assert_eq!(n.warnings, ["w"]);
        assert!(Node::from_engine(&json!({"name": "no link"})).is_none());
    }

    #[test]
    fn unreadable_profile_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("f50-proxy-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("profile.json");
        std::fs::write(&path, "{broken").expect("write");
        let p = load(&path);
        assert_eq!(p, Profile::default());
        assert!(dir.join("profile.json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

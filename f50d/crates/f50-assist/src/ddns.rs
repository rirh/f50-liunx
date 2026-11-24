//! DDNS on Cloudflare: each record is kept at an address of the router (its cellular IPv6 or IPv4) or of a LAN
//! device: its IPv6 suffix (the last 64 bits, `::1cf3:85ff:c719:c51c`) on the LAN's current prefix (br-lan's public
//! /64), so a new prefix from the carrier moves the record with it. A device's address cannot be found reliably by
//! the router (no traffic, no neighbour entry; macOS ignores multicast echo), hence the suffix; `neighbors` lists the
//! addresses the router has seen per device, to pick one from.
//!
//! - Checked every `interval_minutes` (default 5) and at once after a change; Cloudflare is asked only when the
//!   address changed or the last update is a day old. The zone id is looked up from the zone's name.
//! - A private or CGNAT IPv4 (the cellular network's usual 10.x) is not published: nothing outside could reach it.
//! - The token is never returned (`token_set`, its last four characters).
//!
//! Publishing an address opens nothing: fw4 still rejects input from the WAN and forwarding into the LAN.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::time::unix_now;
use f50_core::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{Inner, curl, store};

const FILE: &str = "ddns.json";
const API: &str = "https://api.cloudflare.com/client/v4";
/// An unchanged address is written again after this long anyway.
const REFRESH: u64 = 86_400;
const LAN_BRIDGE: &str = "br-lan";

static WAKE: std::sync::LazyLock<Notify> = std::sync::LazyLock::new(Notify::new);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Record {
    /// The full name (`nas.example.com`).
    pub name: String,
    /// `AAAA` or `A`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `router`, or a LAN device's IPv6 suffix (`::1cf3:85ff:c719:c51c`, AAAA only).
    pub target: String,
}

impl Default for Record {
    fn default() -> Self {
        Record { name: String::new(), kind: "AAAA".into(), target: "router".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub token: String,
    /// The zone's name (`example.com`).
    pub zone: String,
    pub records: Vec<Record>,
    pub interval_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: false, token: String::new(), zone: String::new(), records: Vec::new(), interval_minutes: 5 }
    }
}

/// How a record stands.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Status {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ip: Option<String>,
    pub updated_at: Option<u64>,
    pub checked_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct State {
    pub settings: Settings,
    pub zone_id: Option<String>,
    pub status: Vec<Status>,
}

impl State {
    pub fn load(dir: &std::path::Path) -> State {
        store::load(dir, FILE)
    }

    pub fn view(&self) -> Value {
        let t = &self.settings.token;
        json!({
            "enabled": self.settings.enabled,
            "provider": "cloudflare",
            "token_set": !t.is_empty(),
            "token_hint": (t.len() > 4).then(|| t[t.len() - 4..].to_owned()),
            "zone": self.settings.zone,
            "records": self.settings.records,
            "interval_minutes": self.settings.interval_minutes,
            "status": self.status,
        })
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || (o[0] == 100 && (64..128).contains(&o[1])))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments()[0];
    (0x2000..0x4000).contains(&s)
}

/// The router's addresses on the cellular data (`f50.data get`'s `addresses`, `ip/len`).
fn router_addr(addresses: &[String], kind: &str) -> Result<String> {
    let ips: Vec<IpAddr> = addresses.iter().filter_map(|a| a.split('/').next().and_then(|s| s.parse().ok())).collect();
    if kind == "A" {
        let v4 = ips.iter().find_map(|i| if let IpAddr::V4(v) = i { Some(*v) } else { None });
        return match v4 {
            Some(v) if public_v4(v) => Ok(v.to_string()),
            Some(v) => Err(Error::failed(format!("运营商分配的是内网地址 {v}（CGNAT），外网无法访问；请用 AAAA"))),
            None => Err(Error::failed("移动数据没有 IPv4 地址")),
        };
    }
    ips.iter()
        .find_map(|i| if let IpAddr::V6(v) = i { public_v6(*v).then(|| v.to_string()) } else { None })
        .ok_or_else(|| Error::failed("移动数据没有公网 IPv6 地址（APN 的 IP 类型需为 IPv4v6）"))
}

/// An IPv6 suffix: an address whose first 64 bits are zero (`::1cf3:85ff:c719:c51c`).
pub(crate) fn parse_suffix(s: &str) -> Option<Ipv6Addr> {
    let ip: Ipv6Addr = s.parse().ok()?;
    let seg = ip.segments();
    (seg[..4] == [0, 0, 0, 0] && seg[4..] != [0, 0, 0, 0]).then_some(ip)
}

/// The LAN's public /64 from `ip -6 addr show dev br-lan`, with `suffix` as its last 64 bits.
fn on_lan_prefix(addr_show: &str, suffix: Ipv6Addr) -> Option<String> {
    let prefix = addr_show.lines().find_map(|l| {
        let a = l.trim().strip_prefix("inet6 ")?.split_whitespace().next()?;
        let ip: Ipv6Addr = a.split('/').next()?.parse().ok()?;
        public_v6(ip).then_some(ip)
    })?;
    let (p, x) = (prefix.segments(), suffix.segments());
    Some(Ipv6Addr::new(p[0], p[1], p[2], p[3], x[4], x[5], x[6], x[7]).to_string())
}

/// `ip -6 neigh show dev br-lan`: each MAC's public addresses.
fn neighbours(neigh: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in neigh.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let Some(Ok(ip)) = w.first().map(|a| a.parse::<Ipv6Addr>()) else { continue };
        let Some(mac) = w.iter().position(|x| *x == "lladdr").and_then(|i| w.get(i + 1)) else { continue };
        if !public_v6(ip) {
            continue;
        }
        let mac = mac.to_ascii_lowercase();
        match out.iter_mut().find(|(m, _)| *m == mac) {
            Some((_, list)) => list.push(ip.to_string()),
            None => out.push((mac, vec![ip.to_string()])),
        }
    }
    out
}

/// The LAN's devices (DHCP leases) with the public IPv6 addresses the router has seen for each: what a suffix is
/// picked from.
pub(crate) async fn neighbors(inner: &Inner) -> Result<Value> {
    let neigh =
        Cmd::new("ip").args(["-6", "neigh", "show", "dev", LAN_BRIDGE]).timeout(Duration::from_secs(5)).ok().await?;
    let seen = neighbours(&neigh);
    let leases = inner.call("f50.system", "clients", json!({})).await.unwrap_or(Value::Null);
    let mut out: Vec<Value> = Vec::new();
    for l in leases["leases"].as_array().into_iter().flatten() {
        let mac = l["mac"].as_str().unwrap_or_default().to_ascii_lowercase();
        let addrs = seen.iter().find(|(m, _)| *m == mac).map(|(_, a)| a.clone()).unwrap_or_default();
        out.push(json!({ "mac": mac, "host": l["host"], "ip": l["ip"], "ipv6": addrs }));
    }
    for (mac, addrs) in &seen {
        if !out.iter().any(|d| d["mac"] == mac.as_str()) {
            out.push(json!({ "mac": mac, "host": "", "ip": "", "ipv6": addrs }));
        }
    }
    Ok(json!({ "devices": out }))
}

async fn address(inner: &Inner, r: &Record) -> Result<String> {
    if r.target == "router" {
        let d = inner.call("f50.data", "get", json!({})).await?;
        let addrs: Vec<String> =
            d["addresses"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(str::to_owned)).collect();
        return router_addr(&addrs, &r.kind);
    }
    let suffix = parse_suffix(&r.target).ok_or_else(|| Error::failed(format!("「{}」不是 IPv6 后缀", r.target)))?;
    let out = Cmd::new("ip")
        .args(["-6", "addr", "show", "dev", LAN_BRIDGE, "scope", "global"])
        .timeout(Duration::from_secs(5))
        .ok()
        .await?;
    on_lan_prefix(&out, suffix).ok_or_else(|| Error::failed("局域网没有公网 IPv6 前缀"))
}

fn cf_error(code: u16, v: &Value) -> Error {
    let m = v["errors"][0]["message"].as_str().map_or_else(|| format!("HTTP {code}"), str::to_owned);
    Error::failed(format!("Cloudflare: {m}"))
}

async fn cf(token: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let (code, v) =
        curl::json(method, &format!("{API}{path}"), &[format!("Authorization: Bearer {token}")], body).await?;
    if code / 100 != 2 || v["success"] != true {
        return Err(cf_error(code, &v));
    }
    Ok(v["result"].clone())
}

async fn zone_id(token: &str, zone: &str) -> Result<String> {
    let r = cf(token, "GET", &format!("/zones?name={zone}"), None).await?;
    r[0]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::failed(format!("Cloudflare 上没有域名 {zone}（或令牌无权访问）")))
}

/// Puts `name` (`kind`) at `ip`: updated, created, or left when it is already there.
async fn upsert(token: &str, zid: &str, r: &Record, ip: &str) -> Result<()> {
    let found = cf(token, "GET", &format!("/zones/{zid}/dns_records?type={}&name={}", r.kind, r.name), None).await?;
    let body = json!({ "type": r.kind, "name": r.name, "content": ip, "ttl": 60, "proxied": false });
    match found.as_array().and_then(|a| a.first()) {
        Some(rec) if rec["content"] == ip => Ok(()),
        Some(rec) => {
            let id = rec["id"].as_str().unwrap_or_default();
            cf(token, "PUT", &format!("/zones/{zid}/dns_records/{id}"), Some(&body)).await.map(|_| ())
        }
        None => cf(token, "POST", &format!("/zones/{zid}/dns_records"), Some(&body)).await.map(|_| ()),
    }
}

/// One round over every record; `force`: Cloudflare is asked even for an unchanged address.
pub(crate) async fn sync(inner: &Inner, force: bool) {
    let st = Inner::lock(&inner.ddns).clone();
    let s = &st.settings;
    if !s.enabled || s.token.is_empty() || s.zone.is_empty() {
        return;
    }
    let now = unix_now();
    let mut zid = st.zone_id.clone();
    let mut status: Vec<Status> = Vec::new();
    for r in &s.records {
        let before = st.status.iter().find(|x| x.name == r.name && x.kind == r.kind).cloned().unwrap_or_default();
        let mut out = Status { name: r.name.clone(), kind: r.kind.clone(), checked_at: Some(now), ..before.clone() };
        let r2 = async {
            let ip = address(inner, r).await?;
            let fresh = before.ip.as_deref() == Some(ip.as_str())
                && before.error.is_none()
                && before.updated_at.is_some_and(|t| now.saturating_sub(t) < REFRESH);
            if !fresh || force {
                let id = match &zid {
                    Some(z) => z.clone(),
                    None => {
                        let z = zone_id(&s.token, &s.zone).await?;
                        zid = Some(z.clone());
                        z
                    }
                };
                upsert(&s.token, &id, r, &ip).await?;
                tracing::info!(name = %r.name, %ip, "DDNS record updated");
                out.updated_at = Some(now);
            }
            Ok::<_, Error>(ip)
        }
        .await;
        match r2 {
            Ok(ip) => {
                out.ip = Some(ip);
                out.error = None;
            }
            Err(e) => {
                tracing::warn!(name = %r.name, error = %e, "DDNS record not updated");
                out.error = Some(e.message().to_owned());
            }
        }
        status.push(out);
    }
    let snapshot = {
        let mut cur = Inner::lock(&inner.ddns);
        // settings changed meanwhile: this round's results are for the old ones
        if cur.settings.records != st.settings.records || cur.settings.zone != st.settings.zone {
            return;
        }
        cur.status = status;
        cur.zone_id = zid;
        cur.clone()
    };
    let _ = store::save(&inner.dir, FILE, &snapshot).await;
}

pub(crate) async fn run(inner: Arc<Inner>, shutdown: CancellationToken) {
    tokio::select! {
        () = shutdown.cancelled() => return,
        () = tokio::time::sleep(Duration::from_secs(60)) => {}
    }
    loop {
        sync(&inner, false).await;
        let mins = u64::from(Inner::lock(&inner.ddns).settings.interval_minutes.max(1));
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(Duration::from_secs(mins * 60)) => {}
            () = WAKE.notified() => {}
        }
    }
}

const NAME_RE: &str = "([a-z0-9*]([a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z]{2,63}";

pub(crate) struct Change<'a> {
    pub enabled: Option<bool>,
    pub token: Option<&'a str>,
    pub zone: Option<&'a str>,
    pub records: Option<&'a str>,
    pub interval_minutes: Option<u32>,
}

pub(crate) async fn set(inner: &Inner, c: Change<'_>) -> Result<Value> {
    let mut next = Inner::lock(&inner.ddns).clone();
    if let Some(t) = c.token {
        next.settings.token = t.trim().to_owned();
    }
    if let Some(z) = c.zone {
        let z = z.trim().trim_end_matches('.').to_ascii_lowercase();
        if !z.is_empty() && !f50_core::rpc::matches(NAME_RE, &z) {
            return Err(Error::bad_value("zone"));
        }
        if z != next.settings.zone {
            next.zone_id = None;
        }
        next.settings.zone = z;
    }
    if let Some(text) = c.records {
        let list: Vec<Record> = serde_json::from_str(text).map_err(|_| Error::bad_value("records"))?;
        let mut clean: Vec<Record> = Vec::new();
        for mut r in list {
            r.name = r.name.trim().trim_end_matches('.').to_ascii_lowercase();
            r.target = r.target.trim().to_ascii_lowercase();
            if r.target != "router" {
                // a whole address is taken for its suffix
                let ip: Option<Ipv6Addr> = r.target.parse().ok();
                if let Some(ip) = ip.filter(|ip| ip.segments()[..4] != [0, 0, 0, 0]) {
                    let x = ip.segments();
                    r.target = Ipv6Addr::new(0, 0, 0, 0, x[4], x[5], x[6], x[7]).to_string();
                }
            }
            if !f50_core::rpc::matches(NAME_RE, &r.name) {
                return Err(Error::field(Code::BadValue, "records", format!("「{}」不是域名", r.name)));
            }
            if !["AAAA", "A"].contains(&r.kind.as_str()) {
                return Err(Error::field(Code::BadValue, "records", "类型只能是 AAAA 或 A"));
            }
            if r.target != "router" && !(parse_suffix(&r.target).is_some() && r.kind == "AAAA") {
                return Err(Error::field(
                    Code::BadValue,
                    "records",
                    "局域网设备填 IPv6 后缀（如 ::1cf3:85ff:c719:c51c），且只能用 AAAA",
                ));
            }
            if !next.settings.zone.is_empty()
                && !(r.name == next.settings.zone || r.name.ends_with(&format!(".{}", next.settings.zone)))
            {
                return Err(Error::field(
                    Code::BadValue,
                    "records",
                    format!("「{}」不在 {} 下", r.name, next.settings.zone),
                ));
            }
            if !clean.iter().any(|x| x.name == r.name && x.kind == r.kind) {
                clean.push(r);
            }
        }
        if clean.len() > 16 {
            return Err(Error::field(Code::BadValue, "records", "最多 16 条记录"));
        }
        next.settings.records = clean;
    }
    if let Some(m) = c.interval_minutes {
        next.settings.interval_minutes = m;
    }
    if let Some(e) = c.enabled {
        next.settings.enabled = e;
    }
    if next.settings.enabled && (next.settings.token.is_empty() || next.settings.zone.is_empty()) {
        return Err(Error::field(Code::Missing, "token", "开启前先填写 API 令牌和域名"));
    }
    next.status.retain(|s| next.settings.records.iter().any(|r| r.name == s.name && r.kind == s.kind));
    store::save(&inner.dir, FILE, &next).await?;
    let view = next.view();
    *Inner::lock(&inner.ddns) = next;
    WAKE.notify_one();
    Ok(view)
}

/// Every record now, Cloudflare asked even for unchanged addresses.
pub(crate) async fn update_now(inner: &Inner) -> Result<Value> {
    if !Inner::lock(&inner.ddns).settings.enabled {
        return Err(Error::failed("DDNS 没有开启"));
    }
    sync(inner, true).await;
    Ok(Inner::lock(&inner.ddns).view())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_addresses() {
        let a = vec!["10.145.39.219/8".to_owned(), "2409:8900:2dc6:92b8:18e9:4feb:b74d:d16e/64".to_owned()];
        assert_eq!(router_addr(&a, "AAAA").ok().as_deref(), Some("2409:8900:2dc6:92b8:18e9:4feb:b74d:d16e"));
        let e = router_addr(&a, "A").err().map(|e| e.message().to_owned()).unwrap_or_default();
        assert!(e.contains("CGNAT"), "{e}");
        assert_eq!(router_addr(&["1.2.3.4/32".to_owned()], "A").ok().as_deref(), Some("1.2.3.4"));
        assert!(router_addr(&["100.70.1.2/32".to_owned()], "A").is_err());
    }

    #[test]
    fn suffix_on_the_lan_prefix() {
        let show = "\
6: br-lan: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500
    inet6 fd2f:7df0:29fb::1/64 scope global noprefixroute
    inet6 2409:8900:2dc6:92b8::1/64 scope global noprefixroute";
        let sfx = parse_suffix("::1cf3:85ff:c719:c51c").expect("suffix");
        assert_eq!(on_lan_prefix(show, sfx).as_deref(), Some("2409:8900:2dc6:92b8:1cf3:85ff:c719:c51c"));
        assert!(parse_suffix("2409::1").is_none(), "a whole address is not a suffix");
        assert!(parse_suffix("::").is_none());
        assert!(on_lan_prefix("    inet6 fd2f::1/64 scope global", sfx).is_none());
    }

    #[test]
    fn neighbours_by_mac() {
        let neigh = "\
fe80::c8f:a69b:44b0:58ff lladdr 60:3e:5f:3e:09:b2 STALE
2409:8900:2dc6:92b8:1cf3:85ff:c719:c51c lladdr 60:3e:5f:3e:09:b2 REACHABLE
2409:8900:2dc6:92b8::9 lladdr DE:A9:20:CB:D8:F5 STALE
2409:8900:2dc6:92b8::10 FAILED";
        let n = neighbours(neigh);
        assert_eq!(n.len(), 2);
        assert_eq!(n[0], ("60:3e:5f:3e:09:b2".to_owned(), vec!["2409:8900:2dc6:92b8:1cf3:85ff:c719:c51c".to_owned()]));
        assert_eq!(n[1].0, "de:a9:20:cb:d8:f5");
    }

    #[test]
    fn token_is_hidden() {
        let st =
            State { settings: Settings { token: "abcdefgh1234".into(), ..Settings::default() }, ..State::default() };
        let v = st.view();
        assert_eq!(v["token_hint"], "1234");
        assert!(!v.to_string().contains("abcdefgh"));
    }
}

//! Per-MAC limits on routed LAN traffic. Bridge hooks see the client MAC in both directions,
//! including packets delivered to and returned by a transparent proxy.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::rpc::Params;
use f50_core::{Error, Result, fsx};
use f50_platform::Platform;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::dash::{CLIENTS_MAX, LAN_BRIDGE, is_mac};

const TABLE: &str = "f50_client_limits";
const TIMEOUT: Duration = Duration::from_secs(4);
const CHECKPOINT: Duration = Duration::from_secs(60);
pub(crate) const MAX_CAP: i64 = 9_007_199_254_740_991;
pub(crate) const MAX_KBPS: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
struct Limit {
    cap_bytes: u64,
    upload_kbps: u64,
    download_kbps: u64,
    used_bytes: u64,
}

impl Limit {
    fn unlimited(self) -> bool {
        self.cap_bytes == 0 && self.upload_kbps == 0 && self.download_kbps == 0
    }
}

type Limits = BTreeMap<String, Limit>;
type Addresses = BTreeSet<IpAddr>;

#[derive(Debug)]
pub(crate) struct ClientLimits {
    file: PathBuf,
    addresses: Mutex<Option<Addresses>>,
}

impl ClientLimits {
    pub(crate) fn new(platform: &Platform) -> Self {
        Self { file: platform.paths().state().join("client-limits.json"), addresses: Mutex::default() }
    }

    fn load(&self) -> Result<Limits> {
        let Some(text) = fsx::read_opt(&self.file)? else { return Ok(Limits::new()) };
        let limits: Limits = decode(&text)?;
        if limits.len() > CLIENTS_MAX
            || limits.iter().any(|(mac, p)| {
                !is_mac(mac)
                    || p.unlimited()
                    || p.cap_bytes > MAX_CAP as u64
                    || p.upload_kbps > MAX_KBPS as u64
                    || p.download_kbps > MAX_KBPS as u64
                    || p.used_bytes > MAX_CAP as u64
            })
        {
            return Err(Error::failed("invalid saved client limits"));
        }
        Ok(limits)
    }

    fn save(&self, limits: &Limits) -> Result<()> {
        if limits.is_empty() {
            return match std::fs::remove_file(&self.file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(Error::io(self.file.display(), &e)),
            };
        }
        if let Some(dir) = self.file.parent() {
            fsx::private_dir(dir)?;
        }
        let bytes = serde_json::to_vec(limits).map_err(|e| Error::failed(e.to_string()))?;
        fsx::write_atomic(&self.file, &bytes, 0o600)
    }

    pub(crate) async fn view(&self) -> Result<(Value, bool)> {
        let _lock = self.addresses.lock().await;
        let mut limits = self.load()?;
        let active = if limits.is_empty() {
            true
        } else if let Ok(Some(table)) = table().await {
            read_usage(&mut limits, &table);
            true
        } else {
            false
        };
        let view = serde_json::to_value(limits).map_err(|e| Error::failed(e.to_string()))?;
        Ok((view, active))
    }

    pub(crate) async fn set(&self, p: &Params) -> Result<()> {
        let mac = p.need_str("mac")?.to_ascii_lowercase();
        let number = |key: &'static str| u64::try_from(p.need_int(key)?).map_err(|_| Error::bad_value(key));
        let mut limit = Limit {
            cap_bytes: number("cap_bytes")?,
            upload_kbps: number("upload_kbps")?,
            download_kbps: number("download_kbps")?,
            used_bytes: 0,
        };
        let mut addresses = self.addresses.lock().await;
        let mut previous = self.load()?;
        let live = table().await?;
        if let Some(table) = &live {
            read_usage(&mut previous, table);
        }
        if limit.cap_bytes != 0 {
            limit.used_bytes = previous.get(&mac).map_or(0, |p| p.used_bytes);
        }
        let mut next = previous.clone();
        if limit.unlimited() {
            next.remove(&mac);
        } else {
            next.insert(mac, limit);
        }
        if next == previous && (live.is_some() || next.is_empty()) {
            return Ok(());
        }
        if next.len() > CLIENTS_MAX {
            return Err(Error::busy("too many client limits"));
        }
        let local = local_addresses().await?;
        apply_rules(&next, &local).await?;
        let offload_changed = previous.is_empty() != next.is_empty();
        let result: Result<()> = async {
            self.save(&next)?;
            if offload_changed {
                reload_firewall(!next.is_empty()).await?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            if let Err(restore) = self.save(&previous) {
                tracing::error!(error = %restore, "restore saved client limits");
            }
            if let Err(restore) = apply_rules(&previous, &local).await {
                tracing::error!(error = %restore, "restore client limit rules");
            }
            if offload_changed && let Err(restore) = reload_firewall(!previous.is_empty()).await {
                tracing::error!(error = %restore, "restore firewall offload");
            }
            return Err(e);
        }
        *addresses = Some(local);
        Ok(())
    }

    /// Restores the rules after a boot, updates local address exemptions and checkpoints quota usage.
    pub(crate) async fn run(&self, shutdown: CancellationToken) {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut saved = Instant::now();
        loop {
            let stopping = tokio::select! {
                () = shutdown.cancelled() => true,
                _ = tick.tick() => false,
            };
            let checkpoint = stopping || saved.elapsed() >= CHECKPOINT;
            match self.maintain(checkpoint, stopping).await {
                Ok(()) if checkpoint => saved = Instant::now(),
                Err(e) => tracing::warn!(error = %e, "client limits"),
                _ => {}
            }
            if stopping {
                break;
            }
        }
    }

    async fn maintain(&self, checkpoint: bool, stopping: bool) -> Result<()> {
        let mut addresses = self.addresses.lock().await;
        let mut current = self.load()?;
        if current.is_empty() {
            if !stopping && addresses.is_none() {
                if table().await?.is_some() {
                    apply_rules(&current, &Addresses::new()).await?;
                }
                *addresses = Some(Addresses::new());
            }
            return Ok(());
        }
        let live = table().await?;
        if let Some(table) = &live {
            let saved = current.clone();
            read_usage(&mut current, table);
            if checkpoint && current != saved {
                self.save(&current)?;
            }
        }
        if stopping {
            return Ok(());
        }
        let local = local_addresses().await?;
        if live.is_none() || addresses.is_none() {
            apply_rules(&current, &local).await?;
        } else if addresses.as_ref() != Some(&local) {
            // Address changes must not replace the live quota or rate buckets.
            let mut script = String::new();
            for (suffix, v4) in [("4", true), ("6", false)] {
                let _ = writeln!(script, "flush set bridge {TABLE} router{suffix}");
                let list = address_list(&local, v4);
                if !list.is_empty() {
                    let _ = writeln!(script, "add element bridge {TABLE} router{suffix} {{ {list} }}");
                }
            }
            nft(script).await?;
        }
        *addresses = Some(local);
        Ok(())
    }
}

fn decode<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| Error::failed(e.to_string()))
}

async fn local_addresses() -> Result<Addresses> {
    let out = Cmd::new("ip").args(["-j", "address", "show"]).timeout(TIMEOUT).ok().await?;
    let links: Vec<Value> = decode(&out)?;
    Ok(links
        .iter()
        .filter_map(|link| link["addr_info"].as_array())
        .flatten()
        .filter_map(|addr| addr["local"].as_str()?.parse::<IpAddr>().ok())
        .collect())
}

fn address_list(addresses: &Addresses, v4: bool) -> String {
    addresses.iter().filter(|ip| ip.is_ipv4() == v4).map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

async fn table() -> Result<Option<Value>> {
    let out = Cmd::new("nft").args(["-j", "list", "table", "bridge", TABLE]).timeout(TIMEOUT).output().await?;
    if out.success() {
        return decode(&out.stdout).map(Some);
    }
    if out.stderr.contains("No such file or directory") {
        return Ok(None);
    }
    Err(out.error("nft"))
}

fn quota_name(mac: &str) -> String {
    format!("quota_{}", mac.replace(':', ""))
}

fn read_usage(limits: &mut Limits, table: &Value) {
    let Some(items) = table["nftables"].as_array() else { return };
    for (mac, p) in limits.iter_mut().filter(|(_, p)| p.cap_bytes != 0) {
        let name = quota_name(mac);
        if let Some(used) = items
            .iter()
            .map(|item| &item["quota"])
            .find(|q| q["name"].as_str() == Some(name.as_str()))
            .and_then(|q| q["used"].as_u64())
        {
            p.used_bytes = p.used_bytes.max(used.min(p.cap_bytes));
        }
    }
}

async fn reload_firewall(limited: bool) -> Result<()> {
    Cmd::new("/sbin/fw4").arg("reload").timeout(TIMEOUT).ok().await?;
    if limited {
        let out = Cmd::new("nft").args(["-j", "list", "flowtables", "inet"]).timeout(TIMEOUT).ok().await?;
        let tables: Value = decode(&out)?;
        if tables["nftables"].as_array().is_some_and(|items| {
            items.iter().any(|item| item["flowtable"]["table"] == "fw4" && item["flowtable"]["name"] == "ft")
        }) {
            return Err(Error::failed("firewall flow offloading is incompatible with client limits on this firmware"));
        }
    }
    Ok(())
}

async fn nft(script: String) -> Result<()> {
    Cmd::new("nft").args(["-f", "-"]).stdin(script.into_bytes()).timeout(TIMEOUT).ok().await.map(drop)
}

async fn apply_rules(limits: &Limits, addresses: &Addresses) -> Result<()> {
    let mut script = format!("table bridge {TABLE}\ndelete table bridge {TABLE}\n");
    if limits.is_empty() {
        return nft(script).await;
    }
    let _ = writeln!(script, "table bridge {TABLE} {{");
    for (suffix, v4) in [("4", true), ("6", false)] {
        let _ = writeln!(script, "set router{suffix} {{ type ipv{suffix}_addr;");
        let list = address_list(addresses, v4);
        if !list.is_empty() {
            let _ = writeln!(script, "elements = {{ {list} }};");
        }
        let _ = writeln!(script, "}}");
    }
    for (mac, p) in limits.iter().filter(|(_, p)| p.cap_bytes != 0) {
        let name = quota_name(mac);
        let used = p.used_bytes.min(p.cap_bytes);
        let _ = writeln!(script, "quota {name} {{ over {} bytes used {used} bytes; }}", p.cap_bytes);
    }
    for (hook, bridge, side, local) in [("input", "ibrname", "saddr", "daddr"), ("output", "obrname", "daddr", "saddr")] {
        let _ = writeln!(script, "chain {hook} {{ type filter hook {hook} priority 0; policy accept;");
        let _ = writeln!(script, "meta {bridge} != \"{LAN_BRIDGE}\" return");
        let _ = writeln!(script, "ether type != {{ ip, ip6 }} return");
        let _ = writeln!(script, "ether daddr & 01:00:00:00:00:00 != 00:00:00:00:00:00 return");
        let _ = writeln!(script, "ip {local} @router4 return");
        let _ = writeln!(script, "ip6 {local} @router6 return");
        let _ = writeln!(script, "ip6 {local} fe80::/10 return");
        for (mac, p) in limits {
            let rate = if hook == "input" { p.upload_kbps } else { p.download_kbps };
            let client = format!("ether {side} {mac}");
            if rate != 0 {
                let _ = writeln!(script, "{client} limit rate over {} bytes/second burst 64 kbytes drop", rate * 125);
            }
            if p.cap_bytes != 0 {
                let _ = writeln!(script, "{client} quota name {} drop", quota_name(mac));
            }
        }
        let _ = writeln!(script, "}}");
    }
    let _ = writeln!(script, "}}");
    nft(script).await
}

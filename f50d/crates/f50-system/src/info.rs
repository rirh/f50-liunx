//! The read side of `f50.system`: `info` (facts and live figures), `status` (the home page's non-modem numbers)
//! and `clients` (who is on the hotspot and the LAN: private, so a write-grant method).

use serde_json::{Value, json};

use crate::{System, dash, device, facts, thermal, zram};

impl System {
    /// `f50.system info`.
    pub async fn info(&self) -> f50_core::Result<Value> {
        let p = &self.inner.platform;
        let zones = thermal::zones(p);
        let timezone = p
            .uci()
            .section_of_type("system", "system", 0)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.get("zonename").map(str::to_owned));
        let root = facts::disk(&p.paths().sys("/")).await;
        let disk = facts::disk(&p.paths().disk()).await;
        Ok(json!({
            "device": device::DEVICE,
            "system": "openwrt",
            "model": device::model(p),
            "release": device::release(p),
            "kernel": device::kernel(p),
            "hostname": p.system().hostname(),
            "timezone": timezone,
            "user": "root",
            "time": f50_core::time::unix_now(),
            "uptime": facts::uptime(p),
            "load": facts::cpu_times(p),
            "memory": facts::memory(p),
            "storage": { "root": root, "disk": disk },
            "temperatures": thermal::temps(&zones),
            "zram": zram::status(p),
        }))
    }

    /// `f50.system status`: what the home page polls besides the radio. Counters are cumulative; the page diffs them.
    pub async fn status(&self) -> f50_core::Result<Value> {
        let p = &self.inner.platform;
        let cfg = &self.inner.cfg;
        let zones = thermal::zones(p);
        let stations = dash::stations(&cfg.wifi_device, false).await;
        let leases = dash::leases(&p.paths().sys(&cfg.leases_file));
        Ok(json!({
            "ts": f50_core::time::unix_now(),
            "uptime": facts::uptime(p),
            "load": facts::cpu_times(p),
            "memory": facts::memory(p),
            "temperatures": thermal::temps(&zones),
            "heat_alarm": *self.inner.alarm.borrow(),
            "wifi_clients": stations.len(),
            "lan_leases": leases.len(),
            "conns": dash::conntrack(p).await,
            "usb_link": dash::usb_link(p),
            "update": dash::update_tag(&p.paths().sys(&cfg.update_notice)),
        }))
    }

    /// `f50.system clients`: the hotspot's stations (with names and addresses from the leases) and the leases.
    pub async fn clients(&self) -> f50_core::Result<Value> {
        let p = &self.inner.platform;
        let cfg = &self.inner.cfg;
        let mut stations = dash::stations(&cfg.wifi_device, true).await;
        let leases = dash::leases(&p.paths().sys(&cfg.leases_file));
        dash::join(&mut stations, &leases);
        Ok(json!({ "wifi": stations, "leases": leases }))
    }
}

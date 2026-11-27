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
        }))
    }

    /// `f50.system clients`: connected stations, DHCP leases and per-client limits.
    pub async fn clients(&self) -> f50_core::Result<Value> {
        let p = &self.inner.platform;
        let cfg = &self.inner.cfg;
        let (mut stations, mut connected, limits) = tokio::join!(
            dash::stations(&cfg.wifi_device, true),
            dash::wired_clients(p, &cfg.wifi_device),
            self.inner.client_limits.view(),
        );
        let leases = dash::leases(&p.paths().sys(&cfg.leases_file));
        for client in &mut connected {
            dash::join(std::slice::from_mut(&mut client.station), &leases);
        }
        dash::join(&mut stations, &leases);
        connected.retain(|client| !stations.iter().any(|s| s.mac == client.station.mac));
        connected.extend(stations.iter().cloned().map(|station| dash::ConnectedClient { connection: "wifi", station }));
        let (limits, limits_active) = match limits {
            Ok(view) => view,
            Err(e) => {
                tracing::warn!(error = %e, "read client limits");
                (Value::Null, false)
            }
        };
        Ok(json!({
            "connected": connected,
            "wifi": stations,
            "leases": leases,
            "limits": limits,
            "limits_active": limits_active,
        }))
    }
}

//! netifd interfaces through ubus (`network.interface.<name>`), the replacement of `ifstatus`, `ifup`, `ifdown` and
//! `ubus call network reload`.

use f50_core::Result;
use serde_json::{Value, json};

use crate::Platform;
use crate::check::{is_object_name, require};

#[derive(Debug, Clone)]
pub struct Netifd {
    platform: Platform,
}

impl Netifd {
    pub(crate) fn new(platform: Platform) -> Self {
        Netifd { platform }
    }

    /// `ubus call network.interface.<name> status`, as netifd returns it; `None` when there is no such interface.
    pub async fn status(&self, interface: &str) -> Result<Option<Value>> {
        require(is_object_name(interface), "interface")?;
        self.platform.call_opt(&format!("network.interface.{interface}"), "status", &json!({})).await
    }

    pub async fn up(&self, interface: &str) -> Result<()> {
        require(is_object_name(interface), "interface")?;
        self.platform.call(&format!("network.interface.{interface}"), "up", &json!({})).await.map(drop)
    }

    pub async fn down(&self, interface: &str) -> Result<()> {
        require(is_object_name(interface), "interface")?;
        self.platform.call(&format!("network.interface.{interface}"), "down", &json!({})).await.map(drop)
    }

    /// `ubus call network reload` (after a change to /etc/config/network).
    pub async fn reload(&self) -> Result<()> {
        self.platform.call("network", "reload", &json!({})).await.map(drop)
    }

    /// `ubus call network.interface dump`: the status of every interface (each has its `interface` name).
    pub async fn dump(&self) -> Result<Vec<Value>> {
        let v = self.platform.call("network.interface", "dump", &json!({})).await?;
        Ok(v.get("interface").and_then(Value::as_array).cloned().unwrap_or_default())
    }

    /// The interface is up (`"up": true` in its status); a missing interface is down.
    pub async fn is_up(&self, interface: &str) -> Result<bool> {
        Ok(self.status(interface).await?.as_ref().is_some_and(is_up))
    }

    /// The Linux device the interface's traffic goes through (`l3_device`, else `device`), as `ifstatus | jsonfilter
    /// -e @.l3_device` gave it; `None` when the interface is missing or has none.
    pub async fn interface_device(&self, interface: &str) -> Result<Option<String>> {
        Ok(self.status(interface).await?.as_ref().and_then(device_of))
    }

    /// `ubus call network.device status {"name": <device>}`; `None` when netifd does not know the device.
    pub async fn device_status(&self, device: &str) -> Result<Option<Value>> {
        require(is_object_name(device), "device")?;
        match self.platform.call("network.device", "status", &json!({ "name": device })).await {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.code() == f50_core::Code::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// `"up": true` in an interface status.
pub fn is_up(status: &Value) -> bool {
    status.get("up").and_then(Value::as_bool).unwrap_or(false)
}

/// `l3_device`, else `device`, of an interface status.
pub fn device_of(status: &Value) -> Option<String> {
    ["l3_device", "device"]
        .iter()
        .find_map(|k| status.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
        .map(str::to_owned)
}

/// The addresses of an interface status under `key` (`ipv4-address`, `ipv6-address`, `ipv6-prefix-assignment`), as
/// `address/mask` strings.
pub fn addresses(status: &Value, key: &str) -> Vec<String> {
    status
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let addr = e.get("address").and_then(Value::as_str)?;
                    Some(match e.get("mask").and_then(Value::as_u64) {
                        Some(m) => format!("{addr}/{m}"),
                        None => addr.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The DNS servers of an interface status (`dns-server`).
pub fn dns_servers(status: &Value) -> Vec<String> {
    status
        .get("dns-server")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_fields() {
        let s = json!({
            "up": true, "l3_device": "sipa_eth0", "device": "sipa_eth0",
            "ipv4-address": [{"address": "10.1.2.3", "mask": 30}],
            "ipv6-address": [{"address": "2a00::1", "mask": 64}],
            "dns-server": ["10.0.0.1", "10.0.0.2"]
        });
        assert!(is_up(&s));
        assert_eq!(device_of(&s).as_deref(), Some("sipa_eth0"));
        assert_eq!(addresses(&s, "ipv4-address"), vec!["10.1.2.3/30"]);
        assert_eq!(addresses(&s, "ipv6-prefix-assignment"), Vec::<String>::new());
        assert_eq!(dns_servers(&s), vec!["10.0.0.1", "10.0.0.2"]);
        assert_eq!(device_of(&json!({"up": false, "device": "br-lan", "l3_device": ""})).as_deref(), Some("br-lan"));
        assert!(!is_up(&json!({})));
    }

    #[tokio::test]
    async fn names_are_checked_before_the_bus() {
        let p = Platform::new(f50_core::paths::Paths::new("/nonexistent"), None);
        assert_eq!(p.netifd().status("wan/../x").await.expect_err("bad").code(), f50_core::Code::BadValue);
        assert_eq!(p.netifd().status("wan").await.expect_err("no bus").code(), f50_core::Code::Unsupported);
    }
}

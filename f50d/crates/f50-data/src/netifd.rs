//! What netifd hears from the `f50cell` protocol, sent by the daemon itself: `ubus call network.interface
//! notify_proto {"interface":..., "action":N, ...}`, the call `/lib/netifd/netifd-proto.sh` makes for a shell
//! protocol (proto-shell.c dispatches on `action`):
//!
//! | action | netifd-proto.sh | here |
//! |---|---|---|
//! | 0 | `proto_init_update` .. `proto_send_update` | [`Notifier::update`]: link up, addresses, routes, DNS |
//! | 3 | `proto_notify_error` | [`Notifier::error`] |
//! | 4 | `proto_block_restart` | [`Notifier::block_restart`] |
//! | 5 | `proto_set_available` | [`Notifier::set_available`] |
//! | 7 | `proto_setup_failed` | [`Notifier::setup_failed`]: netifd runs the teardown, then tries again |
//!
//! The handler declares `no_proto_task`: its setup script returns at once, and netifd waits in its setup state for
//! one of these. Every update is **address-external** (the third argument of `proto_init_update`): the kernel state
//! of the bearer is ours, netifd only records it for its status and the firewall. Mixing the two (netifd installing
//! the address at setup, a later report saying external) makes netifd delete the address it installed the moment the
//! external report arrives; the old watchdog then saw a WAN without an address and redialled every 60 s.
//!
//! Field types follow netifd-proto.sh: masks and netmasks are strings, `action`, `metric`, `preferred` and `valid`
//! are integers (netifd's policy wants blobmsg INT32 there).

use f50_core::Result;
use f50_platform::Platform;
use serde_json::{Map, Value, json};

/// One `proto_send_update`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Update {
    pub ifname: String,
    /// (address, prefix length)
    pub ipv4: Vec<(String, u8)>,
    /// The IPv4 default route through the device.
    pub default4: bool,
    /// (address, prefix length, preferred, valid): the RA's addresses in relay mode; no lifetime is forever.
    pub ipv6: Vec<(String, u8, Option<u32>, Option<u32>)>,
    /// The RA's default route: (gateway, remaining lifetime).
    pub default6: Option<(String, Option<u32>)>,
    pub dns: Vec<String>,
}

impl Update {
    /// The message netifd-proto.sh would build for this update.
    pub fn to_json(&self, interface: &str) -> Value {
        let mut m = Map::new();
        m.insert("action".into(), json!(0));
        m.insert("ifname".into(), json!(self.ifname));
        m.insert("link-up".into(), json!(true));
        m.insert("address-external".into(), json!(true));
        m.insert("keep".into(), json!(false));
        if !self.ipv4.is_empty() {
            let a: Vec<Value> = self.ipv4.iter().map(|(ip, p)| json!({"ipaddr": ip, "mask": p.to_string()})).collect();
            m.insert("ipaddr".into(), Value::Array(a));
        }
        if !self.ipv6.is_empty() {
            let a: Vec<Value> = self
                .ipv6
                .iter()
                .map(|(ip, p, pref, valid)| {
                    let mut o = json!({"ipaddr": ip, "mask": p.to_string()});
                    if let Some(v) = pref {
                        o["preferred"] = json!(v);
                    }
                    if let Some(v) = valid {
                        o["valid"] = json!(v);
                    }
                    o
                })
                .collect();
            m.insert("ip6addr".into(), Value::Array(a));
        }
        if self.default4 {
            m.insert("routes".into(), json!([{"target": "0.0.0.0", "netmask": "0"}]));
        }
        if let Some((gw, valid)) = &self.default6 {
            let mut r = json!({"target": "::", "netmask": "0", "metric": 4096});
            if !gw.is_empty() {
                r["gateway"] = json!(gw);
            }
            if let Some(v) = valid {
                r["valid"] = json!(v);
            }
            m.insert("routes6".into(), json!([r]));
        }
        if !self.dns.is_empty() {
            m.insert("dns".into(), json!(self.dns));
        }
        m.insert("interface".into(), json!(interface));
        Value::Object(m)
    }
}

/// Sends netifd the protocol's notifications.
#[derive(Debug)]
pub struct Notifier<'a> {
    platform: &'a Platform,
}

impl<'a> Notifier<'a> {
    pub fn new(platform: &'a Platform) -> Self {
        Notifier { platform }
    }

    async fn call(&self, object: &str, method: &str, msg: &Value) -> Result<Value> {
        self.platform.call(object, method, msg).await
    }

    async fn notify(&self, interface: &str, mut msg: Value) -> Result<()> {
        msg["interface"] = json!(interface);
        self.call("network.interface", "notify_proto", &msg).await.map(drop)
    }

    pub async fn update(&self, interface: &str, u: &Update) -> Result<()> {
        self.call("network.interface", "notify_proto", &u.to_json(interface)).await.map(drop)
    }

    /// `proto_notify_error`: the codes netifd shows as the interface's errors.
    pub async fn error(&self, interface: &str, code: &str) -> Result<()> {
        self.notify(interface, json!({"action": 3, "error": [code]})).await
    }

    /// `proto_block_restart`: no retry after this failure.
    pub async fn block_restart(&self, interface: &str) -> Result<()> {
        self.notify(interface, json!({"action": 4})).await
    }

    /// `proto_set_available`: an unavailable interface is not started; made available again, netifd starts it
    /// unless somebody took it down (its autostart is off then).
    pub async fn set_available(&self, interface: &str, available: bool) -> Result<()> {
        self.notify(interface, json!({"action": 5, "available": available})).await
    }

    /// `proto_setup_failed`: netifd tears the interface down and, with autostart on, sets it up again.
    pub async fn setup_failed(&self, interface: &str) -> Result<()> {
        self.notify(interface, json!({"action": 7})).await
    }

    /// `ubus call network add_dynamic`: the DHCPv6 alias of the bearer.
    pub async fn add_dynamic(&self, spec: &Value) -> Result<()> {
        self.call("network", "add_dynamic", spec).await.map(drop)
    }
}

/// What the watchdog and the traffic count read of an interface's status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IfStatus {
    pub up: bool,
    pub pending: bool,
    pub available: bool,
    /// Off after an `ifdown` (the user, the panel, the data cap): data is down on purpose.
    pub autostart: bool,
    pub l3_device: Option<String>,
    /// The addresses netifd has for it, `address/mask`.
    pub addresses: Vec<String>,
    /// Seconds it has been up.
    pub uptime: Option<u64>,
}

impl IfStatus {
    pub fn parse(v: &Value) -> IfStatus {
        let b = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        let mut addresses = Vec::new();
        for key in ["ipv4-address", "ipv6-address"] {
            for a in v.get(key).and_then(Value::as_array).into_iter().flatten() {
                if let (Some(ip), Some(m)) = (a.get("address").and_then(Value::as_str), a.get("mask")) {
                    addresses.push(format!("{ip}/{m}"));
                }
            }
        }
        IfStatus {
            up: b("up"),
            pending: b("pending"),
            available: b("available"),
            autostart: b("autostart"),
            l3_device: v.get("l3_device").and_then(Value::as_str).map(str::to_owned),
            addresses,
            uptime: v.get("uptime").and_then(Value::as_u64),
        }
    }

    pub async fn read(platform: &Platform, interface: &str) -> Result<Option<IfStatus>> {
        Ok(platform.netifd().status(interface).await?.as_ref().map(IfStatus::parse))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_is_external_with_string_masks() {
        let u = Update {
            ifname: "sipa_eth0".into(),
            ipv4: vec![("10.1.2.3".into(), 30)],
            default4: true,
            ipv6: vec![("2001:db8::1".into(), 64, None, Some(7200))],
            default6: Some(("fe80::1".into(), Some(1800))),
            dns: vec!["10.0.0.1".into()],
        };
        let v = u.to_json("wan");
        assert_eq!(v["action"], 0);
        assert_eq!(v["address-external"], true);
        assert_eq!(v["link-up"], true);
        assert_eq!(v["ipaddr"][0]["mask"], "30");
        assert_eq!(v["ip6addr"][0]["valid"], 7200);
        assert!(v["ip6addr"][0].get("preferred").is_none());
        assert_eq!(v["routes"][0]["netmask"], "0");
        assert_eq!(v["routes6"][0]["metric"], 4096);
        assert_eq!(v["interface"], "wan");
    }

    #[test]
    fn status_fields() {
        let s = IfStatus::parse(&json!({"up":true,"autostart":false,"l3_device":"sipa_eth0",
            "uptime":2840,"ipv4-address":[{"address":"10.1.2.3","mask":30}]}));
        assert_eq!(s.uptime, Some(2840));
        assert!(s.up && !s.autostart);
        assert_eq!(s.l3_device.as_deref(), Some("sipa_eth0"));
        assert_eq!(s.addresses, vec!["10.1.2.3/30"]);
    }
}

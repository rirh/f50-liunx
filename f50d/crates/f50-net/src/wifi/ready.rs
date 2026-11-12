//! Is the Wi-Fi up as `/etc/config/wireless` has it? One read-only look at netifd's and hostapd's state over ubus; it
//! never enables a radio or an interface (after kanoqwq f0b5ad43). Ported from `wifi-ready`.
//!
//! The boot asks it before deciding on its one Wi-Fi restart. It used to ask `hostapd.wlan0` alone, which a hotspot
//! switched off never has: such a boot waited the full 20 s and then restarted the Wi-Fi for nothing.

use serde::Serialize;
use serde_json::{Value, json};

use super::Wireless;
use crate::{Net, tools};

/// The answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WifiReady {
    /// Every enabled radio up, every enabled interface on it there, every enabled access point ENABLED in hostapd. A
    /// radio or an interface switched off counts as ready: there is nothing to wait for.
    Ready,
    /// Not there yet, no radio configured, or ubus not answering.
    Waiting,
    /// It will not come by waiting: netifd gave a radio up (`retry_setup_failed`), or hostapd reports an access point
    /// DISABLED (one started under the world regulatory domain ends there; only a Wi-Fi restart brings it back).
    Failed,
}

/// The decision from netifd's `network.wireless status` and each access point's hostapd status.
pub(crate) fn decide(w: &Wireless, runtime: &Value, hostapd: &dyn Fn(&str) -> Option<String>) -> WifiReady {
    let (mut configured, mut waiting, mut failed) = (0, false, false);
    for radio in w.radios() {
        configured += 1;
        if radio.get("disabled") == Some("1") {
            continue;
        }
        let r = runtime.get(&radio.name);
        let flag = |k: &str| r.and_then(|r| r.get(k)).and_then(Value::as_bool).unwrap_or(false);
        if flag("retry_setup_failed") || flag("failed") {
            failed = true;
            continue;
        }
        if !flag("up") || flag("pending") {
            waiting = true;
            continue;
        }
        let actual = r.and_then(|r| r.get("interfaces")).and_then(Value::as_array).cloned().unwrap_or_default();
        for iface in w.ifaces() {
            if iface.get("device") != Some(radio.name.as_str()) || iface.get("disabled") == Some("1") {
                continue;
            }
            let ifname = actual
                .iter()
                .find(|i| i.get("section").and_then(Value::as_str) == Some(iface.name.as_str()))
                .and_then(|i| i.get("ifname"))
                .and_then(Value::as_str);
            let Some(ifname) = ifname else {
                waiting = true;
                continue;
            };
            if iface.get("mode") != Some("ap") {
                continue;
            }
            match hostapd(ifname).as_deref() {
                Some("DISABLED") => failed = true,
                Some("ENABLED") => {}
                _ => waiting = true,
            }
        }
    }
    if failed {
        WifiReady::Failed
    } else if waiting || configured == 0 {
        WifiReady::Waiting
    } else {
        WifiReady::Ready
    }
}

pub(crate) async fn check(net: &Net) -> WifiReady {
    let Ok(runtime) = tools::ubus(net, "network.wireless", "status", &json!({})).await else {
        return WifiReady::Waiting;
    };
    let Ok(w) = Wireless::load(net).await else { return WifiReady::Waiting };
    // the access points' hostapd states first (async), then the pure decision
    // the names first, owned: no borrowing iterator may live across the awaits below (its closures make the
    // future's Send impl unprovable once spawned, rust-lang/rust#102211)
    let ifnames: Vec<String> = runtime
        .as_object()
        .map(|radios| {
            radios
                .values()
                .filter_map(|r| r.get("interfaces")?.as_array())
                .flatten()
                .filter_map(|i| i.get("ifname").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut states = std::collections::HashMap::new();
    for ifname in ifnames {
        let st = tools::ubus(net, &format!("hostapd.{ifname}"), "get_status", &json!({})).await.ok();
        let st = st.and_then(|s| s.get("status")?.as_str().map(str::to_owned));
        states.insert(ifname, st);
    }
    decide(&w, &runtime, &|i| states.get(i).cloned().flatten())
}

#[cfg(test)]
mod tests {
    use super::super::tests::sec;
    use super::*;

    fn w(ap_disabled: &str) -> Wireless {
        Wireless {
            sections: vec![
                sec("radio0", "wifi-device", &[]),
                sec("ap0", "wifi-iface", &[("device", "radio0"), ("mode", "ap"), ("disabled", ap_disabled)]),
            ],
        }
    }

    #[test]
    fn decisions() {
        let up = json!({"radio0": {"up": true, "interfaces": [{"section": "ap0", "ifname": "wlan0"}]}});
        assert_eq!(decide(&w("0"), &up, &|_| Some("ENABLED".into())), WifiReady::Ready);
        assert_eq!(decide(&w("0"), &up, &|_| Some("DISABLED".into())), WifiReady::Failed);
        assert_eq!(decide(&w("0"), &up, &|_| None), WifiReady::Waiting);
        // switched off: nothing to wait for
        assert_eq!(decide(&w("1"), &json!({"radio0": {"up": true}}), &|_| None), WifiReady::Ready);
        assert_eq!(decide(&w("0"), &json!({"radio0": {"retry_setup_failed": true}}), &|_| None), WifiReady::Failed);
        assert_eq!(decide(&w("0"), &json!({"radio0": {"up": true, "pending": true}}), &|_| None), WifiReady::Waiting);
        assert_eq!(decide(&Wireless::default(), &json!({}), &|_| None), WifiReady::Waiting);
    }
}

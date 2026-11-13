//! The hotspot as it is: `f50.wifi get` (never the password, only whether one is set), and the hotspot on or off.
//!
//! On or off turns the access point's own `wifi-iface` on or off, not the whole radio: another interface on the same
//! radio stays as it is. The radio is turned on with the access point only when it was off and nothing else on it
//! would come up with it (a radio switched off by the Wi-Fi key comes back, FINDINGS 222). netifd then reloads, which
//! it does asynchronously: `pending` is all that can be said at that point. Ported from `hotspot-ap.sh` (after
//! kanoqwq's db4e5d2) and `f50-api wifi.get/power`.

use serde_json::{Value, json};

use f50_core::{Error, Result};

use super::{Wireless, band_of, channels, country_of, set};
use crate::{Net, client, tools};

/// The interface netifd runs the access point section on (`network.wireless status`), else the configured one.
pub(crate) async fn ap_ifname(net: &Net, section: &str) -> String {
    let st = tools::ubus(net, "network.wireless", "status", &json!({})).await.unwrap_or(Value::Null);
    let found = st.as_object().into_iter().flat_map(|radios| radios.values()).find_map(|r| {
        r.get("interfaces")?.as_array()?.iter().find_map(|i| {
            (i.get("section")?.as_str()? == section).then(|| i.get("ifname")?.as_str().map(str::to_owned)).flatten()
        })
    });
    found.unwrap_or_else(|| net.cfg().wlan.clone())
}

fn security(enc: Option<&str>) -> &'static str {
    match enc.unwrap_or("") {
        e if e.starts_with("psk2") || e.starts_with("psk-mixed") => "wpa2",
        e if e.starts_with("sae-mixed") || e.starts_with("sae-compat") || e.starts_with("psk3-mixed") => "wpa2-wpa3",
        e if e.starts_with("sae") || e.starts_with("psk3") => "wpa3",
        "none" | "" => "open",
        _ => "other",
    }
}

/// The stations of the access point, with the DHCP lease's address and name where there is one.
async fn clients(net: &Net, ifname: &str) -> Vec<Value> {
    let Ok(v) = tools::ubus(net, &format!("hostapd.{ifname}"), "get_clients", &json!({})).await else {
        return Vec::new();
    };
    let leases = tokio::fs::read_to_string(net.path(&net.cfg().leases)).await.unwrap_or_default();
    let macs = v.get("clients").and_then(Value::as_object).map(|m| m.keys().cloned().collect::<Vec<_>>());
    macs.unwrap_or_default()
        .into_iter()
        .map(|m| m.to_ascii_lowercase())
        .filter(|m| f50_core::rpc::matches("[0-9a-f]{2}(:[0-9a-f]{2}){5}", m))
        .take(64)
        .map(|mac| {
            let lease = leases
                .lines()
                .map(|l| l.split_whitespace().collect::<Vec<_>>())
                .find(|w| w.get(1).is_some_and(|x| x.eq_ignore_ascii_case(&mac)));
            let ip = lease.as_ref().and_then(|w| w.get(2)).map(|s| (*s).to_owned());
            let name = lease.as_ref().and_then(|w| w.get(3)).filter(|n| **n != "*").map(|s| (*s).to_owned());
            json!({ "mac": mac, "ip": ip, "name": name })
        })
        .collect()
}

/// `f50.wifi get`.
pub(crate) async fn get(net: &Net) -> Result<Value> {
    let w = Wireless::load(net).await?;
    let (radio, iface) =
        w.hotspot(&net.cfg().wlan).ok_or_else(|| Error::not_found("access point in /etc/config/wireless"))?;
    let cc = country_of(net, radio);
    let ifname = ap_ifname(net, &iface.name).await;
    let status = tools::ubus(net, &format!("hostapd.{ifname}"), "get_status", &json!({})).await.ok();
    let live = status.as_ref().and_then(|s| s.get("channel")).and_then(Value::as_u64);
    let live = match live {
        Some(c) => Some(c),
        None => tools::iw(&["dev", &ifname, "info"], std::time::Duration::from_secs(5))
            .await
            .ok()
            .and_then(|i| tools::parse_iw_channel(&i))
            .map(u64::from),
    };
    let enabled = radio.get("disabled") != Some("1") && iface.get("disabled") != Some("1");
    Ok(json!({
        "enabled": enabled,
        "client_mode": client::is_active(net).await,
        "ssid": iface.get("ssid"),
        "band": band_of(radio),
        "channel": radio.get("channel").unwrap_or("auto"),
        "channel_live": live,
        "htmode": radio.get("htmode"),
        "country": cc,
        "security": security(iface.get("encryption")),
        "hidden": iface.get("hidden") == Some("1"),
        "password_set": iface.get("key").is_some_and(|k| !k.is_empty()),
        "channels": { "2.4": channels::AP2_ALL, "5": channels::ap5_channels(&cc) },
        "status": status.as_ref().and_then(|s| s.get("status")).cloned(),
        "clients": clients(net, &ifname).await,
    }))
}

/// Uncommitted wireless changes: uci's own (`/tmp/.uci`) or a LuCI session's (rpcd keeps them apart). Applying
/// over them would commit them too, or lose them.
async fn staged(net: &Net) -> bool {
    let nonempty = |p: std::path::PathBuf| std::fs::metadata(p).is_ok_and(|m| m.len() > 0);
    if nonempty(net.path("/tmp/.uci/wireless")) {
        return true;
    }
    let Ok(rd) = std::fs::read_dir(net.path("/var/run/rpcd")) else { return false };
    rd.flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("uci-"))
        .any(|e| nonempty(e.path().join("wireless")))
}

/// The access point on (`true`) or off: `pending` (written, netifd reloads), `unchanged`, or `saved` (written but
/// the reload failed: a reboot or `wifi reload` applies it). Refusals: `busy` while the radio is the client's or
/// other changes are staged; `not-found` with no access point; `failed` when turning the radio on would bring up
/// another interface on it as well.
pub(crate) async fn ap_set(net: &Net, on: bool) -> Result<&'static str> {
    if on && client::is_active(net).await {
        return Err(Error::busy(
            "the radio is the Wi-Fi client's (this driver cannot be both): leave that network first",
        ));
    }
    if staged(net).await {
        return Err(Error::busy("the wireless configuration has unsaved changes: save or revert them first"));
    }
    let w = Wireless::load(net).await?;
    let (radio, iface) = w.hotspot(&net.cfg().wlan).ok_or_else(|| Error::not_found("access point"))?;
    let cur_off = iface.get("disabled") == Some("1");
    let mut radio_too = false;
    if on && radio.get("disabled") == Some("1") {
        let other = w.ifaces().any(|o| {
            o.name != iface.name && o.get("device") == Some(radio.name.as_str()) && o.get("disabled") != Some("1")
        });
        if other {
            return Err(Error::failed(
                "the radio is off, and turning it on would bring up another interface on it as well",
            ));
        }
        radio_too = true;
    }
    if !on && cur_off {
        return Ok("unchanged");
    }
    // (on with nothing to write still reloads: an access point that did not come up gets another start)
    if cur_off == on || radio_too {
        let mut changes = vec![set(iface, "disabled", if on { "0" } else { "1" })];
        if radio_too {
            changes.push(set(radio, "disabled", "0"));
        }
        net.platform().uci().apply("wireless", &changes).await?;
    }
    Ok(if super::reload(net).await.is_ok() { "pending" } else { "saved" })
}

/// The Wi-Fi key: the other state.
pub(crate) async fn toggle(net: &Net) -> Result<&'static str> {
    let w = Wireless::load(net).await?;
    let (radio, iface) = w.hotspot(&net.cfg().wlan).ok_or_else(|| Error::not_found("access point"))?;
    let on = radio.get("disabled") == Some("1") || iface.get("disabled") == Some("1");
    ap_set(net, on).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_names() {
        assert_eq!(security(Some("psk2+ccmp")), "wpa2");
        assert_eq!(security(Some("psk-mixed")), "wpa2");
        assert_eq!(security(Some("sae-mixed")), "wpa2-wpa3");
        assert_eq!(security(Some("sae")), "wpa3");
        assert_eq!(security(None), "open");
        assert_eq!(security(Some("wep")), "other");
    }
}

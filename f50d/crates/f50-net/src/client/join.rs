//! Joining a network and leaving it, with the client lock held (one of these at a time, with the event handler).
//!
//! The order is the safety property: the firewall is `closed` before the supplicant starts; the address is checked
//! against the LAN before anything is shared; on the way out the event watcher goes first (or the supplicant's going
//! would look like a lost link to it), then the supplicant and the association, then the lease, and only then the
//! rules - wlan0 is never on the other network without them. A radio that cannot be got off it keeps them, closed.
//! Ported from `wifi-client connect`, `stop_client`, `share_uplink`, `lan_check`.

use std::net::Ipv4Addr;
use std::sync::atomic::Ordering;

use f50_core::jobs::JobCtx;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use super::share::{self, FwState};
use super::supplicant::{self, Joined};
use super::{State, lease, names::shown, radio, saved, watch};
use crate::{Net, lan, tools};

/// The marker a restarted daemon (and the hotspot) finds: the client holds the radio.
pub(crate) fn active_marker(net: &Net) -> std::path::PathBuf {
    net.run_dir().join("wifi-client.active")
}

fn set_active(net: &Net, on: bool) {
    net.inner.client_active.store(on, Ordering::SeqCst);
    if on {
        if f50_core::fsx::private_dir(&net.run_dir()).is_ok() {
            let _ = f50_core::fsx::write_atomic(&active_marker(net), b"", 0o600);
        }
    } else {
        let _ = std::fs::remove_file(active_marker(net));
    }
}

/// Why the address this network gave cannot be shared, or `None`. The LAN's configured subnet and, if br-lan is up
/// already, the one it has (a changed setting not applied yet).
pub(crate) async fn lan_check(net: &Net, uplink: Option<(Ipv4Addr, u8)>) -> Option<String> {
    let Some(lan) = lan::lan_net(net).await else {
        return Some(format!(
            "this device's own LAN subnet could not be worked out: not sharing {} with it",
            net.cfg().wlan
        ));
    };
    let up = uplink?;
    let live = tools::ipv4_of(&net.cfg().lan_bridge).await;
    if lan::overlaps(lan, up) || live.is_some_and(|l| lan::overlaps(l, up)) {
        let (n, p) = (lan::network(up.0, up.1), up.1);
        return Some(format!(
            "the network uses {n}/{p}, which overlaps this device's own LAN ({}/{}): not joined. Give the LAN another \
             subnet or join another network.",
            lan.0, lan.1
        ));
    }
    None
}

/// Shared, with the subnet DHCP gave; a failure ends closed, or off the network when even that cannot be confirmed.
pub(crate) async fn share(net: &Net, subnet: Option<(Ipv4Addr, u8)>) -> Result<()> {
    let r = match share::wan_zone(net).await {
        Ok(()) => share::put(FwState::Shared, &net.cfg().wlan, subnet).await,
        Err(e) => Err(e),
    };
    if let Err(e) = r {
        fail_closed(net).await;
        return Err(e.context(format!("could not share {}", net.cfg().wlan)));
    }
    Ok(())
}

/// Closed if that can be confirmed; if not, the lease and its routes go, and the radio leaves the network, so that
/// nothing is forwarded to wlan0 whatever the tables are. `false`: it was not confirmed.
pub(crate) async fn fail_closed(net: &Net) -> bool {
    if share::put(FwState::Closed, &net.cfg().wlan, None).await.is_ok() {
        return true;
    }
    lease::drop_all(net).await;
    let _ = supplicant::leave(net).await;
    tools::logger(
        "wifi-client",
        &format!(
            "the firewall for {} could not be confirmed (nft): it has left the network, nothing is shared",
            net.cfg().wlan
        ),
    )
    .await;
    false
}

/// Everything the client set up goes, in order. See the module.
pub(crate) async fn stop_client(net: &Net, st: &mut State) -> Result<()> {
    set_active(net, false);
    if let Some(w) = st.watcher.take() {
        w.cancel();
    }
    let ifname = net.cfg().wlan.clone();
    if let Err(e) = supplicant::leave(net).await {
        let _ = share::put(FwState::Closed, &ifname, None).await;
        lease::drop_all(net).await;
        return Err(e.context(format!("{ifname} is still on the network: its firewall stays closed")));
    }
    lease::drop_all(net).await;
    share::remove(&ifname).await;
    Ok(())
}

fn log(ctx: Option<&JobCtx>, line: &str) {
    if let Some(c) = ctx {
        c.log(line);
    }
}

/// DHCP on the joined network, the address checked, then shared. `Err` leaves the client closed.
pub(crate) async fn uplink_up(net: &Net, metric: u32) -> Result<lease::Lease> {
    lease::drop_all(net).await;
    let l = lease::request(net).await?.ok_or_else(|| Error::failed("no address from the network (no DHCP server?)"))?;
    lease::apply(net, &l, metric).await?;
    if let Some(why) = lan_check(net, l.net()).await {
        lease::drop_all(net).await;
        return Err(Error::failed(why));
    }
    share(net, l.net()).await?;
    Ok(l)
}

/// Joins `ssid` (raw bytes as typed) with `psk` (`None`: an open network) and saves it. From the moment the radio is
/// taken, every way out but success takes the client down again, rules and all, and gives the hotspot back if it was
/// on.
pub(crate) async fn connect(
    net: &Net,
    st: &mut State,
    ssid: &str,
    psk: Option<&str>,
    ctx: Option<&JobCtx>,
) -> Result<Value> {
    stop_client(net, st).await.map_err(|e| e.context("not joining another network"))?;
    if let Err(e) = radio::prepare_station(net).await {
        saved::save(net, ssid, psk)?;
        return Err(Error::failed(format!(
            "{}. The network is saved: reboot and the device joins it before the hotspot starts (leave with forget to \
             undo it).",
            e.message()
        )));
    }
    set_active(net, true);
    match joined(net, st, ssid, psk, ctx).await {
        Ok(v) => Ok(v),
        Err(e) => {
            let _ = stop_client(net, st).await;
            if radio::had_ap(net) {
                let _ = radio::start_ap(net).await;
            }
            Err(e)
        }
    }
}

async fn joined(net: &Net, st: &mut State, ssid: &str, psk: Option<&str>, ctx: Option<&JobCtx>) -> Result<Value> {
    let ifname = net.cfg().wlan.clone();
    share::put(FwState::Closed, &ifname, None)
        .await
        .map_err(|e| e.context(format!("could not put up the firewall for {ifname}: not joining without it")))?;
    let (ctl, can_sae) = supplicant::start(net).await?;
    let sae = can_sae && psk.is_some();
    log(ctx, if sae { "the supplicant can do SAE" } else { "the supplicant cannot do SAE: WPA2 only" });
    supplicant::configure(&ctl, ssid.as_bytes(), psk, sae).await?;
    let name = shown(ssid.as_bytes());
    match supplicant::wait_joined(&ctl, net.cfg().join_wait.as_secs(), ssid.as_bytes(), sae).await {
        Joined::Yes => {}
        Joined::SaeOnly => {
            return Err(Error::failed(format!(
                "'{name}' is a WPA3-only network (SAE), and this Wi-Fi driver cannot do WPA3, so it cannot be joined. \
                 Set the router to WPA2/WPA3 mixed mode (or WPA2), or join another network."
            )));
        }
        Joined::No => return Err(Error::failed(format!("could not join '{name}' (wrong password, or out of range)"))),
    }
    drop(ctl);
    log(ctx, &format!("joined {name}, asking for an address"));
    let metric = saved::load(net).filter(|s| s.ssid == ssid).map_or(net.cfg().client_metric, |s| s.metric);
    let l = uplink_up(net, metric).await.map_err(|e| e.context(&name))?;
    saved::save(net, ssid, psk)?;
    st.watcher = Some(watch::spawn(net, metric));
    let addr = l.ip.map(|a| format!("{a}/{}", l.prefix));
    log(ctx, &format!("connected: {name} {}", addr.as_deref().unwrap_or("")));
    Ok(json!({ "joined": true, "ssid": name, "address": addr }))
}

/// `f50.wificlient status`: from the supplicant, the lease, nft and the saved file, never from what was last done.
pub(crate) async fn status(net: &Net) -> Value {
    let ifname = net.cfg().wlan.clone();
    let mut v = json!({ "mode": "off", "connected": false, "ssid": null, "signal": null, "address": null });
    if let Some(c) = supplicant::connect(net).await {
        let s = c.request("STATUS", std::time::Duration::from_secs(3)).await.unwrap_or_default();
        if let Some(ssid) = super::ctrl::field(&s, "ssid") {
            v["mode"] = json!("client");
            v["ssid"] = json!(super::names::shown_escaped(ssid));
            v["connected"] = json!(super::ctrl::field(&s, "wpa_state") == Some("COMPLETED"));
            let sig = c.request("SIGNAL_POLL", std::time::Duration::from_secs(3)).await.unwrap_or_default();
            v["signal"] = json!(super::ctrl::field(&sig, "RSSI").and_then(|r| r.parse::<i32>().ok()));
            v["address"] = json!(tools::ipv4_of(&ifname).await.map(|(a, p)| format!("{a}/{p}")));
        }
    }
    if v["mode"] == "off" && radio::ap_active(net).await {
        v["mode"] = json!("hotspot");
    }
    let subnet = lease::read(net).and_then(|l| l.net());
    let fw = share::state(&ifname, subnet).await;
    v["firewall"] = json!(fw);
    let active = super::is_active(net).await;
    v["active"] = json!(active);
    // OPEN: the client is up but the filter table is gone (nft flush?); left over: tables of a client that is gone
    v["firewall_ok"] = json!(match fw {
        FwState::None => !active,
        FwState::Unknown => false,
        _ => active,
    });
    v["saved"] = match saved::load(net) {
        Some(s) => json!({ "ssid": shown(s.ssid.as_bytes()), "at_boot": s.enable }),
        None => Value::Null,
    };
    v
}

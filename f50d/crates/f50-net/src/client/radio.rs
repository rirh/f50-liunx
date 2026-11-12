//! Who has the radio: the hotspot or the client.
//!
//! The SC2355 does one thing at a time, and the switch only goes one way (FINDINGS 14b): the driver creates wlan0 in
//! station mode, hostapd turns it into an access point, and nothing turns it back - `iw set type` is refused, and
//! deleting the interface took a WCN power path that does not work on this board (on 5.4). So the mode is decided
//! at boot: with a network saved and enabled, the access point stays off for that boot ([`keep_for_client`]) and the
//! client joins while wlan0 is still a station. Going back to the hotspot works without a reboot ([`start_ap`]),
//! because that is the direction hostapd can do.

use std::time::Duration;

use f50_core::{Error, Result};

use super::saved;
use crate::wifi::{self, Wireless};
use crate::{Net, tools};

/// The hotspot was on when the client took the radio: leaving puts it back.
fn ap_flag(net: &Net) -> std::path::PathBuf {
    net.run_dir().join("wifi-client-had-ap")
}

fn mark_ap(net: &Net) {
    if f50_core::fsx::private_dir(&net.run_dir()).is_ok() {
        let _ = f50_core::fsx::write_atomic(&ap_flag(net), b"", 0o600);
    }
}

pub(crate) fn had_ap(net: &Net) -> bool {
    ap_flag(net).exists()
}

/// The access point is running: ask the interface and hostapd, not only the configuration.
pub(crate) async fn ap_active(net: &Net) -> bool {
    if tools::iface_type(&net.cfg().wlan).await.as_deref() == Some("AP") {
        return true;
    }
    net.platform().has_object(&format!("hostapd.{}", net.cfg().wlan)).await.unwrap_or(false)
}

/// The access point off in the configuration (netifd would put it back on the same radio at the next reload).
async fn disable_ap(net: &Net) -> Result<bool> {
    let w = Wireless::load(net).await?;
    let Some((_, ap)) = w.hotspot(&net.cfg().wlan) else { return Ok(false) };
    if ap.get("disabled") == Some("1") {
        return Ok(false);
    }
    net.platform().uci().apply("wireless", &[wifi::set(ap, "disabled", "1")]).await.map(|()| true)
}

/// At boot, before the Wi-Fi comes up: a saved, enabled network keeps the access point off for this boot, so the
/// radio stays a station; the hotspot comes back when the join fails or the network is left.
pub(crate) async fn keep_for_client(net: &Net) -> Result<bool> {
    if !saved::load(net).is_some_and(|s| s.enable) {
        return Ok(false);
    }
    let changed = disable_ap(net).await?;
    if changed {
        mark_ap(net);
    }
    Ok(changed)
}

/// Stops the hotspot when it runs on a station interface (remembering it was on); the hotspot leaves the interface
/// in the LAN bridge, and a client interface must not be in one.
async fn stop_ap(net: &Net) {
    if !ap_active(net).await {
        return;
    }
    mark_ap(net);
    if let Err(e) = disable_ap(net).await {
        tracing::warn!(error = %e, "could not switch the access point off in UCI");
    }
    let _ = tools::ubus(net, "network.wireless", "down", &serde_json::json!({})).await;
    tools::ip_quiet(&["link", "set", &net.cfg().wlan, "nomaster"]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
}

/// The radio ready for the client, or `Err` when it is an access point (this driver cannot go back without a
/// reboot: stopping the hotspot would only leave the device with no Wi-Fi at all).
pub(crate) async fn prepare_station(net: &Net) -> Result<()> {
    let ifname = net.cfg().wlan.as_str();
    if tools::iface_type(ifname).await.as_deref() != Some("managed") {
        return Err(Error::failed(
            "the radio is an access point right now, and this driver cannot turn it back into a client without a reboot",
        ));
    }
    stop_ap(net).await;
    let _ = f50_core::exec::Cmd::new("rfkill").args(["unblock", "wifi"]).timeout(Duration::from_secs(5)).output().await;
    tools::ip_quiet(&["link", "set", ifname, "nomaster"]).await;
    tools::ip_quiet(&["link", "set", ifname, "up"]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

/// The hotspot back: its interface on in UCI, netifd reloads (hostapd turns the station into an access point).
pub(crate) async fn start_ap(net: &Net) -> Result<()> {
    let _ = std::fs::remove_file(ap_flag(net));
    let w = Wireless::load(net).await?;
    if let Some((_, ap)) = w.hotspot(&net.cfg().wlan)
        && ap.get("disabled") == Some("1")
    {
        net.platform().uci().apply("wireless", &[wifi::set(ap, "disabled", "0")]).await?;
    }
    wifi::reload(net).await
}

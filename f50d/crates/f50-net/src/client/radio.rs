//! Who has the radio: the hotspot or the client.
//!
//! The SC2355 does one thing at a time, and the switch only goes one way (FINDINGS 14b): the driver creates wlan0 in
//! station mode, hostapd turns it into an access point, and nothing turns it back - `iw set type` is refused, and
//! deleting the interface took a WCN power path that does not work on this board (on 5.4). So the mode is decided
//! at boot: with a network saved and enabled, the access point stays off for that boot ([`keep_for_client`]) and the
//! client joins while wlan0 is still a station. Going back to the hotspot works without a reboot ([`start_ap`]),
//! because that is the direction hostapd can do.
//!
//! Scan mode is the same trick for a device with nothing saved: [`arm_scan`] asks the next boot (once) to keep the
//! access point off, so wlan0 stays a station and can scan; the hotspot comes back when a network is left, when the
//! window ([`SCAN_WINDOW`]) passes with nothing joined, or at the boot after.

use std::time::Duration;

use f50_core::{Error, Result};

use super::saved;
use crate::wifi::{self, Wireless};
use crate::{Net, tools};

/// How long a scan-mode boot keeps the hotspot off when nothing is joined.
pub(crate) const SCAN_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Scan mode asked for the next boot (in `/etc/f50`, so it survives the reboot; that boot removes it).
fn scan_next_flag(net: &Net) -> std::path::PathBuf {
    net.legacy("wifi-client.scan-next-boot")
}

/// This boot is a scan-mode boot: holds the Unix time the window ends (in the run directory, gone at the next boot).
fn scan_mode_flag(net: &Net) -> std::path::PathBuf {
    net.run_dir().join("wifi-client-scan-mode")
}

/// The next boot keeps the hotspot off so the radio can scan (once).
pub(crate) fn arm_scan(net: &Net) -> Result<()> {
    let p = scan_next_flag(net);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::io(d.display(), &e))?;
    }
    f50_core::fsx::write_atomic(&p, b"", 0o600)
}

pub(crate) fn scan_armed(net: &Net) -> bool {
    scan_next_flag(net).exists()
}

/// When this boot's scan window ends (Unix seconds), while it is a scan-mode boot.
pub(crate) fn scan_until(net: &Net) -> Option<u64> {
    std::fs::read_to_string(scan_mode_flag(net)).ok()?.trim().parse().ok()
}

pub(crate) fn end_scan_mode(net: &Net) {
    let _ = std::fs::remove_file(scan_mode_flag(net));
}

/// Takes the armed flag (removed first: a boot that goes wrong never repeats it) and starts the window.
fn take_scan(net: &Net) -> bool {
    if std::fs::remove_file(scan_next_flag(net)).is_err() {
        return false;
    }
    let until = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .saturating_add(SCAN_WINDOW.as_secs());
    if f50_core::fsx::private_dir(&net.run_dir()).is_ok() {
        let _ = f50_core::fsx::write_atomic(&scan_mode_flag(net), until.to_string().as_bytes(), 0o600);
    }
    true
}

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
    if take_scan(net) {
        tracing::info!("scan mode: the access point stays off for this boot so the radio can scan");
        let changed = disable_ap(net).await?;
        mark_ap(net);
        return Ok(changed);
    }
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

/// The hotspot is off in the configuration (scan mode, a saved network kept the radio, or turned off by hand).
pub(crate) async fn ap_disabled(net: &Net) -> bool {
    let Ok(w) = Wireless::load(net).await else { return false };
    w.hotspot(&net.cfg().wlan).is_some_and(|(_, ap)| ap.get("disabled") == Some("1"))
}

/// netifd is still setting a radio up (or down).
async fn wireless_pending(net: &Net) -> bool {
    let Ok(v) = tools::ubus(net, "network.wireless", "status", &serde_json::json!({})).await else { return false };
    v.as_object().is_some_and(|radios| radios.values().any(|r| r["pending"].as_bool() == Some(true)))
}

/// The radio's phy (`phy0`): the first in `/sys/class/ieee80211`.
fn phy(net: &Net) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(net.path("/sys/class/ieee80211"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// wlan0 as a station when it is missing. netifd's setup of a radio with no enabled interface deletes every
/// interface on the phy (`iface 'wlan0' deleted`; the driver then powers the chip down), so with the hotspot off
/// nothing is left to scan or join with. `iw phy … interface add` makes it again as a station (the driver powers
/// the chip back up). Waits until netifd is done with the radio first, and makes it again if that setup still
/// deleted it.
pub(crate) async fn ensure_station(net: &Net) -> Result<()> {
    let ifname = net.cfg().wlan.clone();
    for attempt in 0..3 {
        if tools::iface_type(&ifname).await.is_some() {
            return Ok(());
        }
        for _ in 0..30 {
            if !wireless_pending(net).await {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let Some(phy) = phy(net) else { return Err(Error::failed("no Wi-Fi radio (no phy in /sys/class/ieee80211)")) };
        tracing::info!(%phy, %ifname, attempt, "the Wi-Fi interface is gone (hotspot off): making it again as a station");
        tools::iw(&["phy", &phy, "interface", "add", &ifname, "type", "managed"], Duration::from_secs(10)).await?;
        // netifd may still be about to set the radio up: give it the time it took to delete the last one
        tokio::time::sleep(Duration::from_secs(if attempt == 0 { 3 } else { 6 })).await;
    }
    if tools::iface_type(&ifname).await.is_some() {
        Ok(())
    } else {
        Err(Error::failed(format!("{ifname} could not be made again as a station")))
    }
}

/// The radio ready for the client, or `Err` when it is an access point (this driver cannot go back without a
/// reboot: stopping the hotspot would only leave the device with no Wi-Fi at all).
pub(crate) async fn prepare_station(net: &Net) -> Result<()> {
    let ifname = net.cfg().wlan.as_str();
    if !ap_active(net).await {
        ensure_station(net).await?;
    }
    if tools::iface_type(ifname).await.as_deref() != Some("managed") {
        return Err(Error::failed(
            "the radio is the hotspot now and cannot scan or join until a reboot (scan_reboot boots with the hotspot off)",
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
    end_scan_mode(net);
    let w = Wireless::load(net).await?;
    if let Some((_, ap)) = w.hotspot(&net.cfg().wlan)
        && ap.get("disabled") == Some("1")
    {
        net.platform().uci().apply("wireless", &[wifi::set(ap, "disabled", "0")]).await?;
    }
    wifi::reload(net).await
}

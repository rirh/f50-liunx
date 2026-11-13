//! The Wi-Fi client: wlan0 joined to somebody else's network as the device's uplink, shared with the LAN.
//!
//! - [`ctrl`]: wpa_supplicant's control socket; [`supplicant`]: start, configure, wait, leave;
//! - [`scan`] and [`names`]: the networks in range, their names as they may be shown;
//! - [`lease`]: DHCP through udhcpc; [`share`]: the nft tables and fw4's wan zone;
//! - [`join`]: join and leave in their safe order; [`watch`]: the link and the lease while joined;
//! - [`radio`]: who has the radio (FINDINGS 14b); [`saved`]: the saved network.
//!
//! The Wi-Fi route gets a lower metric than mobile data (50 against 100), so while Wi-Fi is connected the device
//! uses it and the SIM stays idle; losing the Wi-Fi falls back to the modem by itself. Ported from `wifi-client` (F50
//! on OpenWrt: the Ubuntu paths, the scan mode of systemd's timers and the VPN kill-switch hooks are not carried over).

mod ctrl;
mod join;
mod lease;
mod names;
mod radio;
mod saved;
mod scan;
mod share;
mod supplicant;
mod watch;

use std::sync::atomic::Ordering;
use std::time::Duration;

use f50_core::jobs::JobSpec;
use f50_core::rpc::Params;
use f50_core::{Error, Result};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{Net, rules};

/// What the client lock guards.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The running session's watcher.
    watcher: Option<CancellationToken>,
}

/// The client holds the radio (this daemon's join, or a previous daemon's that left its marker and its supplicant).
pub(crate) async fn is_active(net: &Net) -> bool {
    net.inner.client_active.load(Ordering::SeqCst) || join::active_marker(net).exists()
}

/// See [`Net::boot_wifi_config`]: the access point off for this boot when a saved network is to be joined.
pub(crate) async fn keep_radio_for_client(net: &Net) -> Result<bool> {
    radio::keep_for_client(net).await
}

/// See [`Net::hotspot_disabled`].
pub(crate) async fn hotspot_disabled(net: &Net) -> bool {
    radio::ap_disabled(net).await
}

/// See [`Net::boot_wifi_client`].
pub(crate) async fn boot(net: &Net) -> Result<()> {
    // the hotspot off for this boot (scan mode, a saved network) leaves no interface once netifd set the radio up
    if radio::ap_disabled(net).await
        && let Err(e) = radio::ensure_station(net).await
    {
        tracing::warn!(error = %e, "Wi-Fi station interface");
    }
    if radio::scan_until(net).is_some() {
        tracing::info!("scan mode: the hotspot stays off for ten minutes unless a network is joined");
        tokio::spawn(scan_window(net.clone()));
    }
    let Some(s) = saved::load(net) else { return Ok(()) };
    if !s.enable {
        tracing::info!("the saved Wi-Fi network is not joined at boot since it was disconnected");
        return Ok(());
    }
    let mut st = net.inner.client.lock().await;
    match join::connect(net, &mut st, &s.ssid, s.psk.as_deref(), None).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // the hotspot again, so a device reached over it is reachable again (connect gave it back if it had
            // taken it; the boot may have kept it off without running it)
            // (not in scan mode: the radio stays a station there, so another network can be picked)
            if radio::had_ap(net) && radio::scan_until(net).is_none() {
                let _ = radio::start_ap(net).await;
            }
            Err(e)
        }
    }
}

/// A scan-mode boot's window: when it passes with no network joined, the hotspot comes back; a network joined
/// meanwhile keeps the radio (the hotspot then returns when it is left, as after any join).
async fn scan_window(net: Net) {
    tokio::time::sleep(radio::SCAN_WINDOW).await;
    if radio::scan_until(&net).is_none() {
        return;
    }
    let _st = net.inner.client.lock().await;
    if is_active(&net).await {
        radio::end_scan_mode(&net);
        return;
    }
    tracing::info!("scan mode over with no network joined: the hotspot comes back");
    if let Err(e) = radio::start_ap(&net).await {
        tracing::warn!(error = %e, "hotspot after scan mode");
    }
}

/// `f50.wificlient scan_reboot`: the next boot keeps the hotspot off for [`radio::SCAN_WINDOW`] so the radio can
/// scan (this driver cannot leave access-point mode without a reboot), then reboots in two seconds.
pub(crate) fn scan_reboot(net: &Net) -> Result<Value> {
    radio::arm_scan(net)?;
    net.platform().system().reboot_after(Duration::from_secs(2))?;
    Ok(json!({ "reboot_in_s": 2, "window_s": radio::SCAN_WINDOW.as_secs() }))
}

/// `f50.wificlient status`.
pub(crate) async fn status(net: &Net) -> Result<Value> {
    Ok(join::status(net).await)
}

/// `f50.wificlient scan`: at once from the supplicant when it holds the interface (about 4 s); otherwise `iw scan`
/// (up to three tries of 25 s) as a job of kind `wifi`.
pub(crate) async fn scan(net: &Net) -> Result<Value> {
    if let Some(c) = supplicant::connect(net).await {
        c.ok("SCAN").await.or_else(|e| if e.message().contains("refused") { Ok(()) } else { Err(e) })?;
        tokio::time::sleep(Duration::from_secs(4)).await;
        let r = c.request("SCAN_RESULTS", Duration::from_secs(5)).await?;
        let entries =
            scan::parse_wpa(&r).into_iter().map(|e| (e.signal, scan::wpa_security(&e.flags), e.raw)).collect();
        return Ok(json!({ "networks": scan::list(entries) }));
    }
    let n = net.clone();
    let id = net.inner.jobs.start(
        JobSpec::new("f50.wificlient scan", "wifi", Duration::from_secs(120)),
        move |ctx| async move {
            let _st = n.inner.client.lock().await;
            radio::prepare_station(&n).await?;
            ctx.log("scanning");
            let nets = scan::iw_scan(&n.cfg().wlan).await?;
            Ok(json!({ "networks": nets }))
        },
    )?;
    Ok(json!({ "job": id }))
}

/// `f50.wificlient join`: the name 1-32 bytes with no control character and no space at either end; the password
/// a WPA passphrase (8-63 printable ASCII) or a raw key (64 hex digits), required unless `open`. A job of kind
/// `wifi` (not cancellable half way: a join stopped there would leave the radio taken and the firewall closed).
pub(crate) async fn join(net: &Net, p: &Params) -> Result<Value> {
    let ssid = p.need_str("ssid")?;
    if !rules::join_ssid_ok(ssid) {
        return Err(Error::bad_value("ssid"));
    }
    let open = p.flag("open");
    let psk = match p.str("password") {
        Some(pw) if rules::psk_ok(pw) || rules::raw_key_ok(pw) => Some(pw.to_owned()),
        Some(_) => return Err(Error::bad_value("password")),
        None if open => None,
        None => return Err(Error::missing("password")),
    };
    let (n, ssid) = (net.clone(), ssid.to_owned());
    let spec = JobSpec::new("f50.wificlient join", "wifi", Duration::from_secs(180)).not_cancellable();
    let id = net.inner.jobs.start(spec, move |ctx| async move {
        let mut st = n.inner.client.lock().await;
        join::connect(&n, &mut st, &ssid, psk.as_deref(), Some(&ctx)).await
    })?;
    Ok(json!({ "job": id }))
}

/// `f50.wificlient leave`: off the network and the hotspot back; the saved network is no longer joined at boot
/// (`keep`: it still is; `forget`: it is deleted).
pub(crate) async fn leave(net: &Net, p: &Params) -> Result<Value> {
    let (forget, keep) = (p.flag("forget"), p.flag("keep"));
    let n = net.clone();
    let id = net.inner.jobs.start(
        JobSpec::new("f50.wificlient leave", "wifi", Duration::from_secs(120)),
        move |ctx| async move {
            let mut st = n.inner.client.lock().await;
            join::stop_client(&n, &mut st).await?;
            if forget {
                saved::forget(&n);
            } else if !keep {
                // without this the next boot joined the network again and left the device with no hotspot
                saved::set_enable(&n, false)?;
            }
            ctx.log("off the network; the hotspot comes back");
            radio::start_ap(&n).await?;
            Ok(json!({ "left": true, "forgotten": forget }))
        },
    )?;
    Ok(json!({ "job": id }))
}

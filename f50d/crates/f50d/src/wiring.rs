//! What connects the capabilities to each other, kept out of every capability crate (they never depend on their
//! siblings): the LED inputs, the keys' actions, and the network's boot sequence.

use std::sync::Arc;
use std::time::Duration;

use f50_api::Services;
use f50_modem::{RadioSnapshot, Rat, SimState};
use f50_platform::Platform;
use f50_system::{ButtonAction, LedInputs, NetworkLed};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;

const LED_EVERY: Duration = Duration::from_secs(5);
/// The boot indicator ends when the boot is done; f50d counts it done this long after it started.
const BOOTED_AFTER: Duration = Duration::from_secs(60);

pub fn start(s: &Services, platform: &Platform, cfg: &Config, tracker: &TaskTracker, shutdown: &CancellationToken) {
    tracker.spawn(leds(s.clone(), platform.clone(), shutdown.clone()));
    tracker.spawn(keys(s.clone(), cfg.factory_reset, shutdown.clone()));
    tracker.spawn(net_boot(s.clone(), shutdown.clone()));
    if cfg.adbd {
        s.net.adbd_start();
    }
}

fn network_led(snap: &RadioSnapshot, sim: Option<SimState>) -> NetworkLed {
    if sim != Some(SimState::Ready) {
        return NetworkLed::None;
    }
    if !snap.registered {
        return NetworkLed::Searching;
    }
    match snap.rat {
        Some(Rat::Gsm) => NetworkLed::G2,
        Some(Rat::Umts) => NetworkLed::G3,
        Some(Rat::Lte) => NetworkLed::G4,
        Some(Rat::Nsa | Rat::Sa) => NetworkLed::G5,
        None => NetworkLed::Searching,
    }
}

/// The LEDs follow the modem's snapshot (as it changes) and the links (every few seconds).
async fn leds(s: Services, platform: Platform, shutdown: CancellationToken) {
    let started = tokio::time::Instant::now();
    let mut snap = s.modem.snapshot();
    let mut tick = tokio::time::interval(LED_EVERY);
    loop {
        let current: Arc<RadioSnapshot> = snap.borrow_and_update().clone();
        let data_up = platform.netifd().is_up("wan").await.unwrap_or(false);
        // hostapd's word, not wlan0's operstate: the F50's driver need not report a running access point "up", and
        // the Wi-Fi client's station is "up" too
        let wifi_on = s.net.hotspot_up().await;
        s.system.set_led_inputs(LedInputs {
            network: network_led(&current, s.modem.sim_state()),
            bars: current.signal.as_ref().map(|x| x.bars),
            data_up,
            wifi_on,
            wifi_clients: 0,
            idle: false,
            booted: started.elapsed() >= BOOTED_AFTER,
        });
        tokio::select! {
            () = shutdown.cancelled() => return,
            r = snap.changed() => if r.is_err() { return },
            _ = tick.tick() => {}
        }
    }
}

/// The keys: the Wi-Fi key toggles the hotspot; the factory reset only when `/etc/config/f50d` allows it.
async fn keys(s: Services, factory_reset: bool, shutdown: CancellationToken) {
    let mut actions = s.system.button_actions();
    loop {
        let a = tokio::select! {
            () = shutdown.cancelled() => return,
            a = actions.recv() => a,
        };
        match a {
            Ok(ButtonAction::WifiToggle) => match s.net.hotspot_toggle().await {
                Ok(state) => tracing::info!(state, "Wi-Fi key: hotspot toggled"),
                Err(e) => tracing::warn!(error = %e, "Wi-Fi key: hotspot not toggled"),
            },
            Ok(ButtonAction::FactoryReset) if factory_reset => factory_reset_now().await,
            Ok(ButtonAction::FactoryReset) => {
                tracing::warn!("reset key held: factory reset is off (f50d.main.factory_reset)");
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// OpenWrt's own reset: `firstboot -y` marks the settings for wiping, then a reboot.
async fn factory_reset_now() {
    tracing::warn!("reset key held: factory reset");
    let r = f50_core::exec::Cmd::new("/sbin/firstboot").arg("-y").timeout(Duration::from_secs(60)).ok().await;
    match r {
        Ok(_) => {
            let _ = f50_core::exec::Cmd::new("/sbin/reboot").timeout(Duration::from_secs(10)).run().await;
        }
        Err(e) => tracing::error!(error = %e, "firstboot failed; nothing was wiped"),
    }
}

/// The network's part of the boot, in the order the shell's init scripts had it.
async fn net_boot(s: Services, shutdown: CancellationToken) {
    let net = s.net;
    let step = async move {
        match net.boot_wifi_config().await {
            Ok(true) => {
                tracing::info!("wireless configuration prepared for this boot");
                // netifd may have read /etc/config/wireless already (it starts first): without a reload it never
                // learns of the radio, and the hotspot stays down until something restarts the Wi-Fi
                if let Err(e) =
                    f50_core::exec::Cmd::new("/sbin/wifi").arg("reload").timeout(Duration::from_secs(60)).run().await
                {
                    tracing::warn!(error = %e, "wifi reload after the boot configuration");
                }
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "wireless boot configuration"),
        }
        if let Err(e) = net.boot_usb().await {
            tracing::warn!(error = %e, "USB boot mode");
        }
        // the Wi-Fi client needs wlan0, which netifd makes
        for _ in 0..60 {
            if std::path::Path::new("/sys/class/net/wlan0").exists() {
                if let Err(e) = net.boot_wifi_client().await {
                    tracing::warn!(error = %e, "Wi-Fi client at boot");
                }
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        // the one Wi-Fi restart: for a radio that failed, and for one still not up after the wait (netifd that
        // never learnt of it waits for ever)
        let mut restart = true;
        for _ in 0..45 {
            match net.wifi_ready().await {
                f50_net::WifiReady::Ready => {
                    restart = false;
                    break;
                }
                f50_net::WifiReady::Failed => break,
                f50_net::WifiReady::Waiting => tokio::time::sleep(Duration::from_secs(2)).await,
            }
        }
        if restart {
            tracing::warn!("Wi-Fi did not come up; restarting it once");
            let _ = f50_core::exec::Cmd::new("/sbin/wifi").timeout(Duration::from_secs(60)).run().await;
        }
        net.usb_rebind_if_no_lease(Duration::from_secs(90));
    };
    tokio::select! {
        () = shutdown.cancelled() => {}
        () = step => {}
    }
}

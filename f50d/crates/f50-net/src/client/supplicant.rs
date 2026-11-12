//! wpa_supplicant on wlan0: started with an empty network list, given the network over its control socket (the
//! passphrase never touches a file or a command line), asked whether it joined, and taken off the network again.
//!
//! WPA2 networks work, and so do WPA2/WPA3 mixed ones (as WPA2). WPA3-only (SAE) needs SAE from the driver as well as
//! from the supplicant, and the SC2355's has none (`GET_CAPABILITY key_mgmt` leaves SAE out; the access point rejects
//! an association with status 1, FINDINGS 14b), so such a network is refused as soon as the supplicant's scan shows
//! that is what it is. A driver that offers SAE gets it.

use std::path::PathBuf;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result, fsx};

use super::ctrl::{self, Ctrl};
use super::scan;
use crate::{Net, tools};

fn conf_path(net: &Net) -> PathBuf {
    net.run_dir().join("wifi-client.wpa.conf")
}

fn pid_path(net: &Net) -> PathBuf {
    net.run_dir().join("wifi-client.wpa.pid")
}

/// A connection to the supplicant of wlan0, if one answers.
pub(crate) async fn connect(net: &Net) -> Option<Ctrl> {
    let c = Ctrl::open(&net.path(&net.cfg().wpa_ctrl_dir), &net.cfg().wlan, &net.run_dir()).await.ok()?;
    let pong = c.request("PING", Duration::from_secs(2)).await.ok()?;
    (pong.trim() == "PONG").then_some(c)
}

pub(crate) async fn running(net: &Net) -> bool {
    connect(net).await.is_some()
}

/// Starts it with no network; whether this side can do SAE (only the running supplicant can say: it depends on the
/// driver too).
pub(crate) async fn start(net: &Net) -> Result<(Ctrl, bool)> {
    fsx::private_dir(&net.run_dir())?;
    let ctrl_dir = net.path(&net.cfg().wpa_ctrl_dir);
    let conf = format!("ctrl_interface={}\n", ctrl_dir.display());
    fsx::write_atomic(&conf_path(net), conf.as_bytes(), 0o600)?;
    let c = conf_path(net).to_string_lossy().into_owned();
    let p = pid_path(net).to_string_lossy().into_owned();
    Cmd::new("wpa_supplicant")
        .args(["-B", "-i", &net.cfg().wlan, "-c", &c, "-P", &p])
        .leaves_daemon()
        .timeout(Duration::from_secs(15))
        .ok()
        .await
        .map_err(|e| e.context("wpa_supplicant did not start"))?;
    let mut caps = String::new();
    let mut ctl = None;
    for _ in 0..5 {
        if let Some(cc) = connect(net).await {
            caps = cc.request("GET_CAPABILITY key_mgmt", Duration::from_secs(3)).await.unwrap_or_default();
            ctl = Some(cc);
            if !caps.trim().is_empty() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let ctl = ctl.ok_or_else(|| Error::failed("wpa_supplicant does not answer on its control socket"))?;
    let sae = caps.split_whitespace().any(|k| k == "SAE");
    Ok((ctl, sae))
}

/// The network block, over the socket: the name in hex (any byte as it is), the passphrase quoted. With SAE (WPA2,
/// WPA2/WPA3 and WPA3-only alike) management frame protection and `sae_pwe 2` (hunting-and-pecking or
/// hash-to-element, whichever the access point uses; 6 GHz needs H2E). `psk`: `None` for an open network.
pub(crate) async fn configure(c: &Ctrl, ssid: &[u8], psk: Option<&str>, sae: bool) -> Result<()> {
    if sae {
        c.ok("SET sae_pwe 2").await?;
    }
    let id = c.request("ADD_NETWORK", Duration::from_secs(5)).await?;
    let id = id.trim();
    if !id.bytes().all(|b| b.is_ascii_digit()) || id.is_empty() {
        return Err(Error::failed("wpa_supplicant refused ADD_NETWORK"));
    }
    c.ok(&format!("SET_NETWORK {id} ssid {}", ctrl::hex(ssid))).await?;
    match psk {
        None => c.ok(&format!("SET_NETWORK {id} key_mgmt NONE")).await?,
        Some(k) => {
            let v = if crate::rules::raw_key_ok(k) { k.to_owned() } else { ctrl::quoted(k) };
            c.ok(&format!("SET_NETWORK {id} psk {v}")).await?;
            if sae {
                c.ok(&format!("SET_NETWORK {id} key_mgmt WPA-PSK SAE")).await?;
                c.ok(&format!("SET_NETWORK {id} ieee80211w 1")).await?;
                if !crate::rules::raw_key_ok(k) {
                    c.ok(&format!("SET_NETWORK {id} sae_password {}", ctrl::quoted(k))).await?;
                }
            } else {
                c.ok(&format!("SET_NETWORK {id} key_mgmt WPA-PSK")).await?;
            }
        }
    }
    c.ok(&format!("SELECT_NETWORK {id}")).await
}

/// How a wait for the join ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Joined {
    Yes,
    No,
    /// Every access point of the name is WPA3-only and this side cannot do SAE.
    SaeOnly,
}

pub(crate) async fn wait_joined(c: &Ctrl, secs: u64, ssid: &[u8], sae: bool) -> Joined {
    for _ in 0..secs.max(1) {
        let st = c.request("STATUS", Duration::from_secs(3)).await.unwrap_or_default();
        if ctrl::field(&st, "wpa_state") == Some("COMPLETED") {
            return Joined::Yes;
        }
        if !sae {
            let r = c.request("SCAN_RESULTS", Duration::from_secs(3)).await.unwrap_or_default();
            if scan::sae_only(&scan::parse_wpa(&r), ssid) {
                return Joined::SaeOnly;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Joined::No
}

/// `iw dev IF link` says it is associated.
pub(crate) async fn associated(net: &Net) -> bool {
    tools::iw(&["dev", &net.cfg().wlan, "link"], Duration::from_secs(5))
        .await
        .is_ok_and(|o| o.lines().any(|l| l.starts_with("Connected to")))
}

/// The supplicant asked to go (it deauthenticates as it does) and waited for, and the radio checked: until then wlan0
/// may still be on the other network, and its rules must stay. Never `ip link set down`: on this board that powers
/// the WCN chip off, and the firmware asserts at the next scan (CP2 assert, sc2355_scan failed -5) until a reboot.
/// `Ok` = off the network.
pub(crate) async fn leave(net: &Net) -> Result<()> {
    if let Some(c) = connect(net).await {
        let _ = c.request("TERMINATE", Duration::from_secs(3)).await;
        drop(c);
        let mut gone = false;
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !running(net).await {
                gone = true;
                break;
            }
        }
        if !gone {
            if let Some(pid) =
                fsx::read_line(&pid_path(net)).filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            {
                let _ = Cmd::new("kill").args(["-9", &pid]).timeout(Duration::from_secs(5)).output().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            if running(net).await {
                return Err(Error::failed("wpa_supplicant would not go"));
            }
        }
    }
    let _ = std::fs::remove_file(conf_path(net));
    let _ = std::fs::remove_file(pid_path(net));
    if !associated(net).await {
        return Ok(());
    }
    let _ = tools::iw(&["dev", &net.cfg().wlan, "disconnect"], Duration::from_secs(5)).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    if associated(net).await { Err(Error::failed("the radio is still associated")) } else { Ok(()) }
}

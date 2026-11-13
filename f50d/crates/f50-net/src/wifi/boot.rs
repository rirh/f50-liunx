//! The wireless configuration at boot, before the Wi-Fi comes up.
//!
//! Made once (`openwrt-wifi-config`): from `/etc/f50/hotspot.conf` (imported from Android by the installer) or with
//! a random passphrase; afterwards the Wi-Fi is managed with UCI and LuCI. 5 GHz is never a DFS channel (the driver
//! has no radar detection, FINDINGS 42): a DFS channel in hotspot.conf, or one the country does not allow, becomes the
//! country's default, and so does "auto"; the radio's channel list (used when LuCI sets "auto") is the country's
//! channels without DFS. At every boot after, a DFS channel set in LuCI or by hand is replaced the same way.

use std::time::Duration;

use f50_core::{Result, fsx};
use f50_platform::uci::{Change, Section};

use super::{Wireless, channels, delete_if, set, wpa2};
use crate::{Net, client, tools};

/// See [`Net::boot_wifi_config`].
pub(crate) async fn config(net: &Net) -> Result<bool> {
    let mut changed = wpa2::migrate(net).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "WPA2-only migration failed");
        false
    });
    changed |= wpa2::uapsd_off(net).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "U-APSD off failed");
        false
    });
    let marker = net.legacy("wifi-configured");
    changed |= if marker.exists() { dfs_fix(net).await? } else { first(net, &marker).await? };
    changed |= client::keep_radio_for_client(net).await?;
    Ok(changed)
}

/// A DFS channel set since becomes the country's default, with a line that says so.
async fn dfs_fix(net: &Net) -> Result<bool> {
    let w = Wireless::load(net).await?;
    let Some(radio) = w.radios().next() else { return Ok(false) };
    let Some(ch) = radio.get("channel").and_then(|c| c.parse::<u32>().ok()) else { return Ok(false) };
    if radio.get("band") != Some("5g") || !channels::ap5_dfs(ch) {
        return Ok(false);
    }
    let cc = super::country_of(net, radio);
    if let Some(note) = channels::ap5_resolve(&cc, Some(ch)).note {
        tools::logger("f50-wifi", &note).await;
    }
    let Some(d) = channels::ap5_default(&cc) else { return Ok(false) };
    let mut out = vec![set(radio, "channel", d.to_string())];
    if let Some(ht) = channels::htmode_for(radio.get("htmode"), &cc, d, None) {
        out.push(set(radio, "htmode", ht));
    }
    out.push(set(radio, "channels", channels::list_text(channels::ap5_channels(&cc))));
    out.push(set(radio, "acs_exclude_dfs", "1"));
    net.platform().uci().apply("wireless", &out).await.map(|()| true)
}

/// `KEY=value` from hotspot.conf, read literally (never sourced: the passphrase may hold anything).
fn conf_value<'a>(conf: &'a str, key: &str) -> Option<&'a str> {
    conf.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).filter(|v| !v.is_empty())
}

/// 12 characters from `A-HJ-NP-Za-km-z2-9` (nothing that reads like another), from the kernel's random source.
fn random_psk() -> Result<String> {
    use std::io::Read;
    const SET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| f50_core::Error::io("/dev/urandom", &e))?;
    let mut out = String::new();
    let mut b = [0u8; 64];
    while out.len() < 12 {
        f.read_exact(&mut b).map_err(|e| f50_core::Error::io("/dev/urandom", &e))?;
        // rejection sampling: no bias towards the first characters
        let limit = 256 - 256 % SET.len();
        out.extend(b.iter().filter(|x| usize::from(**x) < limit).map(|x| char::from(SET[usize::from(*x) % SET.len()])));
    }
    out.truncate(12);
    Ok(out)
}

/// The first configuration's settings: band, channel, width and channel list.
pub(crate) fn first_radio(
    band: Option<&str>,
    channel: Option<&str>,
    cc: &str,
) -> (String, String, String, Option<String>) {
    let two = || ("2g".to_owned(), "6".to_owned(), "HT20".to_owned(), None);
    if band == Some("2.4") {
        let ch = match channel {
            Some(c) if c != "auto" => c.to_owned(),
            _ => "6".into(),
        };
        return ("2g".into(), ch, "HT20".into(), None);
    }
    let asked = channel.and_then(|c| c.parse::<u32>().ok());
    let r = channels::ap5_resolve(cc, asked);
    if let Some(n) = &r.note {
        tracing::info!("{n}");
    }
    match r.channel {
        Some(ch) => (
            "5g".into(),
            ch.to_string(),
            format!("VHT{}", channels::ap5_width(cc, ch)),
            Some(channels::list_text(channels::ap5_channels(cc))),
        ),
        None => {
            tracing::info!("no 5 GHz channel without DFS in country {cc}: the hotspot uses 2.4 GHz");
            two()
        }
    }
}

async fn first(net: &Net, marker: &std::path::Path) -> Result<bool> {
    // the radio first: wifi-scripts generate the default (open, "OpenWrt") network for it
    for _ in 0..30 {
        if net.path("/sys/class/ieee80211/phy0").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let _ = f50_core::exec::Cmd::new("wifi").arg("config").timeout(Duration::from_secs(30)).output().await;
    let conf = fsx::read_opt(&net.legacy("hotspot.conf")).ok().flatten().unwrap_or_default();
    let cc = conf_value(&conf, "COUNTRY").filter(|c| crate::rules::country_ok(c)).unwrap_or(&net.cfg().default_country);
    let ssid = match conf_value(&conf, "SSID").filter(|s| crate::rules::ssid_ok(s)) {
        Some(s) => s.to_owned(),
        None => {
            let mac =
                fsx::read_line(&net.path(format!("/sys/class/net/{}/address", net.cfg().wlan))).unwrap_or_default();
            let hex: String = mac.chars().filter(char::is_ascii_hexdigit).collect();
            format!("F50-{}", hex.get(hex.len().saturating_sub(4)..).unwrap_or("").to_ascii_uppercase())
        }
    };
    let psk = match conf_value(&conf, "PSK").filter(|p| crate::rules::psk_ok(p)) {
        Some(p) => p.to_owned(),
        None => random_psk()?,
    };
    let (band, channel, htmode, list) = first_radio(conf_value(&conf, "BAND"), conf_value(&conf, "CHANNEL"), cc);

    let w = Wireless::load(net).await?;
    let mut out = Vec::new();
    let radio: Section = match w.radios().next() {
        Some(r) => r.clone(),
        None => {
            let r = Section {
                name: "radio0".into(),
                stype: "wifi-device".into(),
                anonymous: false,
                options: Default::default(),
            };
            out.push(Change::Section { section: r.name.clone(), stype: r.stype.clone() });
            out.extend([set(&r, "type", "mac80211"), set(&r, "phy", "phy0")]);
            r
        }
    };
    out.extend([
        set(&radio, "band", band.clone()),
        set(&radio, "channel", channel.clone()),
        set(&radio, "htmode", htmode),
    ]);
    match list {
        Some(l) => out.extend([set(&radio, "channels", l), set(&radio, "acs_exclude_dfs", "1")]),
        None => {
            delete_if(&radio, "channels", &mut out);
            delete_if(&radio, "acs_exclude_dfs", &mut out);
        }
    }
    out.push(set(&radio, "country", cc));
    delete_if(&radio, "disabled", &mut out);
    let default = format!("default_{}", radio.name);
    if w.section(&default).is_some() {
        out.push(Change::Delete { section: default, option: None });
    }
    let ap = Section { name: "ap0".into(), stype: "wifi-iface".into(), anonymous: false, options: Default::default() };
    out.push(Change::Section { section: ap.name.clone(), stype: ap.stype.clone() });
    for (k, v) in [
        ("device", radio.name.as_str()),
        ("network", net.cfg().lan_interface.as_str()),
        ("mode", "ap"),
        ("uapsd", "0"),
        ("ifname", net.cfg().wlan.as_str()),
        ("ssid", ssid.as_str()),
        ("encryption", "psk2"),
        ("key", psk.as_str()),
    ] {
        out.push(set(&ap, k, v));
    }
    net.platform().uci().apply("wireless", &out).await?;
    if let Some(d) = marker.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    fsx::write_atomic(marker, b"", 0o644)?;
    tools::logger("f50-wifi", &format!("wireless configured: {band} channel {channel}")).await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_settings() {
        assert_eq!(first_radio(Some("2.4"), Some("auto"), "TR"), ("2g".into(), "6".into(), "HT20".into(), None));
        assert_eq!(first_radio(Some("2.4"), Some("11"), "TR").1, "11");
        let (b, c, h, l) = first_radio(None, Some("60"), "CN");
        assert_eq!((b.as_str(), c.as_str(), h.as_str()), ("5g", "36", "VHT80"));
        assert_eq!(l.as_deref(), Some("36 40 44 48 149 153 157 161 165"));
        assert_eq!(first_radio(Some("5"), Some("165"), "CN").2, "VHT20");
        assert_eq!(first_radio(Some("5"), None, "00").0, "2g");
    }

    #[test]
    fn hotspot_conf() {
        let c = "SSID=Home\nPSK=a=b=c\nBAND=\n";
        assert_eq!(conf_value(c, "SSID"), Some("Home"));
        assert_eq!(conf_value(c, "PSK"), Some("a=b=c"));
        assert_eq!(conf_value(c, "BAND"), None);
        assert_eq!(conf_value(c, "SSIDX"), None);
    }
}

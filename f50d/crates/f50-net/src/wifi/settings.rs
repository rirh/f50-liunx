//! `f50.wifi set`: the hotspot's name, password, band, channel, width, hidden flag and country, written to UCI as one
//! batch; the Wi-Fi reloads two seconds later (the reply leaves first: the caller may be on this very hotspot).
//!
//! The rules are `f50-api wifi.set`'s and `f50-wifi-band`'s: a 2.4 GHz channel is 1-13, a 5 GHz one must be the
//! country's without DFS (FINDINGS 42); a new band starts on its automatic channel (2.4: 6; 5: the country's default)
//! and a channel asked for with it comes after; WPA3/SAE is never switched on, and an access point still set to it
//! becomes WPA2-PSK (CCMP) with its password or the one given (FINDINGS 80).

use std::time::Duration;

use serde_json::{Value, json};

use f50_core::rpc::Params;
use f50_core::{Error, Result};
use f50_platform::uci::{Change, Section};

use super::{Wireless, asks_sae, band_of, channels, country_of, delete_if, offload_options, set};
use crate::{Net, client, rules};

/// The request, checked against the rules that depend on more than one field.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub band: Option<String>,
    /// `None`: unchanged; `Some(None)`: auto.
    pub channel: Option<Option<u32>>,
    pub width: Option<Option<u32>>,
    pub hidden: Option<bool>,
    pub country: Option<String>,
}

fn parse_auto(field: &'static str, v: &str) -> Result<Option<u32>> {
    if v == "auto" { Ok(None) } else { v.parse().map(Some).map_err(|_| Error::bad_value(field)) }
}

/// The request's fields; at least one, each as `f50-api` took it.
pub(crate) fn request(p: &Params, band_now: &str, country_now: &str) -> Result<Request> {
    let mut r = Request::default();
    if let Some(s) = p.str("ssid") {
        if !rules::ssid_ok(s) {
            return Err(Error::bad_value("ssid"));
        }
        r.ssid = Some(s.to_owned());
    }
    if let Some(pw) = p.str("password") {
        if !rules::psk_ok(pw) {
            return Err(Error::bad_value("password"));
        }
        r.password = Some(pw.to_owned());
    }
    r.band = p.str("band").map(str::to_owned);
    r.channel = p.str("channel").map(|c| parse_auto("channel", c)).transpose()?;
    r.width = p.str("width").map(|c| parse_auto("width", c)).transpose()?;
    r.hidden = p.bool("hidden");
    r.country = p.str("country").map(str::to_owned);
    let band = r.band.as_deref().unwrap_or(band_now);
    let cc = r.country.as_deref().unwrap_or(country_now);
    if let Some(Some(ch)) = r.channel {
        let ok = if band == "2.4" { (1..=13).contains(&ch) } else { channels::ap5_allowed(cc, ch) };
        if !ok {
            return Err(Error::bad_value("channel"));
        }
    }
    if let Some(Some(w)) = r.width
        && band == "2.4"
        && w > 40
    {
        return Err(Error::bad_value("width"));
    }
    if r.country.is_some() && band == "5" && channels::ap5_channels(cc).is_empty() {
        return Err(Error::field(
            f50_core::Code::BadValue,
            "country",
            format!("country {cc} has no 5 GHz channel without DFS: switch the hotspot to 2.4 GHz first"),
        ));
    }
    if r == Request::default() {
        return Err(Error::missing("ssid"));
    }
    Ok(r)
}

/// The radio's changes for `band` (`f50-wifi-band`): its automatic channel, its width, its channel list.
pub(crate) fn band_changes(radio: &Section, band: &str, cc: &str, out: &mut Vec<Change>) -> Result<()> {
    if band == "2.4" {
        out.extend([set(radio, "band", "2g"), set(radio, "channel", "6"), set(radio, "htmode", "HT20")]);
        delete_if(radio, "channels", out);
        delete_if(radio, "acs_exclude_dfs", out);
        return Ok(());
    }
    let ch = channels::ap5_default(cc).ok_or_else(|| {
        Error::failed(format!("no 5 GHz channel without DFS in country {cc}: the hotspot stays on 2.4 GHz"))
    })?;
    out.extend([
        set(radio, "band", "5g"),
        set(radio, "channel", ch.to_string()),
        set(radio, "htmode", format!("VHT{}", channels::ap5_width(cc, ch))),
        set(radio, "channels", channels::list_text(channels::ap5_channels(cc))),
        set(radio, "acs_exclude_dfs", "1"),
    ]);
    Ok(())
}

/// Every change the request makes, in order (the band first, then the channel and width on it).
pub(crate) fn changes(
    radio: &Section,
    iface: &Section,
    r: &Request,
    band_now: &str,
    cc_now: &str,
) -> Result<Vec<Change>> {
    let mut out = Vec::new();
    let enc = iface.get("encryption");
    let sae = asks_sae(enc);
    if sae && r.password.is_none() && !iface.get("key").is_some_and(rules::psk_ok) {
        return Err(Error::missing("password"));
    }
    if let Some(s) = &r.ssid {
        out.push(set(iface, "ssid", s.clone()));
    }
    if let Some(pw) = &r.password {
        out.push(set(iface, "key", pw.clone()));
        // an open hotspot gets WPA2 with its first password; WPA3 is never switched on here
        if matches!(enc, None | Some("none" | "")) {
            out.push(set(iface, "encryption", "psk2"));
        }
    }
    if sae {
        out.push(set(iface, "encryption", "psk2+ccmp"));
        delete_if(iface, "ieee80211w", &mut out);
        delete_if(iface, "sae_pwe", &mut out);
        for o in offload_options(iface) {
            out.push(Change::DelList { section: iface.name.clone(), option: "hostapd_bss_options".into(), value: o });
        }
    }
    if let Some(h) = r.hidden {
        out.push(set(iface, "hidden", if h { "1" } else { "0" }));
    }
    let cc = r.country.as_deref().unwrap_or(cc_now);
    if let Some(c) = &r.country {
        out.push(set(radio, "country", c.clone()));
    }
    let band = r.band.as_deref().unwrap_or(band_now);
    // the htmode the radio will have once the band is set (the family carries over)
    let mut htmode = radio.get("htmode").map(str::to_owned);
    let mut channel = radio.get("channel").and_then(|c| c.parse::<u32>().ok());
    if r.band.is_some() {
        band_changes(radio, band, cc, &mut out)?;
        if band == "2.4" {
            htmode = Some("HT20".into());
            channel = Some(6);
        } else {
            channel = channels::ap5_default(cc);
            htmode = channel.map(|ch| format!("VHT{}", channels::ap5_width(cc, ch)));
        }
    } else if r.country.is_some() && band == "5" {
        // the new country's list for "auto", and its default for a channel it does not allow
        out.push(set(radio, "channels", channels::list_text(channels::ap5_channels(cc))));
        out.push(set(radio, "acs_exclude_dfs", "1"));
        if r.channel.is_none()
            && let Some(ch) = channel
            && !channels::ap5_allowed(cc, ch)
        {
            let res = channels::ap5_resolve(cc, Some(ch));
            if let Some(n) = res.note {
                tracing::info!("{n}");
            }
            channel = res.channel;
            if let Some(d) = channel {
                out.push(set(radio, "channel", d.to_string()));
            }
        }
    }
    if let Some(ch) = r.channel {
        out.push(set(radio, "channel", ch.map_or_else(|| "auto".to_owned(), |c| c.to_string())));
        channel = ch;
    }
    if band == "5" {
        let touched = r.channel.is_some() || r.width.is_some() || r.country.is_some();
        if touched && let Some(ch) = channel {
            let w = r.width.flatten();
            if let Some(ht) = channels::htmode_for(htmode.as_deref(), cc, ch, w) {
                out.push(set(radio, "htmode", ht));
            }
        }
    } else if let Some(w) = r.width {
        out.push(set(radio, "htmode", if w == Some(40) { "HT40" } else { "HT20" }));
    }
    Ok(out)
}

/// `f50.wifi set`.
pub(crate) async fn apply(net: &Net, p: &Params) -> Result<Value> {
    let w = Wireless::load(net).await?;
    let (radio, iface) =
        w.hotspot(&net.cfg().wlan).ok_or_else(|| Error::not_found("access point in /etc/config/wireless"))?;
    let (band_now, cc_now) = (band_of(radio), country_of(net, radio));
    let r = request(p, band_now, &cc_now)?;
    let restart = asks_sae(iface.get("encryption")) || r.password.is_some() || r.ssid.is_some();
    let changes = changes(radio, iface, &r, band_now, &cc_now)?;
    net.platform().uci().apply("wireless", &changes).await?;
    // the radio is a client of another network: the hotspot takes the settings when it comes back
    if !client::is_active(net).await {
        net.later("wifi-reload", Duration::from_secs(2), move |net| async move {
            // Recreate the AP for credential changes instead of retaining its previous authentication state.
            if restart { super::restart(&net).await } else { super::reload(&net).await }
        });
    }
    Ok(json!({ "restart": true }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::sec;
    use super::*;

    fn radio() -> Section {
        sec("radio0", "wifi-device", &[("band", "5g"), ("channel", "36"), ("htmode", "VHT80"), ("country", "CN")])
    }

    fn params() -> Params {
        Params::default()
    }

    fn texts(c: &[Change]) -> Vec<String> {
        c.iter()
            .map(|c| match c {
                Change::Set { section, option, value } => format!("set {section}.{option}={value}"),
                Change::Delete { section, option } => {
                    format!("delete {section}.{}", option.clone().unwrap_or_default())
                }
                Change::DelList { section, option, value } => format!("del_list {section}.{option}={value}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn request_rules() {
        assert_eq!(request(&params(), "5", "CN").map_err(|e| e.code()), Err(f50_core::Code::Missing));
        let bad = |p: Params| request(&p, "5", "CN").err().and_then(|e| e.field_name());
        assert_eq!(bad(params().with_str("ssid", " x")), Some("ssid"));
        assert_eq!(bad(params().with_str("password", "short")), Some("password"));
        assert_eq!(bad(params().with_str("channel", "60")), Some("channel"));
        assert_eq!(bad(params().with_str("channel", "14").with_str("band", "2.4")), Some("channel"));
        assert_eq!(bad(params().with_str("channel", "149").with_str("country", "TR")), Some("channel"));
        assert_eq!(bad(params().with_str("country", "AM")), Some("country"));
        assert_eq!(bad(params().with_str("width", "80").with_str("band", "2.4")), Some("width"));
        let r = request(&params().with_str("channel", "auto"), "5", "CN").expect("auto");
        assert_eq!(r.channel, Some(None));
    }

    #[test]
    fn sae_becomes_wpa2() {
        let ap = sec("ap0", "wifi-iface", &[("encryption", "sae"), ("key", "secret123"), ("ieee80211w", "2")]);
        let r = Request { hidden: Some(true), ..Request::default() };
        let c = changes(&radio(), &ap, &r, "5", "CN").expect("changes");
        assert_eq!(
            texts(&c),
            ["set ap0.encryption=psk2+ccmp", "delete ap0.ieee80211w", "set ap0.hidden=1"].map(String::from)
        );
        let nokey = sec("ap0", "wifi-iface", &[("encryption", "sae")]);
        assert!(changes(&radio(), &nokey, &r, "5", "CN").is_err());
    }

    #[test]
    fn band_then_channel() {
        let ap = sec("ap0", "wifi-iface", &[("encryption", "psk2")]);
        let r = Request { band: Some("2.4".into()), channel: Some(Some(11)), ..Request::default() };
        let c = texts(&changes(&radio(), &ap, &r, "5", "CN").expect("changes"));
        assert_eq!(c[..3], ["set radio0.band=2g", "set radio0.channel=6", "set radio0.htmode=HT20"].map(String::from));
        assert_eq!(c.last().map(String::as_str), Some("set radio0.channel=11"));
        let r = Request { channel: Some(Some(165)), ..Request::default() };
        let c = texts(&changes(&radio(), &ap, &r, "5", "CN").expect("changes"));
        assert_eq!(c, ["set radio0.channel=165", "set radio0.htmode=VHT20"].map(String::from));
    }

    #[test]
    fn open_gets_wpa2() {
        let ap = sec("ap0", "wifi-iface", &[("encryption", "none")]);
        let r = Request { password: Some("12345678".into()), ..Request::default() };
        let c = texts(&changes(&radio(), &ap, &r, "5", "CN").expect("changes"));
        assert_eq!(c, ["set ap0.key=12345678", "set ap0.encryption=psk2"].map(String::from));
    }
}

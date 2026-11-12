//! wifi.* (the hotspot: hotspot.conf on Ubuntu, UCI on OpenWrt) and wificlient.* (the uplink: wifi-client).

use std::collections::HashMap;
use std::fs;

use serde_json::{Value, json};

use super::wifi_channels::{ap5_allowed, ap5_channels, ap5_width};
use super::{argv, ok, start_job, text_of};
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule, is_ascii_line, matches};
use crate::exec::{executable, later};
use crate::jobs::JobSpec;
use crate::m;
use crate::sys::{Ctx, conf_get, conf_put, uq};

const SSID: Rule = Rule::Text { max_bytes: 32, max_chars: 32 };
const SET: &[Field] = &[
    Field::opt("ssid", SSID),
    Field::secret("password", Rule::Text { max_bytes: 64, max_chars: 64 }, false),
    Field::opt("band", Rule::Enum(&["2.4", "5"])),
    Field::opt("channel", Rule::WordNum { re: "auto|[0-9]{1,3}", max: 4 }),
    Field::opt("hidden", Rule::Bool),
];
const POWER: &[Field] = &[Field::req("state", Rule::Enum(&["on", "off"]))];
const JOIN: &[Field] = &[
    Field::req("ssid", SSID),
    Field::secret("password", Rule::Text { max_bytes: 64, max_chars: 64 }, false),
    Field::opt("open", Rule::Bool),
];
const LEAVE: &[Field] = &[Field::opt("forget", Rule::Bool)];

pub static METHODS: &[Method] = &[
    m!("wifi.get", Read, &[], get),
    m!("wifi.set", Write, SET, set),
    m!("wifi.power", Write, POWER, power),
    m!("wificlient.status", Read, &[], wc_status),
    m!("wificlient.scan", Write, &[], wc_scan),
    m!("wificlient.join", Write, JOIN, wc_join),
    m!("wificlient.leave", Write, LEAVE, wc_leave),
];

/// `uci show wireless` as key -> value (quotes off).
async fn uci_wireless(ctx: &Ctx) -> Vec<(String, String)> {
    let Ok(o) = ctx.uci().args(["show", "wireless"]).timeout_secs(5).run().await else { return vec![] };
    o.stdout
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.trim_matches('\'').to_owned()))
        .collect()
}

/// OpenWrt: the first radio and the access point on it ("wireless.radio0", "wireless.default_radio0").
fn radio_iface(w: &[(String, String)]) -> (String, String) {
    let m: HashMap<&str, &str> = w.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let radio = w.iter().find(|(_, v)| v == "wifi-device").map(|(k, _)| k.clone()).unwrap_or("wireless.radio0".into());
    let rname = radio.trim_start_matches("wireless.");
    let iface = w
        .iter()
        .filter(|(_, v)| v == "wifi-iface")
        .map(|(k, _)| k.as_str())
        .find(|s| {
            m.get(format!("{s}.device").as_str()) == Some(&rname) && m.get(format!("{s}.mode").as_str()) == Some(&"ap")
        })
        .unwrap_or("wireless.default_radio0")
        .to_owned();
    (radio, iface)
}

fn get_of<'a>(w: &'a [(String, String)], key: &str) -> &'a str {
    w.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap_or("")
}

/// The access point's interface (Ubuntu: wlan0; OpenWrt: whatever iw calls it).
async fn ap_ifname(ctx: &Ctx) -> String {
    let mut name = None;
    if let Ok(o) = ctx.cmd("iw").arg("dev").timeout_secs(5).run().await {
        let mut cur = "";
        for l in o.stdout.lines().map(str::trim) {
            if let Some(i) = l.strip_prefix("Interface ") {
                cur = i;
            } else if l == "type AP" && !cur.is_empty() {
                name = Some(cur.to_owned());
                break;
            }
        }
    }
    name.unwrap_or_else(|| "wlan0".into())
}

/// The stations of the access point, with the DHCP lease's address and name where there is one.
async fn clients(ctx: &Ctx, ifname: &str) -> Value {
    let leases_path = if ctx.sys.is_openwrt() { "/tmp/dhcp.leases" } else { "/run/mu300-lan.leases" };
    let leases = fs::read_to_string(ctx.cfg.sys(leases_path)).unwrap_or_default();
    let Ok(o) = ctx.cmd("iw").args(["dev", ifname, "station", "dump"]).timeout_secs(5).run().await else {
        return json!([]);
    };
    let v: Vec<Value> = o
        .stdout
        .lines()
        .filter_map(|l| l.strip_prefix("Station ").and_then(|r| r.split_whitespace().next()))
        .map(str::to_ascii_lowercase)
        .filter(|m| matches("[0-9a-f]{2}(:[0-9a-f]{2}){5}", m))
        .take(64)
        .map(|mac| {
            let lease = leases
                .lines()
                .map(|l| l.split_whitespace().collect::<Vec<_>>())
                .find(|f| f.get(1).is_some_and(|m| m.eq_ignore_ascii_case(&mac)));
            let ip = lease.as_ref().and_then(|f| f.get(2)).copied().unwrap_or("");
            let name = lease.as_ref().and_then(|f| f.get(3)).copied().filter(|n| *n != "*").unwrap_or("");
            json!({"mac": mac, "ip": ip, "name": name})
        })
        .collect();
    json!(v)
}

fn hotspot_conf(ctx: &Ctx) -> std::path::PathBuf {
    ctx.cfg.sys("/etc/mu300/hotspot.conf")
}

/// The Ubuntu hotspot is on, or on its way (systemd's activating: its checks take about 20 s after a restart).
async fn hotspot_on(ctx: &Ctx) -> bool {
    matches!(ctx.systemctl_state("mu300-hotspot").await.as_str(), "active" | "activating" | "reloading")
}

/// The hotspot's SSID and whether it is on (status.get).
pub(crate) async fn hotspot_brief(ctx: &Ctx) -> (String, bool) {
    if ctx.sys.is_openwrt() {
        let w = uci_wireless(ctx).await;
        let (r, i) = radio_iface(&w);
        (get_of(&w, &format!("{i}.ssid")).to_owned(), get_of(&w, &format!("{r}.disabled")) != "1")
    } else {
        (conf_get(&hotspot_conf(ctx), "SSID").unwrap_or_default(), hotspot_on(ctx).await)
    }
}

fn country_ok(cc: String) -> String {
    if matches("[A-Z0-9]{2}", &cc) { cc } else { "TR".into() }
}

async fn get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let ifname = ap_ifname(&ctx).await;
    let live = ctx.cmd("iw").args(["dev", &ifname, "info"]).timeout_secs(5).run().await.ok().and_then(|o| {
        o.stdout.lines().find_map(|l| l.trim().strip_prefix("channel ")?.split_whitespace().next()?.parse::<u32>().ok())
    });
    let (on, client, ssid, band, ch, cc, sec, hidden, pw);
    if ctx.sys.is_openwrt() {
        let w = uci_wireless(&ctx).await;
        let (r, i) = radio_iface(&w);
        let g = |k: String| get_of(&w, &k).to_owned();
        ssid = g(format!("{i}.ssid"));
        band = if g(format!("{r}.band")) == "2g" { "2.4" } else { "5" };
        ch = Some(g(format!("{r}.channel"))).filter(|c| !c.is_empty()).unwrap_or("auto".into());
        let enc = g(format!("{i}.encryption"));
        sec = if enc.starts_with("psk2") || enc.starts_with("psk-mixed") {
            "wpa2"
        } else if ["sae-mixed", "sae-compat", "psk3-mixed"].iter().any(|p| enc.starts_with(p)) {
            "wpa2-wpa3"
        } else if enc.starts_with("sae") || enc.starts_with("psk3") {
            "wpa3"
        } else if enc.is_empty() || enc == "none" {
            "open"
        } else {
            "other"
        };
        hidden = g(format!("{i}.hidden")) == "1";
        pw = !g(format!("{i}.key")).is_empty();
        on = g(format!("{r}.disabled")) != "1" && g(format!("{i}.disabled")) != "1";
        client = false;
        cc = country_ok(g(format!("{r}.country")));
    } else {
        let f = hotspot_conf(&ctx);
        let g = |k: &str| conf_get(&f, k).unwrap_or_default();
        ssid = g("SSID");
        band = if g("BAND") == "2.4" { "2.4" } else { "5" };
        ch = Some(g("CHANNEL")).filter(|c| !c.is_empty()).unwrap_or("auto".into());
        sec = "wpa2";
        hidden = matches!(g("HIDDEN").as_str(), "1" | "true" | "yes" | "on");
        pw = !g("PSK").is_empty();
        on = hotspot_on(&ctx).await;
        client = ctx.cfg.sys("/run/mu300-wifi-client.active").exists();
        cc = country_ok(g("COUNTRY"));
    }
    Ok(json!({
        "enabled": on, "client_mode": client, "ssid": ssid, "band": band, "channel": ch, "channel_live": live,
        "country": cc, "security": sec, "hidden": hidden, "password_set": pw,
        "channels": {"2.4": (1..=13).collect::<Vec<u32>>(), "5": ap5_channels(&cc)},
        "clients": clients(&ctx, &ifname).await,
    }))
}

/// 1-32 bytes, no control character, no space first or last (as hostapd and wifi-client take it).
fn ssid_ok(s: &str) -> bool {
    !s.is_empty() && !s.starts_with(' ') && !s.ends_with(' ')
}

/// WPA2's passphrase: 8-63 printable ASCII.
fn psk_ok(s: &str) -> bool {
    is_ascii_line(s) && (8..=63).contains(&s.len())
}

async fn set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let ssid = p.str("ssid");
    if ssid.is_some_and(|s| !ssid_ok(s)) {
        return Err(ApiError::bad_value("ssid"));
    }
    let psk = p.str("password");
    if psk.is_some_and(|s| !psk_ok(s)) {
        return Err(ApiError::bad_value("password"));
    }
    let band = p.str("band");
    let chan = p.str("channel");
    let hidden = p.bool("hidden");
    if ssid.is_none() && psk.is_none() && band.is_none() && chan.is_none() && hidden.is_none() {
        return Err(ApiError::missing("ssid"));
    }
    let w = if ctx.sys.is_openwrt() { uci_wireless(&ctx).await } else { vec![] };
    let (radio, iface) = radio_iface(&w);
    let conf = hotspot_conf(&ctx);
    let (band_now, cc) = if ctx.sys.is_openwrt() {
        (
            if get_of(&w, &format!("{radio}.band")) == "2g" { "2.4" } else { "5" },
            get_of(&w, &format!("{radio}.country")).to_owned(),
        )
    } else {
        (
            if conf_get(&conf, "BAND").as_deref() == Some("2.4") { "2.4" } else { "5" },
            conf_get(&conf, "COUNTRY").unwrap_or_default(),
        )
    };
    let cc = country_ok(cc);
    let eff_band = band.unwrap_or(band_now);
    if let Some(c) = chan.filter(|c| *c != "auto") {
        let n: u32 = c.parse().map_err(|_| ApiError::bad_value("channel"))?;
        let good = if eff_band == "2.4" { (1..=13).contains(&n) } else { ap5_allowed(&cc, n) };
        if !good {
            return Err(ApiError::bad_value("channel"));
        }
    }
    if ctx.sys.is_openwrt() {
        set_openwrt(&ctx, &w, &radio, &iface, &cc, ssid, psk, band, chan, hidden).await?;
    } else {
        if !conf.is_file() {
            return Err(ApiError::unsupported("hotspot.conf (the hotspot has not run yet)"));
        }
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        ssid.into_iter().for_each(|s| pairs.push(("SSID", s)));
        psk.into_iter().for_each(|s| pairs.push(("PSK", s)));
        // a new band starts on its automatic channel (mu300-wifi-band's rule)
        if let Some(b) = band {
            pairs.push(("BAND", b));
            pairs.push(("CHANNEL", chan.unwrap_or("auto")));
        } else if let Some(c) = chan {
            pairs.push(("CHANNEL", c));
        }
        let h = hidden.map(|h| if h { "1" } else { "0" });
        h.into_iter().for_each(|h| pairs.push(("HIDDEN", h)));
        conf_put(&conf, 0o600, &pairs).map_err(|_| ApiError::failed("could not write hotspot.conf"))?;
        // the radio is a client of another network: the hotspot takes the settings when it comes back
        if !ctx.cfg.sys("/run/mu300-wifi-client.active").exists() {
            later(&ctx.cfg, &["systemctl", "restart", "mu300-hotspot"])?;
        }
    }
    Ok(json!({ "restart": true }))
}

/// The uci lines for CHAN on RADIO, and on 5 GHz the width the country allows there.
fn uci_channel(w: &[(String, String)], radio: &str, cc: &str, chan: &str, band5: bool) -> String {
    let mut s = format!("set {radio}.channel={chan}\n");
    if let (true, Ok(n)) = (band5, chan.parse::<u32>()) {
        let ht = get_of(w, &format!("{radio}.htmode"));
        let mut width = ap5_width(cc, n);
        if ht.starts_with("HT") {
            if width == 80 {
                width = 40;
            }
            s.push_str(&format!("set {radio}.htmode=HT{width}\n"));
        } else if !ht.is_empty() && ht != "NOHT" {
            let prefix: String = ht.chars().take_while(|c| !c.is_ascii_digit()).collect();
            s.push_str(&format!("set {radio}.htmode={prefix}{width}\n"));
        }
    }
    s
}

#[allow(clippy::too_many_arguments)]
async fn set_openwrt(
    ctx: &Ctx,
    w: &[(String, String)],
    radio: &str,
    iface: &str,
    cc: &str,
    ssid: Option<&str>,
    psk: Option<&str>,
    band: Option<&str>,
    chan: Option<&str>,
    hidden: Option<bool>,
) -> ApiResult<()> {
    let enc = get_of(w, &format!("{iface}.encryption"));
    // WPA3/SAE is disabled on this Wi-Fi chip: a hotspot still set to it becomes WPA2-PSK (CCMP) here
    let sae = matches!(enc.split('+').next().unwrap_or(""), "sae" | "sae-mixed" | "sae-compat" | "psk3" | "psk3-mixed");
    if sae && psk.is_none() && !psk_ok(get_of(w, &format!("{iface}.key"))) {
        return Err(ApiError::missing("password"));
    }
    let mut s = String::new();
    if let Some(v) = ssid {
        s.push_str(&format!("set {iface}.ssid={}\n", uq(v)));
    }
    if let Some(v) = psk {
        s.push_str(&format!("set {iface}.key={}\n", uq(v)));
        if enc.is_empty() || enc == "none" {
            s.push_str(&format!("set {iface}.encryption=psk2\n"));
        }
    }
    if sae {
        s.push_str(&format!("set {iface}.encryption=psk2+ccmp\n"));
        for o in ["ieee80211w", "sae_pwe"] {
            if !get_of(w, &format!("{iface}.{o}")).is_empty() {
                s.push_str(&format!("delete {iface}.{o}\n"));
            }
        }
        if get_of(w, &format!("{iface}.hostapd_bss_options")).contains("u30_sae_offload=1") {
            s.push_str(&format!("del_list {iface}.hostapd_bss_options=u30_sae_offload=1\n"));
        }
    }
    if let Some(h) = hidden {
        s.push_str(&format!("set {iface}.hidden={}\n", u8::from(h)));
    }
    let band5_now = get_of(w, &format!("{radio}.band")) != "2g";
    if let (Some(c), None) = (chan, band) {
        s.push_str(&uci_channel(w, radio, cc, c, band5_now));
    }
    s.push_str("commit wireless\n");
    ctx.uci_batch(s).await?;
    if let Some(b) = band {
        ctx.need_tool("mu300-wifi-band")?.arg(b).timeout_secs(30).ok().await?;
        // mu300-wifi-band puts the new band on its automatic channel; a channel asked for with it comes after
        if let Some(c) = chan.filter(|c| *c != "auto") {
            let w2 = uci_wireless(ctx).await;
            ctx.uci_batch(uci_channel(&w2, radio, cc, c, b == "5") + "commit wireless\n").await?;
            later(&ctx.cfg, &["wifi", "reload"])?;
        }
    } else if sae {
        later(&ctx.cfg, &["sh", "-c", "wifi down; sleep 1; wifi up"])?;
    } else {
        later(&ctx.cfg, &["wifi", "reload"])?;
    }
    Ok(())
}

async fn power(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let on = p.str("state") == Some("on");
    if ctx.sys.is_openwrt() {
        let (radio, _) = radio_iface(&uci_wireless(&ctx).await);
        ctx.uci_batch(format!("set {radio}.disabled={}\ncommit wireless\n", u8::from(!on))).await?;
        later(&ctx.cfg, &["wifi", if on { "up" } else { "down" }])?;
    } else {
        let _ = ctx
            .cmd("systemctl")
            .args([if on { "enable" } else { "disable" }, "mu300-hotspot"])
            .timeout_secs(15)
            .run()
            .await;
        later(&ctx.cfg, &["systemctl", if on { "start" } else { "stop" }, "mu300-hotspot"])?;
    }
    ok()
}

fn need_wc(ctx: &Ctx) -> ApiResult<std::path::PathBuf> {
    let p = ctx.cfg.tool("wifi-client");
    if executable(&p) { Ok(p) } else { Err(ApiError::unsupported("wifi-client")) }
}

async fn wc_status(ctx: Ctx, _: Params) -> ApiResult<Value> {
    need_wc(&ctx)?;
    let o = ctx.tool("wifi-client").arg("status").timeout_secs(15).output().await?;
    Ok(json!({ "active": ctx.cfg.sys("/run/mu300-wifi-client.active").exists(), "text": text_of(&o) }))
}

/// A scan takes up to 40 s with the hotspot on: a job; its result text is the list.
async fn wc_scan(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let wc = need_wc(&ctx)?;
    start_job(&ctx, JobSpec::new("wificlient.scan", "wifi", 60, argv(&wc, &["scan"])))
}

async fn wc_join(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let wc = need_wc(&ctx)?;
    let ssid = p.str("ssid").unwrap_or_default();
    if !ssid_ok(ssid) {
        return Err(ApiError::bad_value("ssid"));
    }
    let open = p.flag("open");
    let pw = p.str("password").filter(|s| !s.is_empty());
    // a WPA2/WPA3 passphrase (8-63 printable ASCII) or a raw key (64 hex digits)
    if pw.is_some_and(|s| !psk_ok(s) && !matches("[0-9A-Fa-f]{64}", s)) {
        return Err(ApiError::bad_value("password"));
    }
    let spec = match (open, pw) {
        (true, _) => JobSpec::new("wificlient.join", "wifi", 180, argv(&wc, &["connect", ssid, "--open"])),
        (false, Some(pw)) => {
            JobSpec::new("wificlient.join", "wifi", 180, argv(&wc, &["connect", ssid, "-"])).stdin(format!("{pw}\n"))
        }
        (false, None) => return Err(ApiError::missing("password")),
    };
    start_job(&ctx, spec)
}

async fn wc_leave(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let wc = need_wc(&ctx)?;
    let verb = if p.flag("forget") { "forget" } else { "disconnect" };
    start_job(&ctx, JobSpec::new("wificlient.leave", "wifi", 120, argv(&wc, &[verb])))
}

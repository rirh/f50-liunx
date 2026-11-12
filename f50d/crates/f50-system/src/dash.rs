//! The home page's non-modem figures (dashboard-info): Wi-Fi clients, DHCP leases, tracked connections, the
//! USB-C link and the update notice. Every list is bounded; every external ask has a hard timeout.

use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_platform::Platform;
use serde::Serialize;

/// At most this many Wi-Fi clients and leases are listed.
pub(crate) const CLIENTS_MAX: usize = 64;
pub(crate) const LEASES_MAX: usize = 256;
/// SC2355's `station dump` lists only each MAC; `station get` has the metrics: asked for at most this many.
const STATION_GET_MAX: usize = 16;

/// One associated station.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub(crate) struct Station {
    pub mac: String,
    pub host: Option<String>,
    pub ip: Option<String>,
    /// dBm; `None` for the driver's "0" (unknown).
    pub signal: Option<f64>,
    pub tx: Option<String>,
    pub rx: Option<String>,
    /// Minutes connected.
    pub conn_min: Option<u64>,
}

fn is_mac(s: &str) -> bool {
    let p: Vec<&str> = s.split(':').collect();
    p.len() == 6 && p.iter().all(|x| x.len() == 2 && x.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `iw dev X station dump|get`, one entry per station (a station without metrics still counts).
pub(crate) fn parse_stations(text: &str) -> Vec<Station> {
    let mut out: Vec<Station> = Vec::new();
    let mut cur: Option<Station> = None;
    let push = |s: Option<Station>, out: &mut Vec<Station>| {
        if let Some(s) = s.filter(|s| is_mac(&s.mac) && !out.iter().any(|o| o.mac == s.mac)) {
            out.push(s);
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = line.strip_prefix("Station") {
            push(cur.take(), &mut out);
            let mac = rest.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            cur = Some(Station { mac, ..Station::default() });
            continue;
        }
        let Some(s) = cur.as_mut() else { continue };
        let words: Vec<&str> = t.split_whitespace().collect();
        if let Some(v) = t.strip_prefix("signal:") {
            s.signal = v.split_whitespace().next().and_then(|x| x.parse::<f64>().ok()).filter(|x| *x != 0.0);
        } else if t.starts_with("tx bitrate:") {
            s.tx = words.get(2).filter(|x| x.parse::<f64>().is_ok()).map(|x| format!("{x} Mbit/s"));
        } else if t.starts_with("rx bitrate:") {
            s.rx = words.get(2).filter(|x| x.parse::<f64>().is_ok()).map(|x| format!("{x} Mbit/s"));
        } else if t.starts_with("connected time:") {
            s.conn_min = words.get(2).and_then(|x| x.parse::<u64>().ok()).map(|x| x / 60);
        }
    }
    push(cur.take(), &mut out);
    out.truncate(CLIENTS_MAX);
    out
}

fn iface_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 15
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

async fn iw(args: &[&str]) -> Option<String> {
    Cmd::new("iw").args(args).timeout(Duration::from_secs(3)).max_output(64 * 1024).ok().await.ok()
}

/// The hotspot's stations; `fill` asks `station get` for the ones without metrics.
pub(crate) async fn stations(dev: &str, fill: bool) -> Vec<Station> {
    if !iface_ok(dev) {
        return Vec::new();
    }
    let mut list = parse_stations(&iw(&["dev", dev, "station", "dump"]).await.unwrap_or_default());
    if fill {
        for s in list
            .iter_mut()
            .filter(|s| s.signal.is_none() || s.tx.is_none() || s.conn_min.is_none())
            .take(STATION_GET_MAX)
        {
            let got = iw(&["dev", dev, "station", "get", &s.mac]).await.unwrap_or_default();
            if let Some(g) = parse_stations(&got).into_iter().find(|g| g.mac == s.mac) {
                s.signal = s.signal.or(g.signal);
                s.tx = s.tx.take().or(g.tx);
                s.rx = s.rx.take().or(g.rx);
                s.conn_min = s.conn_min.or(g.conn_min);
            }
        }
    }
    list
}

/// One DHCP lease.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Lease {
    pub host: Option<String>,
    pub ip: String,
    pub mac: String,
    /// Seconds left.
    pub left: u64,
}

/// dnsmasq's leases file: `expiry mac ip host clientid`, "*" for no name.
pub(crate) fn parse_leases(text: &str, now: u64) -> Vec<Lease> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let mac = f.get(1)?.to_ascii_lowercase();
            let exp: u64 = f.first().and_then(|e| e.parse().ok()).unwrap_or(now);
            let host = f.get(3).filter(|h| **h != "*").map(|h| f50_core::sanitize::sanitize_message(h));
            Some(Lease { host, ip: f.get(2)?.to_string(), mac, left: exp.saturating_sub(now) })
        })
        .take(LEASES_MAX)
        .collect()
}

pub(crate) fn leases(path: &Path) -> Vec<Lease> {
    parse_leases(&std::fs::read_to_string(path).unwrap_or_default(), f50_core::time::unix_now())
}

/// Names and addresses of the stations from the leases.
pub(crate) fn join(stations: &mut [Station], leases: &[Lease]) {
    for s in stations {
        if let Some(l) = leases.iter().find(|l| l.mac == s.mac) {
            s.host.clone_from(&l.host);
            s.ip = Some(l.ip.clone());
        }
    }
}

/// Tracked connections: the kernel's counter, else the table's lines; unknown (never a made-up 0) without either.
pub(crate) async fn conntrack(platform: &Platform) -> Option<u64> {
    if let Some(n) = platform.sysfs().read_num("/proc/sys/net/netfilter/nf_conntrack_count") {
        return Some(n);
    }
    let table = platform.sysfs().path("/proc/net/nf_conntrack");
    tokio::task::spawn_blocking(move || {
        use std::io::BufRead;
        let f = std::fs::File::open(table).ok()?;
        Some(std::io::BufReader::new(f).lines().map_while(Result::ok).count() as u64)
    })
    .await
    .ok()
    .flatten()
}

/// The USB-C port's negotiated link.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct UsbLink {
    pub role: &'static str,
    pub speed: Option<String>,
    pub mbps: Option<u64>,
    pub max: Option<String>,
    pub max_mbps: Option<u64>,
}

pub(crate) fn usb_mbps(s: &str) -> Option<u64> {
    match s {
        "low-speed" => Some(1),
        "full-speed" => Some(12),
        "high-speed" => Some(480),
        "super-speed" => Some(5000),
        "super-speed-plus" => Some(10_000),
        n => n.parse().ok(),
    }
}

/// Device mode: the gadget's UDC; host mode: the fastest attached device (a root hub, 1d6b, is the port itself).
pub(crate) fn usb_link(platform: &Platform) -> Option<UsbLink> {
    let sys = platform.sysfs();
    let dirs = |d: &str| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> =
            std::fs::read_dir(sys.path(d)).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        v
    };
    for u in dirs("/sys/class/udc") {
        let Some(s) = f50_core::fsx::read_line(&u.join("current_speed")) else { continue };
        let m = f50_core::fsx::read_line(&u.join("maximum_speed"));
        return Some(UsbLink {
            role: "device",
            mbps: usb_mbps(&s),
            speed: Some(s),
            max_mbps: m.as_deref().and_then(usb_mbps),
            max: m,
        });
    }
    dirs("/sys/bus/usb/devices")
        .into_iter()
        .filter(|d| f50_core::fsx::read_line(&d.join("idVendor")).as_deref() != Some("1d6b"))
        .filter_map(|d| {
            let s = f50_core::fsx::read_line(&d.join("speed"))?;
            Some((usb_mbps(&s)?, s))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(m, s)| UsbLink { role: "host", speed: Some(s), mbps: Some(m), max: None, max_mbps: None })
}

/// A release tag (docs/research/FINDINGS.md 125): `vYYYY.MM.DD`, or `vYYYY.MM.DD.N`.
pub(crate) fn is_release_tag(s: &str) -> bool {
    f50_core::rpc::matches(r"v[0-9]{4}\.(0[1-9]|1[0-2])\.(0[1-9]|[12][0-9]|3[01])(\.[1-9][0-9]?)?", s)
}

/// The tag of a newer release, from the update service's note (the first word that is a tag).
/// The newer release's tag from f50-update's notice (`{"tag":...,"installed":...,"at":...}`), or the first tag on a
/// plain text first line.
pub(crate) fn update_tag(path: &Path) -> Option<String> {
    let t = std::fs::read_to_string(path).ok()?;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
        return v.get("tag").and_then(|x| x.as_str()).filter(|w| is_release_tag(w)).map(str::to_owned);
    }
    t.lines().next()?.split_whitespace().find(|w| is_release_tag(w)).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_dump() {
        let t = "Station AA:bb:cc:dd:ee:01 (on wlan0)\n\tsignal:  \t-52 dBm\n\ttx bitrate:\t144.4 MBit/s\n\
                 \trx bitrate:\t(unknown)\n\tconnected time:\t125 seconds\n\
                 Station aa:bb:cc:dd:ee:02 (on wlan0)\n\tsignal: 0 dBm\nStation bogus (on wlan0)\n";
        let s = parse_stations(t);
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].mac.as_str(), s[0].signal, s[0].tx.as_deref(), s[0].rx.as_deref(), s[0].conn_min),
            ("aa:bb:cc:dd:ee:01", Some(-52.0), Some("144.4 Mbit/s"), None, Some(2))
        );
        assert_eq!(s[1].signal, None);
    }

    #[test]
    fn leases_and_tags() {
        let l = parse_leases(
            "1000 AA:bb:cc:dd:ee:01 192.168.77.10 phone 01:aa\nx aa:bb:cc:dd:ee:02 192.168.77.11 *\n",
            400,
        );
        assert_eq!(
            l[0],
            Lease {
                host: Some("phone".into()),
                ip: "192.168.77.10".into(),
                mac: "aa:bb:cc:dd:ee:01".into(),
                left: 600
            }
        );
        assert_eq!((l[1].host.clone(), l[1].left), (None, 0));
        assert!(is_release_tag("v2026.10.27.1") && is_release_tag("v2026.10.27"));
        assert!(!is_release_tag("v2026.13.01") && !is_release_tag("v2026.10.27.0"));
        assert_eq!(usb_mbps("high-speed"), Some(480));
    }
}

//! The networks in range: from the supplicant's scan when it holds the interface (`iw scan` can then block until it
//! gives the radio back), else from `iw dev wlan0 scan`. Both list each access point; the result is one entry per
//! name, strongest first, no hidden networks. Ported from `wifi-client scan`.

use std::time::Duration;

use serde::Serialize;

use f50_core::{Error, Result};

use super::names::{decode, shown};

/// One network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Network {
    /// The name as it may be shown (see `names`).
    pub ssid: String,
    /// dBm.
    pub signal: i32,
    /// `WPA2/3`, `WPA3`, `WPA2`, `WPA`, `OWE`, `EAP`, `WEP` or `open`.
    pub security: &'static str,
    #[serde(skip)]
    pub raw: Vec<u8>,
}

/// The label for a supplicant's flags field (`[WPA2-PSK-CCMP][ESS]`).
pub(crate) fn wpa_security(flags: &str) -> &'static str {
    let (sae, psk) = (flags.contains("SAE"), flags.contains("PSK"));
    if sae && psk {
        "WPA2/3"
    } else if sae {
        "WPA3"
    } else if psk {
        "WPA2"
    } else if flags.contains("OWE") {
        "OWE"
    } else if flags.contains("EAP") {
        "EAP"
    } else if flags.contains("WEP") {
        "WEP"
    } else {
        "open"
    }
}

/// One access point of the supplicant's `SCAN_RESULTS`: flags and the raw name.
pub(crate) struct WpaEntry {
    pub signal: i32,
    pub flags: String,
    pub raw: Vec<u8>,
}

/// `bssid / frequency / signal level / flags / ssid` lines after the header.
pub(crate) fn parse_wpa(results: &str) -> Vec<WpaEntry> {
    results
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(5, '\t').collect();
            (f.len() == 5).then(|| WpaEntry {
                signal: f[2].trim().parse().unwrap_or(-100),
                flags: f[3].to_owned(),
                raw: decode(f[4]),
            })
        })
        .collect()
}

/// `iw dev IF scan` output: only the element lines of the BSS itself, one tab in (further in, a reduced neighbour
/// report has "BSSID: 00:00:00:00:00:00", which an unanchored "SSID: " took for the name).
pub(crate) fn parse_iw(out: &str) -> Vec<(i32, &'static str, Vec<u8>)> {
    #[derive(Default)]
    struct B {
        seen: bool,
        sig: i32,
        ssid: Vec<u8>,
        rsn: bool,
        wpa: bool,
        privacy: bool,
        psk: bool,
        sae: bool,
        owe: bool,
        eap: bool,
    }
    fn flush(b: &B, out: &mut Vec<(i32, &'static str, Vec<u8>)>) {
        if !b.seen {
            return;
        }
        let enc = if b.rsn || b.wpa {
            if b.sae && b.psk {
                "WPA2/3"
            } else if b.sae {
                "WPA3"
            } else if b.psk {
                if b.rsn { "WPA2" } else { "WPA" }
            } else if b.owe {
                "OWE"
            } else if b.eap {
                "EAP"
            } else if b.rsn {
                "WPA2"
            } else {
                "WPA"
            }
        } else if b.privacy {
            "WEP"
        } else {
            "open"
        };
        out.push((b.sig, enc, b.ssid.clone()));
    }
    let mut res = Vec::new();
    let mut b = B::default();
    for line in out.lines() {
        if line.starts_with("BSS ") {
            flush(&b, &mut res);
            b = B { seen: true, ..B::default() };
        } else if let Some(v) = line.strip_prefix("\tsignal: ") {
            b.sig = v.split('.').next().and_then(|n| n.trim().parse().ok()).unwrap_or(-100);
        } else if let Some(v) = line.strip_prefix("\tSSID: ") {
            b.ssid = decode(v);
        } else if line.starts_with("\tcapability: ") && line.contains("Privacy") {
            b.privacy = true;
        } else if line.starts_with("\tRSN:") {
            b.rsn = true;
        } else if line.starts_with("\tWPA:") {
            b.wpa = true;
        } else if let Some((_, suites)) = line.split_once("* Authentication suites:") {
            for a in suites.split_whitespace() {
                if a.contains("SAE") {
                    b.sae = true;
                } else if a.contains("PSK") {
                    b.psk = true;
                } else if a.contains("OWE") {
                    b.owe = true;
                } else if a.contains("802.1X") || a.contains("EAP") || a.contains("SUITE-B") {
                    b.eap = true;
                }
            }
        }
    }
    flush(&b, &mut res);
    res
}

/// One entry per name, strongest first, no hidden networks (an empty name, all zero bytes, or the null BSSID).
pub(crate) fn list(mut entries: Vec<(i32, &'static str, Vec<u8>)>) -> Vec<Network> {
    entries.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|(_, _, raw)| !raw.is_empty() && !raw.iter().all(|b| *b == 0) && raw.as_slice() != b"00:00:00:00:00:00")
        .filter(|(_, _, raw)| seen.insert(raw.clone()))
        .map(|(signal, security, raw)| Network { ssid: shown(&raw), signal, security, raw })
        .collect()
}

/// Is every access point of `ssid` WPA3-only (SAE and no PSK)? Without SAE here such a network can never be joined,
/// and the supplicant would just keep trying until the timeout (FINDINGS 14b).
pub(crate) fn sae_only(entries: &[WpaEntry], ssid: &[u8]) -> bool {
    let mine: Vec<&WpaEntry> = entries.iter().filter(|e| e.raw == ssid).collect();
    !mine.is_empty() && mine.iter().all(|e| e.flags.contains("SAE") && !e.flags.contains("PSK"))
}

/// `iw dev IF scan`, up to three times (the first scan after the radio changes mode often comes back busy or empty).
pub(crate) async fn iw_scan(ifname: &str) -> Result<Vec<Network>> {
    let mut last = String::new();
    for n in 0..3 {
        if n > 0 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        match crate::tools::iw(&["dev", ifname, "scan"], Duration::from_secs(25)).await {
            Ok(out) if out.contains("BSS ") => return Ok(list(parse_iw(&out))),
            Ok(out) => last = out.lines().next().unwrap_or("").to_owned(),
            Err(e) => last = e.message().to_owned(),
        }
    }
    Err(Error::failed(format!("the scan found nothing ({})", shown(last.as_bytes()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IW: &str = "BSS 11:22:33:44:55:66(on wlan0)\n\tsignal: -61.00 dBm\n\tSSID: Home\n\tRSN:\t * Version: 1\n\
\t\t * Authentication suites: PSK SAE\n\tcapability: ESS Privacy (0x0011)\n\
BSS aa:bb:cc:dd:ee:ff(on wlan0)\n\tsignal: -40.00 dBm\n\tSSID: Home\n\tRSN:\t * Version: 1\n\
\t\t * Authentication suites: PSK\n\t\tReduced neighbor report:\n\t\t\tBSSID: 00:00:00:00:00:00\n\
BSS 01:02:03:04:05:06(on wlan0)\n\tsignal: -70.00 dBm\n\tSSID: \\xc3\\x87ay\n\tcapability: ESS (0x0001)\n\
BSS 01:02:03:04:05:07(on wlan0)\n\tsignal: -30.00 dBm\n\tSSID: \n\tcapability: ESS Privacy (0x0011)\n";

    #[test]
    fn iw_scan_list() {
        let l = list(parse_iw(IW));
        let got: Vec<(&str, i32, &str)> = l.iter().map(|n| (n.ssid.as_str(), n.signal, n.security)).collect();
        assert_eq!(got, [("Home", -40, "WPA2"), ("Çay", -70, "open")]);
        let all = parse_iw(IW);
        assert_eq!(all[0].1, "WPA2/3");
        assert_eq!(all[3].1, "WEP");
    }

    #[test]
    fn supplicant_results() {
        let r = "bssid / frequency / signal level / flags / ssid\n\
aa:bb:cc:dd:ee:01\t5180\t-50\t[RSN-SAE-CCMP][ESS]\tWiFi7\n\
aa:bb:cc:dd:ee:02\t2412\t-60\t[WPA2-PSK+SAE-CCMP][ESS]\tMixed\n\
aa:bb:cc:dd:ee:03\t2412\t-70\t[ESS]\tOpen\\x20\n";
        let e = parse_wpa(r);
        assert_eq!(e.len(), 3);
        assert_eq!(wpa_security(&e[0].flags), "WPA3");
        assert_eq!(wpa_security(&e[1].flags), "WPA2/3");
        assert_eq!(wpa_security(&e[2].flags), "open");
        assert_eq!(shown(&e[2].raw), "Open\\x20");
        assert!(sae_only(&e, b"WiFi7"));
        assert!(!sae_only(&e, b"Mixed"));
        assert!(!sae_only(&e, b"Missing"));
    }
}

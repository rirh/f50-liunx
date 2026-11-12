//! The saved network (`/etc/mu300/wifi-client.conf`, root's alone): `SSID=`, `PSK=`, `ENABLE=` (joined at boot) and
//! `METRIC=`, the format the shell layer wrote, so an updated device keeps it. Values are only ever matched, never
//! sourced; the file is replaced whole (no half-written PSK).

use f50_core::{Result, fsx};

use crate::Net;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Saved {
    pub ssid: String,
    pub psk: Option<String>,
    pub enable: bool,
    pub metric: u32,
}

/// The last `KEY=` line's value (the shell's `sed | tail -n1`).
fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().filter_map(|l| l.strip_prefix(key)?.strip_prefix('=')).next_back()
}

pub(crate) fn parse(text: &str) -> Option<Saved> {
    let ssid = value(text, "SSID").filter(|s| !s.is_empty())?.to_owned();
    Some(Saved {
        ssid,
        psk: value(text, "PSK").filter(|p| !p.is_empty()).map(str::to_owned),
        enable: value(text, "ENABLE") == Some("1"),
        metric: value(text, "METRIC").and_then(|m| m.parse().ok()).unwrap_or(50),
    })
}

pub(crate) fn text(s: &Saved) -> String {
    let mut t = format!("SSID={}\n", s.ssid);
    if let Some(p) = &s.psk {
        t.push_str(&format!("PSK={p}\n"));
    }
    t.push_str(&format!("ENABLE={}\nMETRIC={}\n", u8::from(s.enable), s.metric));
    t
}

pub(crate) fn load(net: &Net) -> Option<Saved> {
    fsx::read_opt(&net.legacy("wifi-client.conf")).ok().flatten().as_deref().and_then(parse)
}

/// Saved (and joined at boot), keeping the metric there was.
pub(crate) fn save(net: &Net, ssid: &str, psk: Option<&str>) -> Result<()> {
    let metric = load(net).map_or(net.cfg().client_metric, |s| s.metric);
    let s = Saved { ssid: ssid.into(), psk: psk.map(str::to_owned), enable: true, metric };
    write(net, &s)
}

fn write(net: &Net, s: &Saved) -> Result<()> {
    let p = net.legacy("wifi-client.conf");
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| f50_core::Error::io(d.display(), &e))?;
    }
    fsx::write_atomic(&p, text(s).as_bytes(), 0o600)
}

/// Whether the boot joins it.
pub(crate) fn set_enable(net: &Net, on: bool) -> Result<()> {
    match load(net) {
        Some(mut s) => {
            s.enable = on;
            write(net, &s)
        }
        None => Ok(()),
    }
}

pub(crate) fn forget(net: &Net) {
    let _ = std::fs::remove_file(net.legacy("wifi-client.conf"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = parse("SSID=Home=1\nPSK=pa ss\nENABLE=0\nENABLE=1\nMETRIC=60\n").expect("saved");
        assert_eq!(s, Saved { ssid: "Home=1".into(), psk: Some("pa ss".into()), enable: true, metric: 60 });
        assert_eq!(parse(&text(&s)), Some(s));
        assert_eq!(parse("PSK=x\n"), None);
        let open = parse("SSID=Cafe\n").expect("open");
        assert_eq!((open.psk, open.enable, open.metric), (None, false, 50));
    }
}

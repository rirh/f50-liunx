//! The address the other network gives by DHCP: busybox `udhcpc`, its lease handed to f50d through a small script
//! f50d writes, applied by f50d (address, default route with the uplink's metric), and dropped with everything it
//! set when the link goes.
//!
//! udhcpc stays after the first lease to renew it (it puts itself in the background with a session of its own). A
//! renewal that keeps the address changes nothing; one that changes it, or a lost lease, is seen by the client's
//! watcher, which closes the sharing, applies it, checks the new address and only then shares again. DNS is left to
//! dnsmasq as it is (OpenWrt's /etc/resolv.conf is netifd's; the shell layer did not change it either).

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result, fsx};

use crate::{Net, tools};

/// One lease as udhcpc reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lease {
    /// `bound`, `renew` or `deconfig`.
    pub event: String,
    pub ip: Option<Ipv4Addr>,
    pub prefix: u8,
    pub routers: Vec<Ipv4Addr>,
}

impl Lease {
    pub fn net(&self) -> Option<(Ipv4Addr, u8)> {
        self.ip.map(|a| (a, self.prefix))
    }
}

pub(crate) fn parse(text: &str) -> Lease {
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix('=')).unwrap_or("").trim();
    let prefix = match get("mask") {
        "" => 24,
        m if m.contains('.') => m.parse().ok().and_then(crate::lan::prefix_of).unwrap_or(24),
        m => m.parse().ok().filter(|p| *p <= 32).unwrap_or(24),
    };
    Lease {
        event: get("event").to_owned(),
        ip: get("ip").parse().ok(),
        prefix,
        routers: get("router").split_whitespace().filter_map(|r| r.parse().ok()).collect(),
    }
}

/// The script udhcpc runs: it writes the lease's values, which are only ever parsed (never sourced), to `lease`.
pub(crate) fn script_text(lease: &std::path::Path) -> String {
    format!(
        "#!/bin/sh\n# written by f50d: hands udhcpc's lease to f50d, which applies and checks it\n\
case $1 in bound|renew|deconfig) ;; *) exit 0 ;; esac\numask 077\nf='{}'\n\
printf 'event=%s\\nip=%s\\nmask=%s\\nrouter=%s\\n' \"$1\" \"${{ip:-}}\" \"${{mask:-}}\" \"${{router:-}}\" > \"$f.tmp\" && mv -f \"$f.tmp\" \"$f\"\n",
        lease.display()
    )
}

pub(crate) struct Files {
    pub lease: PathBuf,
    pub script: PathBuf,
    pub pid: PathBuf,
}

pub(crate) fn files(net: &Net) -> Files {
    let d = net.run_dir();
    Files {
        lease: d.join("wifi-client.lease"),
        script: d.join("wifi-client.udhcpc"),
        pid: d.join("wifi-client.udhcpc.pid"),
    }
}

/// The lease udhcpc reported last.
pub(crate) fn read(net: &Net) -> Option<Lease> {
    fsx::read_opt(&files(net).lease).ok().flatten().map(|t| parse(&t))
}

/// Asks the other network for an address; returns the lease (`None`: no answer). udhcpc stays for the renewals.
pub(crate) async fn request(net: &Net) -> Result<Option<Lease>> {
    let f = files(net);
    fsx::private_dir(&net.run_dir())?;
    fsx::write_atomic(&f.script, script_text(&f.lease).as_bytes(), 0o700)?;
    let _ = std::fs::remove_file(&f.lease);
    let ifname = net.cfg().wlan.clone();
    let script = f.script.to_string_lossy().into_owned();
    let pid = f.pid.to_string_lossy().into_owned();
    let o = Cmd::new("udhcpc")
        .args(["-i", &ifname, "-n", "-t", "8", "-s", &script, "-p", &pid])
        .leaves_daemon()
        .timeout(Duration::from_secs(60))
        .output()
        .await?;
    if !o.success() {
        return Ok(None);
    }
    Ok(read(net).filter(|l| l.ip.is_some()))
}

/// The lease on wlan0: its address, and the default route through it with the uplink's metric.
pub(crate) async fn apply(net: &Net, l: &Lease, metric: u32) -> Result<()> {
    let ifname = net.cfg().wlan.as_str();
    let ip = l.ip.ok_or_else(|| Error::failed("the lease has no address"))?;
    tools::ip_quiet(&["-4", "addr", "flush", "dev", ifname]).await;
    tools::ip(&["-4", "addr", "add", &format!("{ip}/{}", l.prefix), "dev", ifname]).await?;
    tools::ip_quiet(&["link", "set", ifname, "up"]).await;
    let metric = metric.to_string();
    for r in &l.routers {
        let r = r.to_string();
        if tools::ip_quiet(&["-4", "route", "replace", "default", "via", &r, "dev", ifname, "metric", &metric]).await {
            break;
        }
    }
    Ok(())
}

/// The address and the routes this network gave: gone when the link is (a default route via a network the device is
/// no longer on would hold the traffic that mobile data could carry).
pub(crate) async fn drop_all(net: &Net) {
    let f = files(net);
    if let Some(pid) = fsx::read_line(&f.pid).filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        let _ = Cmd::new("kill").arg(&pid).timeout(Duration::from_secs(5)).output().await;
    }
    for p in [&f.lease, &f.pid] {
        let _ = std::fs::remove_file(p);
    }
    let ifname = net.cfg().wlan.as_str();
    tools::ip_quiet(&["route", "flush", "dev", ifname]).await;
    tools::ip_quiet(&["-4", "addr", "flush", "dev", ifname]).await;
    tools::ip_quiet(&["-6", "addr", "flush", "dev", ifname, "scope", "global"]).await;
    // an RA's default route (the link-local route stays: the interface keeps its fe80 address)
    for _ in 0..8 {
        if !tools::ip_quiet(&["-6", "route", "del", "default", "dev", ifname]).await {
            break;
        }
    }
}

/// Only the IPv4 address goes (a lost lease: the rest is redone with the next).
pub(crate) async fn flush_address(net: &Net) {
    tools::ip_quiet(&["-4", "addr", "flush", "dev", &net.cfg().wlan]).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_script_output() {
        let l = parse("event=bound\nip=192.168.1.37\nmask=24\nrouter=192.168.1.1 192.168.1.2\n");
        assert_eq!(l.event, "bound");
        assert_eq!(l.net(), Some(("192.168.1.37".parse().unwrap(), 24)));
        assert_eq!(l.routers.len(), 2);
        assert_eq!(parse("ip=10.0.0.5\nmask=255.255.0.0\n").prefix, 16);
        assert_eq!(parse("event=deconfig\nip=\n").ip, None);
    }

    #[test]
    fn script_quotes_values() {
        let s = script_text(std::path::Path::new("/var/run/f50d/wifi-client.lease"));
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("\"${ip:-}\""));
        assert!(s.contains("f='/var/run/f50d/wifi-client.lease'"));
    }
}

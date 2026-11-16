//! The interception (`f50-proxy/DESIGN.md` §2 "Firewall"), only while the proxy runs.
//!
//! - nftables: one table, `inet f50_proxy`, replaced whole and atomically (`delete table` and the definition in one
//!   `nft -f` file under the run directory). `prerouting` sends what LAN clients (and the router's own traffic,
//!   re-routed through `lo` with mark 1) open to the engine's tproxy inbound; `output` marks the router's own traffic
//!   when `intercept_router`. The engine's own sockets carry mark 0xff and are never caught.
//! - Policy routing: packets with fwmark 1 are delivered locally (`ip rule fwmark 1 lookup 117 pref 1170`, `ip route
//!   local default dev lo table 117`), IPv4 and IPv6. Table 117 and preference 1170 are used by nothing else in f50d
//!   or the image's overlay (checked when this was written).
//! - The LAN interfaces: the LAN bridge netifd reports (`br-lan`), and the USB gadget's netdev when it is not a port
//!   of that bridge (its own bridge, or none).
//!
//! fw4 reloads only its own table (`inet fw4`), so a firewall reload leaves this one alone.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::rpc::matches;
use f50_core::{Result, fsx};
use f50_platform::Platform;

pub(crate) const TABLE: &str = "f50_proxy";
/// The mark the interception sets; the policy rule delivers it locally.
pub(crate) const FWMARK: u32 = 1;
pub(crate) const ROUTE_TABLE: u32 = 117;
pub(crate) const RULE_PREF: u32 = 1170;

const SHORT: Duration = Duration::from_secs(10);

/// Never intercepted: private, loopback, link-local, multicast, CGNAT and reserved ranges.
const BYPASS4: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.0.2.0/24",
    "192.168.0.0/16",
    "198.18.0.0/15",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "224.0.0.0/4",
    "240.0.0.0/4",
];
const BYPASS6: &[&str] =
    &["::/127", "::ffff:0:0/96", "64:ff9b::/96", "100::/64", "2001:db8::/32", "fc00::/7", "fe80::/10", "ff00::/8"];

/// The gadget netdevs that may carry a USB host's traffic.
const USB_NETDEVS: &[&str] = &["usb0", "rndis0"];

/// What the ruleset is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub lan_if: Vec<String>,
    /// Lower-case `aa:bb:cc:dd:ee:ff`.
    pub bypass_macs: Vec<String>,
    pub intercept_router: bool,
    pub tproxy_port: u16,
    /// The engine's own mark.
    pub mark: u32,
}

/// An interface name nft may quote: IFNAMSIZ, no quote or blank.
pub(crate) fn ifname_ok(s: &str) -> bool {
    s.len() <= 15 && matches("[A-Za-z0-9][A-Za-z0-9_.:@-]*", s)
}

pub(crate) fn mac_ok(s: &str) -> bool {
    matches("[0-9a-f]{2}(:[0-9a-f]{2}){5}", s)
}

fn list(items: impl IntoIterator<Item = String>) -> String {
    items.into_iter().collect::<Vec<_>>().join(", ")
}

/// The whole `nft -f` file: the old table removed (made first so the delete never fails) and the new one.
pub(crate) fn ruleset(s: &Spec) -> String {
    let mut t = String::new();
    let port = s.tproxy_port;
    let mark = s.mark;
    let _ = writeln!(t, "table inet {TABLE}\ndelete table inet {TABLE}\ntable inet {TABLE} {{");
    let _ = writeln!(
        t,
        "\tset bypass4 {{\n\t\ttype ipv4_addr\n\t\tflags interval\n\t\telements = {{ {} }}\n\t}}",
        BYPASS4.join(", ")
    );
    let _ = writeln!(
        t,
        "\tset bypass6 {{\n\t\ttype ipv6_addr\n\t\tflags interval\n\t\telements = {{ {} }}\n\t}}",
        BYPASS6.join(", ")
    );
    let ifs: Vec<String> = s.lan_if.iter().filter(|i| ifname_ok(i)).map(|i| format!("\"{i}\"")).collect();
    let _ = write!(t, "\tset lan_if {{\n\t\ttype ifname\n");
    if !ifs.is_empty() {
        let _ = writeln!(t, "\t\telements = {{ {} }}", list(ifs));
    }
    let _ = writeln!(t, "\t}}");
    let macs: Vec<String> = s.bypass_macs.iter().filter(|m| mac_ok(m)).cloned().collect();
    let _ = write!(t, "\tset bypass_mac {{\n\t\ttype ether_addr\n");
    if !macs.is_empty() {
        let _ = writeln!(t, "\t\telements = {{ {} }}", list(macs));
    }
    let _ = writeln!(t, "\t}}");

    let dns4 = format!(
        "meta nfproto ipv4 meta l4proto {{ tcp, udp }} th dport 53 fib daddr type != local meta mark set {FWMARK:#x}"
    );
    let dns6 = format!(
        "meta nfproto ipv6 meta l4proto {{ tcp, udp }} th dport 53 fib daddr type != local meta mark set {FWMARK:#x}"
    );
    let bypass = ["fib daddr type local return", "ip daddr @bypass4 return", "ip6 daddr @bypass6 return"];

    let _ = writeln!(t, "\tchain prerouting {{\n\t\ttype filter hook prerouting priority mangle; policy accept;");
    let _ = writeln!(t, "\t\tmeta mark {mark:#x} return");
    let _ = writeln!(t, "\t\tiifname != \"lo\" iifname != @lan_if return");
    let _ = writeln!(t, "\t\tiifname \"lo\" meta mark != {FWMARK:#x} return");
    let _ = writeln!(t, "\t\tiifname @lan_if ether saddr @bypass_mac return");
    let _ = writeln!(t, "\t\t{dns4} tproxy ip to 127.0.0.1:{port} accept");
    let _ = writeln!(t, "\t\t{dns6} tproxy ip6 to [::1]:{port} accept");
    for b in bypass {
        let _ = writeln!(t, "\t\t{b}");
    }
    let _ = writeln!(
        t,
        "\t\tmeta nfproto ipv4 meta l4proto {{ tcp, udp }} meta mark set {FWMARK:#x} tproxy ip to 127.0.0.1:{port} accept"
    );
    let _ = writeln!(
        t,
        "\t\tmeta nfproto ipv6 meta l4proto {{ tcp, udp }} meta mark set {FWMARK:#x} tproxy ip6 to [::1]:{port} accept"
    );
    let _ = writeln!(t, "\t}}");

    if s.intercept_router {
        let _ = writeln!(t, "\tchain output {{\n\t\ttype route hook output priority mangle; policy accept;");
        let _ = writeln!(t, "\t\tmeta mark {mark:#x} return");
        let _ = writeln!(t, "\t\t{dns4} return");
        let _ = writeln!(t, "\t\t{dns6} return");
        for b in bypass {
            let _ = writeln!(t, "\t\t{b}");
        }
        let _ = writeln!(t, "\t\tmeta l4proto {{ tcp, udp }} meta mark set {FWMARK:#x}");
        let _ = writeln!(t, "\t}}");
    }
    let _ = writeln!(t, "}}");
    t
}

/// Only the removal (the table made first, so it never fails for a table that is not there).
pub(crate) fn removal() -> String {
    format!("table inet {TABLE}\ndelete table inet {TABLE}\n")
}

/// The LAN interfaces: netifd's device of `lan` (else `default_bridge`), and each gadget netdev that is not one of
/// its ports (the bridge it is in instead, or itself).
pub(crate) async fn lan_interfaces(platform: &Platform, lan: &str, default_bridge: &str) -> Vec<String> {
    let bridge = platform.netifd().interface_device(lan).await.ok().flatten().filter(|d| ifname_ok(d));
    let bridge = bridge.unwrap_or_else(|| default_bridge.to_owned());
    let mut out = vec![bridge.clone()];
    let net = platform.paths().sys("/sys/class/net");
    for dev in USB_NETDEVS {
        let d = net.join(dev);
        if !d.exists() {
            continue;
        }
        let master = std::fs::read_link(d.join("master"))
            .ok()
            .and_then(|l| l.file_name().map(|n| n.to_string_lossy().into_owned()));
        let iface = match master {
            Some(m) if m == bridge => continue,
            Some(m) => m,
            None => (*dev).to_owned(),
        };
        if ifname_ok(&iface) && !out.contains(&iface) {
            out.push(iface);
        }
    }
    out
}

async fn ip(args: &[&str]) -> Result<()> {
    Cmd::new("ip").args(args).timeout(SHORT).ok().await.map(drop)
}

async fn ip_quiet(args: &[&str]) -> bool {
    Cmd::new("ip").args(args).timeout(SHORT).output().await.is_ok_and(|o| o.success())
}

/// The policy rule and the local route, both families. A rule left by an earlier run is replaced, not doubled.
pub(crate) async fn routes_up() -> Result<()> {
    let (mark, table, pref) = (FWMARK.to_string(), ROUTE_TABLE.to_string(), RULE_PREF.to_string());
    for fam in ["-4", "-6"] {
        for _ in 0..4 {
            if !ip_quiet(&[fam, "rule", "del", "pref", &pref]).await {
                break;
            }
        }
        ip(&[fam, "rule", "add", "fwmark", &mark, "lookup", &table, "pref", &pref]).await?;
        ip(&[fam, "route", "replace", "local", "default", "dev", "lo", "table", &table]).await?;
    }
    Ok(())
}

pub(crate) async fn routes_down() {
    let (table, pref) = (ROUTE_TABLE.to_string(), RULE_PREF.to_string());
    for fam in ["-4", "-6"] {
        for _ in 0..4 {
            if !ip_quiet(&[fam, "rule", "del", "pref", &pref]).await {
                break;
            }
        }
        ip_quiet(&[fam, "route", "flush", "table", &table]).await;
    }
}

/// `nft -f` of `text`, written to `file` first (kept for whoever looks at what was applied).
pub(crate) async fn nft(file: &Path, text: &str) -> Result<()> {
    fsx::write_atomic(file, text.as_bytes(), 0o600)?;
    Cmd::new("nft").arg("-f").arg(file).timeout(SHORT).ok().await.map(drop).map_err(|e| e.context("nft"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec {
            lan_if: vec!["br-lan".into(), "usb0".into(), "bad\"name".into()],
            bypass_macs: vec!["aa:bb:cc:dd:ee:ff".into(), "nope".into()],
            intercept_router: true,
            tproxy_port: 17892,
            mark: 255,
        }
    }

    #[test]
    fn ruleset_with_router_and_macs() {
        let t = ruleset(&spec());
        assert!(t.starts_with("table inet f50_proxy\ndelete table inet f50_proxy\ntable inet f50_proxy {\n"));
        assert!(t.contains("elements = { \"br-lan\", \"usb0\" }"), "{t}");
        assert!(!t.contains("bad"));
        assert!(t.contains("type ether_addr\n\t\telements = { aa:bb:cc:dd:ee:ff }\n"));
        assert!(t.contains("100.64.0.0/10"));
        assert!(t.contains("fe80::/10"));
        assert!(t.contains("meta mark 0xff return"));
        assert!(t.contains("iifname != \"lo\" iifname != @lan_if return"));
        assert!(t.contains("iifname \"lo\" meta mark != 0x1 return"));
        assert!(t.contains("iifname @lan_if ether saddr @bypass_mac return"));
        assert!(t.contains("tproxy ip to 127.0.0.1:17892 accept"));
        assert!(t.contains("tproxy ip6 to [::1]:17892 accept"));
        // the DNS rule comes before the bypass sets, so carrier DNS on 10.x is still split
        let dns = t.find("th dport 53").expect("dns");
        let bypass = t.find("ip daddr @bypass4 return").expect("bypass");
        assert!(dns < bypass);
        assert!(t.contains("chain output {\n\t\ttype route hook output priority mangle; policy accept;"));
        assert!(t.contains("\t\tmeta l4proto { tcp, udp } meta mark set 0x1\n"));
        assert_eq!(t.matches('{').count(), t.matches('}').count());
        assert!(t.ends_with("}\n"));
    }

    #[test]
    fn ruleset_without_router_or_macs() {
        let t = ruleset(&Spec { intercept_router: false, bypass_macs: vec![], ..spec() });
        assert!(!t.contains("chain output"));
        assert!(!t.contains("hook output"));
        assert!(t.contains("set bypass_mac {\n\t\ttype ether_addr\n\t}"), "an empty set has no elements: {t}");
        assert!(t.contains("chain prerouting"));
        assert_eq!(t.matches('{').count(), t.matches('}').count());
        assert_eq!(removal(), "table inet f50_proxy\ndelete table inet f50_proxy\n");
    }

    #[test]
    fn names_and_macs() {
        assert!(ifname_ok("br-lan") && ifname_ok("usb0") && ifname_ok("wlan0.1"));
        assert!(!ifname_ok("") && !ifname_ok("a b") && !ifname_ok("x\"") && !ifname_ok("abcdefghijklmnop"));
        assert!(mac_ok("aa:bb:cc:00:11:22") && !mac_ok("AA:BB:CC:00:11:22") && !mac_ok("aa:bb:cc:00:11"));
    }
}

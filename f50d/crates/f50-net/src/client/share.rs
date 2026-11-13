//! Sharing the Wi-Fi uplink with the LAN, with mobile data's model: the other network is treated as the cellular
//! side is.
//!
//! * LAN clients reach the internet through it, and nothing else on that network: forwarded traffic to private,
//!   link-local, shared (CGNAT), multicast and other special-purpose addresses, to the network's own subnet, and all
//!   IPv6, is dropped. A guest on this device's USB does not get onto the home or hotel network it is a client of.
//! * From the other network only answers come in: nothing is forwarded to the LAN, and the device's own services are
//!   closed to it. DHCP, ping and IPv6 neighbour and router discovery are let in.
//!
//! Three states, each put up by one nft transaction that replaces both tables whole: `closed` (from before the
//! supplicant starts until DHCP has answered, and whenever the link is lost), `shared` (the address is known and
//! checked), `none` (wlan0 is on no network any more). The state is what nft says, never what was last set: after
//! every transaction nft's own listing of the filter table is compared, rule for rule (counters aside), with the one
//! the state asks for. fw4 does the NAT (wlan0 in its wan zone); these filter tables come on top. Ported from
//! `wifi-client` (`rules_text`, `fw_state`, `wan_zone`); the VPN kill-switch hooks are not carried over.

use std::net::Ipv4Addr;

use f50_core::{Error, Result};
use f50_platform::services::Action;
use f50_platform::uci::Change;

use crate::Net;
use crate::tools::{self, Listing};

pub(crate) const NAT: &str = "f50_wifi_nat";
pub(crate) const FILTER: &str = "f50_wifi_filter";
/// The tables' names before the rename: removed with every transaction, so a device has one set only.
const LEGACY_NAT: &str = "mu300_wifi_nat";
const LEGACY_FILTER: &str = "mu300_wifi_filter";
/// The firewall rule that keeps LAN clients off the other network's private addresses (fw4 side).
pub(crate) const PRIVATE_RULE: &str = "f50-wifi-client-private";
const PRIVATE_SECTION: &str = "f50_wifi_private";
/// The rule's names before the rename: a device that has it under them keeps it (it is the same rule).
const LEGACY_PRIVATE_RULE: &str = "mu300-wifi-client-private";
const LEGACY_PRIVATE_SECTION: &str = "mu300_wifi_private";
/// Every IPv4 destination that is not the internet (224.0.0.0/3 is 224.0.0.0 to 255.255.255.255).
pub(crate) const PRIVATE4: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.168.0.0/16",
    "198.18.0.0/15",
    "224.0.0.0/3",
];

/// What the firewall tables are for wlan0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FwState {
    None,
    Closed,
    Shared,
    Unknown,
}

/// The transaction for a state. Each rule is written the way nft lists it back (the burst of `limit`, the order of an
/// icmpv6 type set), since [`state`] compares the two line for line.
pub(crate) fn rules_text(st: FwState, ifname: &str, subnet: Option<(Ipv4Addr, u8)>) -> String {
    let mut t = format!("table ip {NAT}\ndelete table ip {NAT}\ntable inet {FILTER}\ndelete table inet {FILTER}\n");
    t.push_str(&format!(
        "table ip {LEGACY_NAT}\ndelete table ip {LEGACY_NAT}\ntable inet {LEGACY_FILTER}\ndelete table inet {LEGACY_FILTER}\n"
    ));
    if matches!(st, FwState::None | FwState::Unknown) {
        return t;
    }
    let i = format!("iifname \"{ifname}\"");
    let o = format!("oifname \"{ifname}\"");
    t.push_str(&format!("table inet {FILTER} {{\n    chain input {{\n"));
    t.push_str("        type filter hook input priority filter; policy accept;\n");
    t.push_str(&format!("        {i} ct state established,related accept\n"));
    t.push_str(&format!("        {i} udp sport 67 udp dport 68 accept\n"));
    t.push_str(&format!("        {i} icmp type echo-request limit rate 5/second burst 5 packets accept\n"));
    t.push_str(&format!(
        "        {i} icmpv6 type {{ echo-request, nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert }} accept\n"
    ));
    t.push_str(&format!("        {i} counter drop\n    }}\n    chain forward {{\n"));
    t.push_str("        type filter hook forward priority filter; policy accept;\n");
    if st == FwState::Shared {
        t.push_str(&format!("        {i} ct state established,related accept\n"));
        t.push_str(&format!("        {i} counter drop\n"));
        t.push_str(&format!("        {o} ip daddr {{ {} }} counter drop\n", PRIVATE4.join(", ")));
        if let Some((a, p)) = subnet {
            t.push_str(&format!("        {o} ip daddr {}/{p} counter drop\n", crate::lan::network(a, p)));
        }
        t.push_str(&format!("        {o} meta nfproto ipv6 counter drop\n"));
        t.push_str(&format!("        {o} tcp flags syn tcp option maxseg size set rt mtu\n"));
    } else {
        t.push_str(&format!("        {i} counter drop\n"));
        t.push_str(&format!("        {o} counter drop\n"));
    }
    t.push_str("    }\n}\n");
    t
}

/// The filter table as nft lists it (or as [`rules_text`] writes it), without indentation, counters or empty lines.
pub(crate) fn normal(text: &str) -> Vec<String> {
    let head = format!("table inet {FILTER} {{");
    let mut on = false;
    let mut out = Vec::new();
    for l in text.lines() {
        let mut l = l.trim().to_owned();
        if let Some(i) = l.find("counter packets ") {
            // "counter packets N bytes M" -> "counter"
            let rest = &l[i + "counter packets ".len()..];
            let tail: Vec<&str> = rest.splitn(4, ' ').collect();
            if tail.len() >= 3 && tail[1] == "bytes" {
                let after = tail.get(3).map_or(String::new(), |t| format!(" {t}"));
                l = format!("{}counter{after}", &l[..i]);
            }
        }
        l = l.replace("limit rate 5/second burst 5 packets", "limit rate 5/second");
        if l == head {
            on = true;
        }
        if on && !l.is_empty() {
            out.push(l);
        }
    }
    out
}

/// What nft says the tables are; `subnet` is the one `shared` must name.
pub(crate) async fn state(ifname: &str, subnet: Option<(Ipv4Addr, u8)>) -> FwState {
    match tools::nft_list("inet", FILTER).await {
        Listing::Absent => FwState::None,
        Listing::Unknown => FwState::Unknown,
        Listing::Table(t) => {
            let got = normal(&t);
            if got == normal(&rules_text(FwState::Closed, ifname, None)) {
                FwState::Closed
            } else if got == normal(&rules_text(FwState::Shared, ifname, subnet)) {
                FwState::Shared
            } else {
                FwState::Unknown
            }
        }
    }
}

/// The transaction, and nft's own listing that it took.
pub(crate) async fn put(st: FwState, ifname: &str, subnet: Option<(Ipv4Addr, u8)>) -> Result<()> {
    tools::nft_apply(&rules_text(st, ifname, subnet)).await?;
    let now = state(ifname, subnet).await;
    if now == st { Ok(()) } else { Err(Error::failed(format!("the firewall for {ifname} is not as asked (nft)"))) }
}

/// `none`: both tables gone (each on its own when the one transaction is refused); tables nft still shows are left
/// closed.
pub(crate) async fn remove(ifname: &str) {
    if tools::nft_apply(&rules_text(FwState::None, ifname, None)).await.is_err() {
        let _ = tools::nft_apply(&format!("delete table ip {NAT}\n")).await;
        let _ = tools::nft_apply(&format!("delete table inet {FILTER}\n")).await;
        let _ = tools::nft_apply(&format!("delete table ip {LEGACY_NAT}\n")).await;
        let _ = tools::nft_apply(&format!("delete table inet {LEGACY_FILTER}\n")).await;
    }
    if state(ifname, None).await != FwState::None && put(FwState::Closed, ifname, None).await.is_err() {
        tools::logger("wifi-client", &format!("the firewall tables for {ifname} could not be removed or closed (nft)"))
            .await;
    }
}

/// fw4: wlan0 in the wan zone (NAT, and input and forwarding from it rejected but for answers, as for the modem),
/// and a rule that keeps LAN clients off the other network's private addresses (fw4 forwards lan to wan to any
/// address). When wlan0 is the hotspot it is a port of br-lan and fw4 only ever sees br-lan, so neither does anything
/// then; done once, left in place. No wan zone: no NAT either, so no join.
pub(crate) async fn wan_zone(net: &Net) -> Result<()> {
    let ifname = &net.cfg().wlan;
    let fw = net.platform().uci().load("firewall").await?;
    let zone = fw
        .iter()
        .find(|s| s.stype == "zone" && s.get("name") == Some("wan"))
        .ok_or_else(|| Error::failed("the firewall has no wan zone: the Wi-Fi uplink cannot be shared"))?;
    let mut out = Vec::new();
    if !zone.list("device").iter().any(|d| d == ifname) {
        out.push(Change::AddList { section: zone.name.clone(), option: "device".into(), value: ifname.clone() });
    }
    let have_rule = fw.iter().any(|s| {
        [PRIVATE_SECTION, LEGACY_PRIVATE_SECTION].contains(&s.name.as_str())
            || s.get("name").is_some_and(|n| [PRIVATE_RULE, LEGACY_PRIVATE_RULE].contains(&n))
    });
    if !have_rule {
        let s = PRIVATE_SECTION.to_owned();
        out.push(Change::Section { section: s.clone(), stype: "rule".into() });
        for (k, v) in [
            ("name", PRIVATE_RULE),
            ("src", "lan"),
            ("dest", "wan"),
            ("device", ifname.as_str()),
            ("direction", "out"),
            ("family", "ipv4"),
            ("proto", "all"),
            ("target", "REJECT"),
        ] {
            out.push(Change::Set { section: s.clone(), option: k.into(), value: v.into() });
        }
        for a in PRIVATE4 {
            out.push(Change::AddList { section: s.clone(), option: "dest_ip".into(), value: (*a).into() });
        }
    }
    if out.is_empty() {
        return Ok(());
    }
    // all of it or none: a half-built rule would never be repaired (the name check skips it)
    net.platform().uci().apply("firewall", &out).await?;
    net.platform().services().run_logged("firewall", Action::Reload).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_text() {
        let t = rules_text(FwState::Closed, "wlan0", None);
        assert!(t.starts_with(
            "table ip f50_wifi_nat\ndelete table ip f50_wifi_nat\ntable inet f50_wifi_filter\ndelete table inet f50_wifi_filter\n\
             table ip mu300_wifi_nat\ndelete table ip mu300_wifi_nat\ntable inet mu300_wifi_filter\ndelete table inet mu300_wifi_filter\n"
        ));
        assert!(
            t.contains("        iifname \"wlan0\" counter drop\n        oifname \"wlan0\" counter drop\n    }\n}\n")
        );
        assert!(!t.contains("masquerade"));
    }

    #[test]
    fn shared_text() {
        let t = rules_text(FwState::Shared, "wlan0", Some(("192.168.1.37".parse().unwrap(), 24)));
        assert!(t.contains(
            "oifname \"wlan0\" ip daddr { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 169.254.0.0/16, 172.16.0.0/12, \
             192.0.0.0/24, 192.168.0.0/16, 198.18.0.0/15, 224.0.0.0/3 } counter drop\n"
        ));
        assert!(t.contains("oifname \"wlan0\" ip daddr 192.168.1.0/24 counter drop\n"));
        assert!(t.contains("oifname \"wlan0\" tcp flags syn tcp option maxseg size set rt mtu\n"));
        assert_eq!(
            rules_text(FwState::None, "wlan0", None).lines().count(),
            8,
            "ours and the tables of before the rename"
        );
    }

    #[test]
    fn listing_compares_without_counters() {
        let listed = "table inet f50_wifi_filter {\n\tchain input {\n\t\ttype filter hook input priority filter; policy accept;\n\
\t\tiifname \"wlan0\" ct state established,related accept\n\t\tiifname \"wlan0\" udp sport 67 udp dport 68 accept\n\
\t\tiifname \"wlan0\" icmp type echo-request limit rate 5/second burst 5 packets accept\n\
\t\tiifname \"wlan0\" icmpv6 type { echo-request, nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert } accept\n\
\t\tiifname \"wlan0\" counter packets 12 bytes 3456 drop\n\t}\n\n\tchain forward {\n\
\t\ttype filter hook forward priority filter; policy accept;\n\t\tiifname \"wlan0\" counter packets 0 bytes 0 drop\n\
\t\toifname \"wlan0\" counter packets 0 bytes 0 drop\n\t}\n}\n";
        assert_eq!(normal(listed), normal(&rules_text(FwState::Closed, "wlan0", None)));
        let extra = listed.replace("\t}\n}\n", "\t\toifname \"wlan0\" accept\n\t}\n}\n");
        assert_ne!(normal(&extra), normal(&rules_text(FwState::Closed, "wlan0", None)));
    }
}

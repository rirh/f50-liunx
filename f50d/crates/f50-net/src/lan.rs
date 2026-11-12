//! The LAN: its address and DHCP range in UCI (`network.lan`, `dhcp.lan`), and the IPv4 arithmetic the Wi-Fi client
//! and the USB port need (subnets, overlaps, the USB host's address). Ported from `mu300-api lan.*`, `mu300-lan-ip`
//! and `lib/mu300/usb-host.sh`.

use std::net::Ipv4Addr;
use std::time::Duration;

use f50_core::rpc::{Params, check_word};
use f50_core::{Error, Result};
use f50_platform::services::Action;
use f50_platform::uci::{Change, Section};
use serde_json::{Value, json};

use crate::{Net, rules};

/// `A.B.C.D/N` (or a bare address: /32).
pub(crate) fn parse_cidr(s: &str) -> Option<(Ipv4Addr, u8)> {
    let (a, p) = s.split_once('/').unwrap_or((s, "32"));
    let p: u8 = p.parse().ok()?;
    (p <= 32).then_some(())?;
    Some((a.parse().ok()?, p))
}

/// A dotted netmask's prefix length, `None` when its ones are not contiguous.
pub(crate) fn prefix_of(mask: Ipv4Addr) -> Option<u8> {
    let m = u32::from(mask);
    let p = m.leading_ones();
    (m.checked_shl(p).unwrap_or(0) == 0).then(|| u8::try_from(p).ok()).flatten()
}

pub(crate) fn mask_of(prefix: u8) -> Ipv4Addr {
    let p = u32::from(prefix.min(32));
    Ipv4Addr::from(u32::MAX.checked_shl(32 - p).unwrap_or(0))
}

/// The network address of `addr/prefix`.
pub(crate) fn network(addr: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(addr) & u32::from(mask_of(prefix)))
}

/// Whether two IPv4 networks share any address (the other network on the LAN's own subnet would make the LAN's
/// addresses ambiguous: replies to a USB client could leave on wlan0).
pub(crate) fn overlaps(a: (Ipv4Addr, u8), b: (Ipv4Addr, u8)) -> bool {
    let p = a.1.min(b.1);
    network(a.0, p) == network(b.0, p)
}

/// The USB host's address on the LAN (preinit and uci-defaults give it): .200 of the router's /24 when that is
/// inside the subnet and not the router itself (.199 then); in a smaller subnet, the last host address that is not
/// the router.
pub(crate) fn usb_host_ip(router: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    let (r, m) = (u32::from(router), u32::from(mask));
    let net = r & m;
    let bc = net | !m;
    let mut h = (r & 0xffff_ff00) | 200;
    if h == r {
        h -= 1;
    }
    if h <= net || h >= bc {
        h = bc.wrapping_sub(1);
        if h == r {
            h = bc.wrapping_sub(2);
        }
    }
    Ipv4Addr::from(h)
}

/// The LAN as netifd sets it from `ipaddr` (`A.B.C.D` or `A.B.C.D/N`, maybe a list) and `netmask`; a /24 when
/// neither gives a prefix. `None` when `ipaddr` is no address.
pub(crate) fn lan_of(section: &Section) -> Option<(Ipv4Addr, u8)> {
    let first = section.get("ipaddr")?.split_whitespace().next()?;
    match first.split_once('/') {
        Some((a, p)) => {
            let p: u8 = p.parse().ok().filter(|p| (8..=30).contains(p)).unwrap_or(24);
            Some((a.parse().ok()?, p))
        }
        None => {
            let p = section.get("netmask").and_then(|m| m.parse().ok()).and_then(prefix_of).unwrap_or(24);
            Some((first.parse().ok()?, p))
        }
    }
}

async fn lan_section(net: &Net) -> Result<Option<Section>> {
    let want = &net.cfg().lan_interface;
    Ok(net.platform().uci().load("network").await?.into_iter().find(|s| &s.name == want))
}

/// The LAN's subnet from its configuration (br-lan has no address yet when the boot join runs).
pub(crate) async fn lan_net(net: &Net) -> Option<(Ipv4Addr, u8)> {
    lan_section(net).await.ok().flatten().as_ref().and_then(lan_of)
}

/// `f50.lan get`.
pub(crate) async fn get(net: &Net) -> Result<Value> {
    let sec = lan_section(net).await?;
    let (addr, prefix) = sec.as_ref().and_then(lan_of).unwrap_or((net.cfg().default_lan_ip, 24));
    let live = crate::tools::ipv4_of(&net.cfg().lan_bridge).await;
    let dhcp = net.platform().uci().load("dhcp").await.unwrap_or_default();
    let d = dhcp.iter().find(|s| s.stype == "dhcp" && s.get("interface") == Some(net.cfg().lan_interface.as_str()));
    let num = |k: &str| d.and_then(|d| d.get(k)).and_then(|v| v.parse::<i64>().ok());
    Ok(json!({
        "address": addr.to_string(),
        "netmask": mask_of(prefix).to_string(),
        "prefix": prefix,
        "live": live.map(|(a, p)| format!("{a}/{p}")),
        "default_address": net.cfg().default_lan_ip.to_string(),
        "usb_host_address": usb_host_ip(addr, mask_of(prefix)).to_string(),
        "dhcp": {
            "enabled": d.is_some_and(|d| d.get("ignore") != Some("1")),
            "start": num("start"),
            "limit": num("limit"),
            "leasetime": d.and_then(|d| d.get("leasetime")),
        },
    }))
}

const MASKS: &str = "255\\.255\\.(0|128|192|224|240|248|252|254|255)\\.0|255\\.255\\.255\\.(0|128|192|224|240|248|252)";
const LEASETIME: &str = "[1-9][0-9]{0,4}[mh]|infinite";

/// `f50.lan set`: the address (a private /24's .1, as `mu300-api` took it), the netmask, the DHCP range. Saved in
/// UCI; netifd and dnsmasq reload two seconds later, so the reply leaves before the LAN moves.
pub(crate) async fn set(net: &Net, p: &Params) -> Result<Value> {
    let lan = net.cfg().lan_interface.clone();
    let sec = lan_section(net).await?.ok_or_else(|| Error::unsupported("network.lan"))?;
    let mut changes = Vec::new();
    let mut prefix = lan_of(&sec).map_or(24, |l| l.1);
    if let Some(m) = p.str("netmask") {
        check_word("netmask", m, MASKS, 15)?;
        prefix = m.parse().ok().and_then(prefix_of).ok_or_else(|| Error::bad_value("netmask"))?;
    }
    let cidr_form = sec.get("ipaddr").is_some_and(|a| a.contains('/'));
    if let Some(a) = p.str("address") {
        // checked by the field rule already; the address is the network's first host
        let ip = if cidr_form { format!("{a}/{prefix}") } else { a.to_owned() };
        changes.push(set_opt(&lan, "ipaddr", ip));
    }
    if p.has("netmask") {
        if cidr_form && p.str("address").is_none() {
            let a = lan_of(&sec).map(|l| l.0).ok_or_else(|| Error::missing("address"))?;
            changes.push(set_opt(&lan, "ipaddr", format!("{a}/{prefix}")));
        } else if !cidr_form {
            changes.push(set_opt(&lan, "netmask", mask_of(prefix).to_string()));
        }
    }
    let dhcp_changes = dhcp_changes(net, p).await?;
    if changes.is_empty() && dhcp_changes.is_empty() {
        return Err(Error::missing("address"));
    }
    if !changes.is_empty() {
        net.platform().uci().apply("network", &changes).await?;
    }
    let dhcp = !dhcp_changes.is_empty();
    if dhcp {
        net.platform().uci().apply("dhcp", &dhcp_changes).await?;
    }
    let network = !changes.is_empty();
    net.later("lan-reload", Duration::from_secs(2), move |net| async move {
        if network {
            net.platform().netifd().reload().await?;
        }
        if dhcp {
            net.platform().services().run("dnsmasq", Action::Reload).await?;
        }
        Ok(())
    });
    Ok(json!({ "restart": true }))
}

async fn dhcp_changes(net: &Net, p: &Params) -> Result<Vec<Change>> {
    if !["dhcp", "dhcp_start", "dhcp_limit", "leasetime"].iter().any(|k| p.has(k)) {
        return Ok(Vec::new());
    }
    let lan = &net.cfg().lan_interface;
    let sections = net.platform().uci().load("dhcp").await?;
    let s = sections
        .iter()
        .find(|s| s.stype == "dhcp" && s.get("interface") == Some(lan.as_str()))
        .ok_or_else(|| Error::unsupported("dhcp.lan"))?;
    let mut out = Vec::new();
    if let Some(on) = p.bool("dhcp") {
        if on && s.get("ignore").is_some() {
            out.push(Change::Delete { section: s.name.clone(), option: Some("ignore".into()) });
        } else if !on {
            out.push(set_opt(&s.name, "ignore", "1".into()));
        }
    }
    let start = p.int("dhcp_start").or_else(|| s.get("start").and_then(|v| v.parse().ok())).unwrap_or(100);
    let limit = p.int("dhcp_limit").or_else(|| s.get("limit").and_then(|v| v.parse().ok())).unwrap_or(150);
    if start + limit > 255 {
        return Err(Error::bad_value(if p.has("dhcp_limit") { "dhcp_limit" } else { "dhcp_start" }));
    }
    if let Some(v) = p.int("dhcp_start") {
        out.push(set_opt(&s.name, "start", v.to_string()));
    }
    if let Some(v) = p.int("dhcp_limit") {
        out.push(set_opt(&s.name, "limit", v.to_string()));
    }
    if let Some(v) = p.str("leasetime") {
        check_word("leasetime", v, LEASETIME, 9)?;
        out.push(set_opt(&s.name, "leasetime", v.into()));
    }
    Ok(out)
}

pub(crate) fn set_opt(section: &str, option: &str, value: String) -> Change {
    Change::Set { section: section.into(), option: option.into(), value }
}

/// The address field's rule.
pub(crate) const ADDRESS: f50_core::rpc::Rule = f50_core::rpc::Rule::Word { re: rules::LAN_ADDRESS, max: 15 };

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use f50_platform::uci::Value as U;

    use super::*;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    #[test]
    fn arithmetic() {
        assert_eq!(parse_cidr("192.168.1.7/24"), Some((ip("192.168.1.7"), 24)));
        assert_eq!(parse_cidr("10.0.0.1/33"), None);
        assert_eq!(prefix_of(ip("255.255.255.0")), Some(24));
        assert_eq!(prefix_of(ip("255.255.0.255")), None);
        assert_eq!(mask_of(0), ip("0.0.0.0"));
        assert_eq!(mask_of(25), ip("255.255.255.128"));
        assert_eq!(network(ip("192.168.77.130"), 25), ip("192.168.77.128"));
        assert!(overlaps((ip("192.168.77.1"), 24), (ip("192.168.0.0"), 16)));
        assert!(!overlaps((ip("192.168.77.1"), 24), (ip("192.168.78.5"), 24)));
    }

    #[test]
    fn usb_host_address() {
        let m24 = ip("255.255.255.0");
        assert_eq!(usb_host_ip(ip("192.168.77.1"), m24), ip("192.168.77.200"));
        assert_eq!(usb_host_ip(ip("192.168.77.200"), m24), ip("192.168.77.199"));
        assert_eq!(usb_host_ip(ip("192.168.77.1"), ip("255.255.255.128")), ip("192.168.77.126"));
        assert_eq!(usb_host_ip(ip("192.168.77.126"), ip("255.255.255.128")), ip("192.168.77.125"));
    }

    #[test]
    fn lan_from_uci() {
        let sec = |o: &[(&str, &str)]| Section {
            name: "lan".into(),
            stype: "interface".into(),
            anonymous: false,
            options: o.iter().map(|(k, v)| ((*k).to_owned(), U::Str((*v).to_owned()))).collect::<BTreeMap<_, _>>(),
        };
        assert_eq!(
            lan_of(&sec(&[("ipaddr", "192.168.77.1"), ("netmask", "255.255.0.0")])),
            Some((ip("192.168.77.1"), 16))
        );
        assert_eq!(lan_of(&sec(&[("ipaddr", "10.1.2.1/25")])), Some((ip("10.1.2.1"), 25)));
        assert_eq!(lan_of(&sec(&[("ipaddr", "10.1.2.1")])), Some((ip("10.1.2.1"), 24)));
        assert_eq!(lan_of(&sec(&[("ipaddr", "nope")])), None);
        assert!(f50_core::rpc::matches(MASKS, "255.255.255.0"));
        assert!(!f50_core::rpc::matches(MASKS, "255.0.255.0"));
        assert!(f50_core::rpc::matches(LEASETIME, "12h"));
    }
}

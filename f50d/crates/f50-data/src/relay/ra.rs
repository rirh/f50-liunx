//! One router advertisement that withdraws prefixes from the LAN (ra-deprecate, #87): each prefix option gives the
//! /64 a preferred lifetime of 0 and a valid lifetime of two hours, so hosts that SLAACed an address in it stop
//! choosing it at once and drop it when the two hours are up (RFC 4862 5.5.3 e: a host never shortens a valid
//! lifetime below two hours from an unauthenticated RA). odhcpd's relay forwards the carrier's RAs as they are and
//! keeps no prefix state, and the carrier says nothing about a prefix it stopped using - so this is ours to say.
//!
//! Every RA also refreshes the sender as the hosts' default router, so the router lifetime is what is left of the
//! carrier's router on the bearer, never 0 (that would take the LAN's IPv6 default route away). Hop limit, reachable
//! time and retransmit timer stay unspecified and the M and O flags clear; the next relayed RA sets them again.

use std::net::Ipv6Addr;
use std::str::FromStr;

use f50_core::{Error, Result};
use f50_platform::Platform;

use super::sys;

const VALID: u32 = 7200;
const ND_ROUTER_ADVERT: u8 = 134;

/// The /64 of a `PREFIX::/64` string, if it is a global unicast one (2000::/3) and only the /64 itself.
pub fn global64(p: &str) -> Option<[u8; 16]> {
    let a = Ipv6Addr::from_str(p.strip_suffix("/64")?).ok()?.octets();
    (a[0] & 0xe0 == 0x20 && a[8..].iter().all(|b| *b == 0)).then_some(a)
}

/// The advertisement, ICMPv6 checksum left 0 (the kernel computes it for a raw ICMPv6 socket).
pub fn packet(router_lifetime: u16, prefixes: &[[u8; 16]]) -> Vec<u8> {
    let mut p = vec![ND_ROUTER_ADVERT, 0, 0, 0, 0, 0];
    p.extend_from_slice(&router_lifetime.to_be_bytes());
    p.extend_from_slice(&[0; 8]);
    for a in prefixes {
        // prefix information: type 3, length 4 (x 8 bytes), /64, flags L and A, valid, preferred 0, reserved
        p.extend_from_slice(&[3, 4, 64, 0xc0]);
        p.extend_from_slice(&VALID.to_be_bytes());
        p.extend_from_slice(&[0; 8]);
        p.extend_from_slice(a);
    }
    p
}

/// What is left of the carrier's default router, from `ip -6 route show default dev <bearer>`: 1-9000 s, 1800 when
/// the route shows no expiry.
pub fn router_lifetime(route: &str) -> u16 {
    let v = route
        .split_whitespace()
        .skip_while(|w| *w != "expires")
        .nth(1)
        .and_then(|w| w.trim_end_matches("sec").parse::<u64>().ok())
        .filter(|v| *v >= 1);
    u16::try_from(v.map_or(1800, |v| v.min(9000))).unwrap_or(1800)
}

/// Sends the withdrawal of `prefixes` on `iface`.
pub fn deprecate(platform: &Platform, iface: &str, lifetime: u16, prefixes: &[String]) -> Result<()> {
    let ps: Vec<[u8; 16]> = prefixes.iter().filter_map(|p| global64(p)).collect();
    if ps.is_empty() {
        return Ok(());
    }
    let ifindex: u32 = platform
        .sysfs()
        .read_num(format!("/sys/class/net/{iface}/ifindex"))
        .ok_or_else(|| Error::not_found(format!("interface {iface}")))?;
    sys::send_all_nodes(ifindex, &packet(lifetime, &ps)).map_err(|e| Error::io("ra-deprecate", &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_advertisement() {
        let a = global64("2001:db8:1:2::/64").expect("global");
        let p = packet(1800, &[a]);
        assert_eq!(p.len(), 16 + 32);
        assert_eq!(p[0], 134);
        assert_eq!(&p[6..8], &1800u16.to_be_bytes());
        assert_eq!(&p[16..20], &[3, 4, 64, 0xc0]);
        assert_eq!(&p[20..24], &7200u32.to_be_bytes());
        assert_eq!(&p[24..28], &[0; 4], "preferred 0");
        assert_eq!(&p[32..48], &a);
        assert!(global64("fe80::/64").is_none());
        assert!(global64("2001:db8::1/64").is_none());
        assert!(global64("2001:db8::/48").is_none());
    }

    #[test]
    fn lifetimes() {
        assert_eq!(router_lifetime("default via fe80::1 proto ra metric 1024 expires 1799sec hoplimit 64"), 1799);
        assert_eq!(router_lifetime("default via fe80::1 proto ra metric 1024 expires 99999sec"), 9000);
        assert_eq!(router_lifetime("default via fe80::1 proto ra metric 1024"), 1800);
        assert_eq!(router_lifetime(""), 1800);
    }
}

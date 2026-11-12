//! The context's addresses as the modem reports them, believed only when well formed (FINDINGS 91): what the modem
//! says reaches `ip` and netifd only after these checks.
//!
//! - `+CGCONTRDP: <cid>,<bearer>,"<apn>","a.b.c.d.m.m.m.m","<gw>","<dns1>","<dns2>",...`: the IPv4 address with its
//!   mask in one dotted field (an IPv4V6 context answers a second line for IPv6, with 32 parts).
//! - `+CGPADDR: <cid>,"<v4>","<v6>"`: the IPv6 address as 16 dotted bytes (`+CGPIAF` 0) or hex groups.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use serde::Serialize;

/// The bearer the network gave the context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bearer {
    pub device: String,
    pub ipv4: String,
    pub prefix: u8,
    pub dns: Vec<String>,
    /// The interface identifier (the low 64 bits, `a:b:c:d`) of the IPv6 address assigned to the context; none when
    /// it is IPv4 only. This, not the PDP type asked for, decides whether there is IPv6: a carrier may answer IPV4V6
    /// with IPv4 alone.
    pub iid: Option<String>,
}

impl Bearer {
    pub fn cidr(&self) -> String {
        format!("{}/{}", self.ipv4, self.prefix)
    }
}

/// A dotted IPv4 address, each part 0-255 without a sign or a leading zero, and not 0.0.0.0.
pub fn valid_ipv4(s: &str) -> bool {
    Ipv4Addr::from_str(s).is_ok_and(|a| !a.is_unspecified())
}

/// A dotted netmask to its prefix length; only a contiguous mask.
pub fn mask2prefix(m: &str) -> Option<u8> {
    let a = Ipv4Addr::from_str(m).ok()?;
    let bits = u32::from(a);
    let n = bits.leading_ones();
    (bits.checked_shl(n).unwrap_or(0) == 0).then(|| u8::try_from(n).unwrap_or(32))
}

fn fields(value: &str) -> Vec<String> {
    value.split(',').map(|f| f.trim().trim_matches('"').trim().to_owned()).collect()
}

/// The IPv4 part of one `+CGCONTRDP` value: (address, prefix, DNS servers). `None` unless the address is a real one.
pub fn parse_contrdp(value: &str) -> Option<(String, u8, Vec<String>)> {
    let f = fields(value);
    let parts: Vec<&str> = f.get(3)?.split('.').collect();
    if parts.len() != 8 {
        return None;
    }
    let ip = parts[..4].join(".");
    if !valid_ipv4(&ip) {
        return None;
    }
    let prefix = mask2prefix(&parts[4..].join(".")).unwrap_or(32);
    // a carrier without a second server sends 0.0.0.0; an IPv6 server is never on this line's v4 fields
    let dns = [5, 6].iter().filter_map(|&i| f.get(i)).filter(|d| valid_ipv4(d)).cloned().collect();
    Some((ip, prefix, dns))
}

/// The first well-formed IPv4 line of an `AT+CGCONTRDP` answer.
pub fn first_contrdp<'a>(values: impl Iterator<Item = &'a str>) -> Option<(String, u8, Vec<String>)> {
    values.into_iter().find_map(parse_contrdp)
}

/// The interface identifier of the IPv6 address in a `+CGPADDR` value, `a:b:c:d` in hex; `None` for IPv4 only.
pub fn iid_of_cgpaddr(value: &str) -> Option<String> {
    for f in fields(value).iter().skip(1) {
        let groups: Option<[u16; 8]> = if f.split('.').count() == 16 {
            let b: Vec<u16> = f.split('.').filter_map(|x| x.parse::<u8>().ok()).map(u16::from).collect();
            (b.len() == 16).then(|| std::array::from_fn(|i| b[2 * i] << 8 | b[2 * i + 1]))
        } else if f.contains(':') {
            Ipv6Addr::from_str(f).ok().map(|a| a.segments())
        } else {
            None
        };
        if let Some(g) = groups {
            if g[4..].iter().all(|x| *x == 0) {
                return None;
            }
            return Some(format!("{:x}:{:x}:{:x}:{:x}", g[4], g[5], g[6], g[7]));
        }
    }
    None
}

/// The context's IPv4 address in a `+CGPADDR` value (its first field after the cid, when it has four parts).
pub fn v4_of_cgpaddr(value: &str) -> Option<String> {
    let f = fields(value);
    f.get(1).filter(|a| valid_ipv4(a)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_addresses() {
        assert_eq!(mask2prefix("255.255.255.0"), Some(24));
        assert_eq!(mask2prefix("255.255.255.255"), Some(32));
        assert_eq!(mask2prefix("0.0.0.0"), Some(0));
        assert_eq!(mask2prefix("255.0.255.0"), None);
        assert_eq!(mask2prefix("255.255.255"), None);
        assert!(valid_ipv4("10.1.2.3"));
        assert!(
            !valid_ipv4("0.0.0.0") && !valid_ipv4("10.01.2.3") && !valid_ipv4("256.1.1.1") && !valid_ipv4("-1.2.3.4")
        );
    }

    #[test]
    fn contrdp_believes_only_a_well_formed_answer() {
        let v = r#"1,5,"internet","10.20.30.40.255.255.255.252","10.20.30.41","8.8.8.8","0.0.0.0""#;
        assert_eq!(parse_contrdp(v), Some(("10.20.30.40".into(), 30, vec!["8.8.8.8".into()])));
        assert_eq!(parse_contrdp(r#"1,5,"x","0.0.0.0.0.0.0.0","","","""#), None);
        assert_eq!(parse_contrdp(r#"1,5,"x","10.1.2.3""#), None);
        let v6 = r#"1,5,"x","32.1.13.184.0.0.0.0.0.0.0.0.0.0.0.1.255.255.255.255.255.255.255.255.0.0.0.0.0.0.0.0""#;
        assert_eq!(first_contrdp([v6, v].into_iter()).map(|b| b.1), Some(30));
        assert_eq!(parse_contrdp(r#"1,5,"x","10.1.2.3.255.0.255.0","","1.1.1.1","""#).map(|b| b.1), Some(32));
    }

    #[test]
    fn the_interface_identifier() {
        let dotted = r#"1,"10.1.2.3","32.1.13.184.0.1.0.2.0.10.0.11.0.12.0.13""#;
        assert_eq!(iid_of_cgpaddr(dotted).as_deref(), Some("a:b:c:d"));
        assert_eq!(iid_of_cgpaddr(r#"1,"2001:db8::1:2:3:4""#).as_deref(), Some("1:2:3:4"));
        assert_eq!(iid_of_cgpaddr(r#"1,"10.1.2.3""#), None);
        assert_eq!(iid_of_cgpaddr(r#"1,"10.1.2.3","0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0""#), None);
        assert_eq!(v4_of_cgpaddr(r#"1,"10.1.2.3","x""#).as_deref(), Some("10.1.2.3"));
        assert_eq!(v4_of_cgpaddr(r#"1,"0.0.0.0""#), None);
    }
}

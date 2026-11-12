//! netifd's complete view of the bearer in relay mode, kept current without polling (mu300cell-v6.sh, K36).
//!
//! IPv6 comes from the carrier's RA, so the kernel's address and route announcements are the authoritative source,
//! lifetime refreshes included: each one is reported (with the lifetimes, so netifd never turns an expiring RA
//! address or default into a permanent one). IPv4 and the DNS servers come from `+CGCONTRDP`; the modem announces
//! PDP changes with `+CGEV` (and Unisoc's `+SPPCODATA`, where a DNS change arrives), and each burst of those is one
//! read of the bearer, at least 2 s after the last and 1 s after the burst began (a DEACT and several ACTs make one
//! transition). Only a well-formed answer is believed (FINDINGS 91). A new address is added before the old one goes:
//! no ifdown, no CGACT cycle, so a handover loses packets only during the carrier's own transition. A step that fails
//! keeps the previous state, so the next hint or renew tries again. netifd's renew asks for one read.
//!
//! Every report is address-external, as the setup's was (a report that flips the flag makes netifd delete the
//! address it holds). Runs as part of the protocol session: it ends with the teardown.

use std::time::Duration;

use f50_at::SendOpts;
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, sleep, sleep_until};
use tokio_util::sync::CancellationToken;

use super::sys::{RouteEvents, next_event};
use crate::Data;
use crate::dial::Bearer;
use crate::dial::address::first_contrdp;
use crate::kernel::Kernel;
use crate::netifd::Update;

/// A look at the kernel's IPv6 when no announcement came for this long.
const SAFETY: Duration = Duration::from_secs(300);

/// The RA's state of the bearer as `ip` shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct V6 {
    /// (address, prefix length, preferred, valid); `None` lifetimes are forever.
    pub addrs: Vec<(String, u8, Option<u32>, Option<u32>)>,
    /// (gateway, remaining lifetime)
    pub default: Option<(String, Option<u32>)>,
}

fn lifetime(w: Option<&str>) -> Option<u32> {
    w?.trim_end_matches("sec").parse().ok()
}

/// `ip -6 addr show dev X scope global`: `inet6 A/P scope global ...` then `valid_lft V preferred_lft P`.
pub fn parse_addrs(text: &str) -> Vec<(String, u8, Option<u32>, Option<u32>)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, u8)> = None;
    for l in text.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first() {
            Some(&"inet6") => {
                cur = w.get(1).and_then(|c| c.split_once('/')).and_then(|(a, p)| Some((a.to_owned(), p.parse().ok()?)));
            }
            Some(&"valid_lft") => {
                if let Some((a, p)) = cur.take() {
                    let valid = lifetime(w.get(1).copied());
                    let pref =
                        w.iter().position(|x| *x == "preferred_lft").and_then(|i| lifetime(w.get(i + 1).copied()));
                    out.push((a, p, pref, valid));
                }
            }
            _ => {}
        }
    }
    out.sort();
    out
}

/// `ip -6 route show dev X default proto ra`: the gateway and what is left of it.
pub fn parse_default(text: &str) -> Option<(String, Option<u32>)> {
    let l = text.lines().next()?;
    let w: Vec<&str> = l.split_whitespace().collect();
    let after = |k: &str| w.iter().position(|x| *x == k).and_then(|i| w.get(i + 1).copied());
    Some((after("via").unwrap_or("").to_owned(), lifetime(after("expires"))))
}

async fn snapshot(k: &Kernel<'_>, dev: &str) -> V6 {
    let a = k.show(&["-6", "addr", "show", "dev", dev, "scope", "global"]).await;
    let r = k.show(&["-6", "route", "show", "dev", dev, "default", "proto", "ra"]).await;
    V6 { addrs: parse_addrs(&a), default: parse_default(&r) }
}

/// A line that hints that the PDP context changed.
pub fn pdp_hint(line: &str) -> bool {
    line.starts_with("+SPPCODATA:")
        || (line.starts_with("+CGEV:") && ["PDN", "ACT", "DETACH", "MODIFY"].iter().any(|k| line.contains(k)))
}

async fn report(data: &Data, config: &str, b: &Bearer, v6: &V6) {
    let u = Update {
        ifname: b.device.clone(),
        ipv4: vec![(b.ipv4.clone(), b.prefix)],
        default4: true,
        ipv6: v6.addrs.clone(),
        default6: v6.default.clone(),
        dns: b.dns.clone(),
    };
    if let Err(e) = data.notifier().update(config, &u).await {
        tracing::warn!(error = %e, "relay: the update did not reach netifd");
    }
}

/// One read of the bearer; a changed one (or one the kernel lost) is installed and reported.
async fn refresh(data: &Data, config: &str, b: &mut Bearer, v6: &V6, why: &str) {
    let cmd = format!("AT+CGCONTRDP={}", data.config().cid);
    let opts = SendOpts { queue_wait: Duration::from_secs(1), ..SendOpts::poll(Duration::from_secs(3)) };
    let Some((ip, prefix, dns)) =
        data.at().send(&cmd, opts).await.ok().and_then(|r| first_contrdp(r.values("+CGCONTRDP")))
    else {
        tracing::info!("{why}: the PDP address is not available; keeping the last netifd state");
        return;
    };
    let k = Kernel::new(data.platform());
    let cidr = format!("{ip}/{prefix}");
    let lost = !k.v4_addresses(&b.device).await.contains(&cidr);
    if ip == b.ipv4 && prefix == b.prefix && dns == b.dns && !lost {
        tracing::debug!("{why}: bearer unchanged");
        return;
    }
    if let Err(e) = k.replace_v4(&b.device, Some(&b.cidr()), &ip, prefix).await {
        tracing::warn!(error = %e, "{why}: the new address could not be installed; keeping the previous state for a retry");
        return;
    }
    (b.ipv4, b.prefix, b.dns) = (ip, prefix, dns);
    if let Some(s) = data.state().bearer.as_mut() {
        s.clone_from(b);
    }
    report(data, config, b, v6).await;
    tracing::info!("{why}: bearer updated to {}", b.cidr());
}

/// The monitor, until `cancel` (the teardown).
pub(crate) async fn run(data: &Data, config: &str, mut b: Bearer, renew: &Notify, cancel: &CancellationToken) {
    let k = Kernel::new(data.platform());
    let events = match RouteEvents::open() {
        Ok(e) => Some(e),
        Err(e) => {
            tracing::warn!(error = %e, "relay: no route announcements; a look every {} s", SAFETY.as_secs());
            None
        }
    };
    let mut urcs = data.at().urcs();
    let mut last = snapshot(&k, &b.device).await;
    report(data, config, &b, &last).await;
    let mut last_pdp = Instant::now() - Duration::from_secs(2);
    let mut pdp_at: Option<Instant> = None;
    loop {
        let look = tokio::select! {
            () = cancel.cancelled() => return,
            () = renew.notified() => {
                refresh(data, config, &mut b, &last, "manual-renew").await;
                last_pdp = Instant::now();
                false
            }
            r = next_event(events.as_ref()) => {
                if r.is_err() {
                    sleep(Duration::from_secs(1)).await;
                }
                // the rest of the burst
                sleep(Duration::from_millis(500)).await;
                true
            }
            u = urcs.recv() => {
                match u {
                    Ok(u) if pdp_hint(&u.line) && pdp_at.is_none() => {
                        pdp_at = Some((Instant::now() + Duration::from_secs(1)).max(last_pdp + Duration::from_secs(3)));
                    }
                    Err(RecvError::Closed) => sleep(Duration::from_secs(5)).await,
                    _ => {}
                }
                false
            }
            () = sleep_until(pdp_at.unwrap_or_else(|| Instant::now() + SAFETY)), if pdp_at.is_some() => {
                pdp_at = None;
                refresh(data, config, &mut b, &last, "cgev").await;
                last_pdp = Instant::now();
                false
            }
            () = sleep(SAFETY) => true,
        };
        if look {
            let now = snapshot(&k, &b.device).await;
            if now != last {
                last = now;
                report(data, config, &b, &last).await;
                tracing::debug!("relay: IPv6 RA address/route state refreshed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernels_ipv6_view() {
        let a = "\
5: sipa_eth0: <POINTOPOINT,NOARP,UP,LOWER_UP> mtu 1500
    inet6 2001:db8:1:2:a:b:c:d/64 scope global dynamic mngtmpaddr
       valid_lft 7190sec preferred_lft 3590sec
    inet6 2001:db8:9::1/64 scope global
       valid_lft forever preferred_lft forever
";
        assert_eq!(
            parse_addrs(a),
            vec![
                ("2001:db8:1:2:a:b:c:d".to_owned(), 64, Some(3590), Some(7190)),
                ("2001:db8:9::1".to_owned(), 64, None, None),
            ]
        );
        let r = "default via fe80::1 proto ra metric 1024 expires 1799sec hoplimit 255 pref medium";
        assert_eq!(parse_default(r), Some(("fe80::1".to_owned(), Some(1799))));
        assert_eq!(parse_default(""), None);
        assert!(pdp_hint("+CGEV: NW PDN DEACT 1") && pdp_hint("+CGEV: ME DETACH") && pdp_hint("+SPPCODATA: 1,x"));
        assert!(!pdp_hint("+CEREG: 1") && !pdp_hint("+CGEV: REJECT"));
    }
}

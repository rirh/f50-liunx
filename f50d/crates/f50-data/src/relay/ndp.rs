//! The bearer's public /64 routed to the LAN, in relay mode (ndp-learn, K37).
//!
//! The carrier hands out one /64 by RA on the bearer; odhcpd relays it to the LAN, whose clients SLAAC their own
//! addresses in it. Replies to those addresses must leave on the LAN bridge, but no on-link route for the prefix
//! exists there: a lookup falls through to the default route and the reply goes back out the modem (measured: every
//! connection to a client address timed out, and Android clients stopped asking for AAAA). So each /64 the bearer
//! holds gets a route on the bridge, at metric 128 - below the RA's own route for the same /64 on the bearer (256),
//! so replies go to the LAN. Nothing is learnt from the LAN: pinning /128s from the neighbour table let any client
//! hijack another's address with one spoofed advertisement; the /64 does the work.
//!
//! A /64 that left the bearer loses its route, and once a *different* /64 is on the bearer it is withdrawn from the
//! clients ([`super::ra`], three rounds: an RA is a multicast a client can miss). Not as soon as it leaves: a WAN
//! restart flushes the bearer's address and the carrier usually gives the same /64 back. Host routes inside the
//! prefixes on the bearer (odhcpd's NDP relay, if somebody turned it on) are removed: they would override the /64.
//! While the bearer has no global address a router solicitation is sent, after 10 s, then twice as long each time
//! up to 300 s.
//!
//! The kernel's addresses come from /proc/net/if_inet6 (fixed-width hex, read without a program) every 10 s; the
//! routes are put back every third round, so one somebody removed returns within 30 s. Runs for the daemon's life
//! when `network.wan.ipv6` is `relay` at its start, so the prefix memory outlives a reconnect.

use std::net::Ipv6Addr;
use std::str::FromStr;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::ra;
use crate::Data;
use crate::kernel::Kernel;
use crate::settings::WanSettings;
use crate::state::Phase;

const LAN_METRIC: u32 = 128;
/// The metric an earlier ndp-learn used, which lost to the RA's route; removed where found.
const OLD_METRIC: u32 = 1024;
const INTERVAL: Duration = Duration::from_secs(10);
const ROUTE_EVERY: u32 = 3;
const WITHDRAW_ROUNDS: u32 = 3;
const HEAL_MAX: Duration = Duration::from_secs(300);

/// The bearer's global /64s in an if_inet6 text, sorted, each once; and whether it has a global address at all.
pub fn bearer_prefixes(if_inet6: &str, dev: &str) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut global = false;
    for l in if_inet6.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let [addr, _, _, scope, _, name] = f.as_slice() else { continue };
        if *name != dev || *scope != "00" {
            continue;
        }
        global = true;
        if addr.len() != 32 || !addr.starts_with(['2', '3']) || !addr.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let groups: Vec<String> = (0..4)
            .map(|i| {
                let h = addr[i * 4..i * 4 + 4].trim_start_matches('0');
                if h.is_empty() { "0".to_owned() } else { h.to_ascii_lowercase() }
            })
            .collect();
        out.push(format!("{}::/64", groups.join(":")));
    }
    out.sort();
    out.dedup();
    (out, global)
}

/// `addr` is a plain IPv6 address inside the /64 `prefix`, and not its subnet-router anycast.
pub fn in_prefix(addr: &str, prefix: &str) -> bool {
    let (Ok(a), Some(p)) = (Ipv6Addr::from_str(addr), prefix.strip_suffix("::/64")) else { return false };
    let Ok(p) = Ipv6Addr::from_str(&format!("{p}::")) else { return false };
    a.segments()[..4] == p.segments()[..4] && a.segments()[4..].iter().any(|s| *s != 0)
}

#[derive(Debug, Default)]
struct Ndp {
    last: Vec<String>,
    gone: Vec<String>,
    withdraw: Vec<String>,
    withdraw_rounds: u32,
}

impl Ndp {
    async fn learn(&mut self, data: &Data, dev: &str, prefixes: &[String]) {
        let k = Kernel::new(data.platform());
        let lan = data.config().lan.as_str();
        for p in self.last.iter().filter(|p| !prefixes.contains(p)) {
            for m in [LAN_METRIC, OLD_METRIC] {
                k.route6_quiet(&["-6", "route", "del", p, "dev", lan, "metric", &m.to_string()]).await;
            }
        }
        self.withdraw_old(data, dev, prefixes).await;
        for p in prefixes {
            if !self.last.contains(p) {
                tracing::info!("LAN prefix route: {p} dev {lan} metric {LAN_METRIC}");
            }
            if let Err(e) = k.route6("replace", p, lan, LAN_METRIC).await {
                tracing::warn!(error = %e, "LAN prefix route {p}");
            }
            k.route6_quiet(&["-6", "route", "del", p, "dev", lan, "metric", &OLD_METRIC.to_string()]).await;
        }
        self.last = prefixes.to_vec();
        if prefixes.is_empty() {
            return;
        }
        let routes = k.show(&["-6", "route", "show", "dev", dev]).await;
        for r in routes.lines().filter_map(|l| l.split_whitespace().next()).filter(|r| !r.contains('/')) {
            if prefixes.iter().any(|p| in_prefix(r, p)) {
                k.route6_quiet(&["-6", "route", "del", &format!("{r}/128"), "dev", dev]).await;
            }
        }
    }

    async fn withdraw_old(&mut self, data: &Data, dev: &str, prefixes: &[String]) {
        for p in &self.last {
            if !prefixes.contains(p) && !self.gone.contains(p) {
                self.gone.push(p.clone());
            }
        }
        // one that came back (a WAN restart that got the same /64) is neither waiting nor withdrawn any more
        self.gone.retain(|p| !prefixes.contains(p));
        self.withdraw.retain(|p| !prefixes.contains(p));
        if prefixes.is_empty() {
            return;
        }
        if !self.gone.is_empty() {
            for p in std::mem::take(&mut self.gone) {
                if !self.withdraw.contains(&p) {
                    self.withdraw.push(p);
                }
            }
            self.withdraw_rounds = WITHDRAW_ROUNDS;
            tracing::info!("withdrawing from the LAN: {}", self.withdraw.join(" "));
        }
        if self.withdraw.is_empty() || self.withdraw_rounds == 0 {
            return;
        }
        self.withdraw_rounds -= 1;
        let k = Kernel::new(data.platform());
        let lifetime = ra::router_lifetime(&k.show(&["-6", "route", "show", "default", "dev", dev]).await);
        if let Err(e) = ra::deprecate(data.platform(), &data.config().lan, lifetime, &self.withdraw) {
            tracing::warn!(error = %e, "could not withdraw {}", self.withdraw.join(" "));
        }
    }
}

/// The task: nothing at all unless relay mode is configured.
pub(crate) async fn run(data: Data, shutdown: CancellationToken) {
    let interface = data.config().interface.clone();
    let relay = tokio::select! {
        () = shutdown.cancelled() => return,
        s = WanSettings::load(data.platform(), &interface, data.modem().active_slot()) => s.is_ok_and(|s| s.relay),
    };
    if !relay {
        return;
    }
    let mut ndp = Ndp::default();
    let (mut seen, mut n) = (None::<String>, 0u32);
    let (mut heal, mut heal_at) = (INTERVAL, Instant::now() + INTERVAL);
    loop {
        let dev = data.device();
        let text = data.platform().sysfs().path("/proc/net/if_inet6");
        let text = std::fs::read_to_string(text).unwrap_or_default();
        let (prefixes, global) = bearer_prefixes(&text, &dev);
        n = n.wrapping_add(1);
        let changed = seen.as_deref() != Some(text.as_str());
        if changed || ndp.withdraw_rounds > 0 || n % ROUTE_EVERY == 0 {
            if changed {
                (heal, heal_at) = (INTERVAL, Instant::now() + INTERVAL);
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = ndp.learn(&data, &dev, &prefixes) => {}
            }
            seen = Some(text);
        }
        // the carrier's RA can be missed while the bearer settles, and nothing else solicits again
        let connected = data.state().phase == Phase::Connected;
        if global || !connected {
            (heal, heal_at) = (INTERVAL, Instant::now() + INTERVAL);
        } else if Instant::now() >= heal_at {
            Kernel::new(data.platform()).solicit(&dev).await;
            heal_at = Instant::now() + heal;
            heal = (heal * 2).min(HEAL_MAX);
        }
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IF_INET6: &str = "\
20010db8000100020000000000000005 05 40 00 00 sipa_eth0
20010db8000100020000000000000006 05 40 00 00 sipa_eth0
2a0012340000abcd0000000000000001 05 40 00 00 sipa_eth0
fe800000000000000000000000000001 05 40 20 80 sipa_eth0
fd000000000000000000000000000001 05 40 00 00 sipa_eth0
20010db8000900020000000000000001 03 40 00 00 br-lan
";

    #[test]
    fn every_bearer_prefix_and_nothing_from_the_lan() {
        let (p, global) = bearer_prefixes(IF_INET6, "sipa_eth0");
        assert_eq!(p, vec!["2001:db8:1:2::/64".to_owned(), "2a00:1234:0:abcd::/64".to_owned()]);
        assert!(global);
        assert_eq!(bearer_prefixes(IF_INET6, "sipa_eth8"), (vec![], false));
    }

    #[test]
    fn host_routes_inside_the_prefix_only() {
        assert!(in_prefix("2001:db8:1:2::5", "2001:db8:1:2::/64"));
        assert!(!in_prefix("2001:db8:1:2::", "2001:db8:1:2::/64"), "the subnet-router anycast");
        assert!(!in_prefix("2001:db8:1:3::5", "2001:db8:1:2::/64"));
        assert!(!in_prefix("-rf", "2001:db8:1:2::/64"));
        assert!(in_prefix("2a00:1234:0:abcd::1", "2a00:1234:0:abcd::/64"));
    }
}

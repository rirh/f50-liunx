//! The bearer's kernel state, which the protocol owns (netifd only records it, see [`crate::netifd`]): the device
//! up, its IPv4 address and default route, IPv6 on or off, its counters; and fw4's flowtable on the vendor 5.4
//! kernel. Addresses and routes go through `ip` (iproute2 is the platform here), the per-device switches through
//! /proc/sys.

use std::time::Duration;

use f50_core::Result;
use f50_core::exec::Cmd;
use f50_platform::Platform;

const IP: &str = "/sbin/ip";
const STEP: Duration = Duration::from_secs(10);

pub(crate) struct Kernel<'a> {
    platform: &'a Platform,
}

impl<'a> Kernel<'a> {
    pub fn new(platform: &'a Platform) -> Self {
        Kernel { platform }
    }

    /// `ip ARGS`; a failure says what `ip` said.
    async fn ip(&self, args: &[&str]) -> Result<String> {
        Cmd::new(IP).args(args).timeout(STEP).ok().await
    }

    /// `ip ARGS` where a failure changes nothing (a delete of something already gone).
    async fn ip_quiet(&self, args: &[&str]) {
        if let Err(e) = self.ip(args).await {
            tracing::debug!(error = %e, "ip {}", args.join(" "));
        }
    }

    fn conf6(&self, dev: &str, key: &str, value: &str) {
        let p = format!("/proc/sys/net/ipv6/conf/{dev}/{key}");
        if let Err(e) = self.platform.sysfs().write(&p, value) {
            tracing::debug!(error = %e, "{p}");
        }
    }

    pub async fn link_up(&self, dev: &str) -> Result<()> {
        self.ip(&["link", "set", "dev", dev, "up"]).await.map(drop)
    }

    /// The last dial's IPv6 state goes: this carrier's RAs give infinite lifetimes, so after a redial on a new prefix
    /// the old SLAAC address would never expire and prefixes would stack up.
    pub async fn flush_v6(&self, dev: &str) {
        self.ip_quiet(&["-6", "addr", "flush", "dev", dev, "scope", "global"]).await;
        self.ip_quiet(&["-6", "route", "flush", "dev", dev]).await;
    }

    /// Relay mode takes the carrier's RA. netifd treats the protocol as IPv4, so with forwarding on accept_ra stays 0;
    /// the disable_ipv6 1->0 cycle restarts addrconf (the link-local on-link route is rebuilt and a fresh router
    /// solicitation goes out), and accept_ra 2 because this is a router.
    pub fn take_ra(&self, dev: &str) {
        self.conf6(dev, "disable_ipv6", "1");
        self.conf6(dev, "disable_ipv6", "0");
        self.conf6(dev, "accept_ra", "0");
        self.conf6(dev, "accept_ra", "2");
    }

    /// accept_ra off and on: a router solicitation, for a bearer that missed the carrier's RA.
    pub async fn solicit(&self, dev: &str) {
        self.conf6(dev, "accept_ra", "0");
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.conf6(dev, "accept_ra", "2");
    }

    pub fn ipv6(&self, dev: &str, on: bool) {
        self.conf6(dev, "disable_ipv6", if on { "0" } else { "1" });
    }

    /// The global IPv4 addresses of the device, `a.b.c.d/p`.
    pub async fn v4_addresses(&self, dev: &str) -> Vec<String> {
        let out = self.ip(&["-4", "-o", "addr", "show", "dev", dev, "scope", "global"]).await.unwrap_or_default();
        out.lines()
            .filter_map(|l| {
                let mut w = l.split_whitespace();
                w.position(|x| x == "inet")?;
                w.next().map(str::to_owned)
            })
            .collect()
    }

    /// The bearer's address and default route, installed by us: replace first (an unchanged address stays
    /// continuous across a re-setup), everything else the carrier left removed, then once more - a leftover primary
    /// in the new subnet made the new one its secondary, and deleting the primary took the secondary along (no
    /// promote_secondaries).
    pub async fn install_v4(&self, dev: &str, ip: &str, prefix: u8) -> Result<()> {
        let cidr = format!("{ip}/{prefix}");
        if self.ip(&["-4", "addr", "replace", &cidr, "dev", dev]).await.is_err() {
            self.ip(&["-4", "addr", "add", &cidr, "dev", dev]).await?;
        }
        for a in self.v4_addresses(dev).await {
            if a != cidr {
                self.ip_quiet(&["-4", "addr", "del", &a, "dev", dev]).await;
            }
        }
        self.ip_quiet(&["-4", "addr", "replace", &cidr, "dev", dev]).await;
        self.ip(&["-4", "route", "replace", "default", "dev", dev]).await.map(drop)
    }

    /// A new address before the old one goes (the relay monitor's handover: no ifdown, no CGACT cycle).
    pub async fn replace_v4(&self, dev: &str, old: Option<&str>, ip: &str, prefix: u8) -> Result<()> {
        let cidr = format!("{ip}/{prefix}");
        if self.ip(&["-4", "addr", "add", &cidr, "dev", dev]).await.is_err() {
            self.ip(&["-4", "addr", "replace", &cidr, "dev", dev]).await?;
        }
        self.ip(&["-4", "route", "replace", "default", "dev", dev]).await?;
        if let Some(old) = old.filter(|o| *o != cidr) {
            self.ip_quiet(&["-4", "addr", "del", old, "dev", dev]).await;
            self.ip_quiet(&["-4", "addr", "replace", &cidr, "dev", dev]).await;
            self.ip_quiet(&["-4", "route", "replace", "default", "dev", dev]).await;
        }
        Ok(())
    }

    /// The link-local address from the interface identifier the network assigned, as 3GPP has the UE build it (the
    /// kernel's EUI-64 one would come from sipa_eth's made-up MAC).
    pub async fn link_local(&self, dev: &str, iid: &str) {
        self.ip_quiet(&["-6", "addr", "add", &format!("fe80::{iid}/64"), "dev", dev]).await;
    }

    /// The teardown: netifd does not remove external state on ifdown, so both families go here.
    pub async fn flush_all(&self, dev: &str) {
        self.ip_quiet(&["-4", "addr", "flush", "dev", dev, "scope", "global"]).await;
        self.ip_quiet(&["-4", "route", "del", "default", "dev", dev]).await;
        self.flush_v6(dev).await;
    }

    /// `ip ARGS` output for the relay monitor's IPv6 view.
    pub async fn show(&self, args: &[&str]) -> String {
        self.ip(args).await.unwrap_or_default()
    }

    pub async fn route6(&self, verb: &str, prefix: &str, dev: &str, metric: u32) -> Result<()> {
        self.ip(&["-6", "route", verb, prefix, "dev", dev, "metric", &metric.to_string()]).await.map(drop)
    }

    pub async fn route6_quiet(&self, args: &[&str]) {
        self.ip_quiet(args).await;
    }

    /// A counter of the device (`rx_packets`, `tx_bytes`, ...).
    pub fn counter(&self, dev: &str, name: &str) -> Option<u64> {
        self.platform.sysfs().read_num(format!("/sys/class/net/{dev}/statistics/{name}"))
    }

    /// The kernel was built with the modem stack (the vendor modem loader or the SIPC drivers).
    pub fn modem_stack(&self) -> bool {
        let s = self.platform.sysfs();
        s.path("/dev/modem").exists() || s.path("/dev/pmsys").exists() || s.path("/dev/stty_nr1").exists()
    }

    pub fn kernel_release(&self) -> String {
        self.platform.sysfs().read("/proc/sys/kernel/osrelease").unwrap_or_default()
    }

    /// fw4's firewall zone of `interface`, if it is in one (`fw4 -q network`).
    pub async fn fw4_zone(&self, interface: &str) -> Option<String> {
        let out = Cmd::new("/sbin/fw4").args(["-q", "network", interface]).timeout(STEP).ok().await.ok()?;
        let z = out.trim();
        (!z.is_empty()).then(|| z.to_owned())
    }

    /// The vendor 5.4 kernel and fw4's flowtable (`19-f50-fw4`, FINDINGS 38): fw4 replaces its ruleset in one
    /// transaction, and 5.4 refuses a flowtable naming a device that the flowtable deleted in the same transaction
    /// still has (the pending deletion is not skipped before 5.13). The reload netifd's ifup hotplug runs then fails
    /// with "Resource busy" and the firewall stays the one rendered before the bearer existed: no masquerade, no LAN
    /// input rules. Deleting `ft` in a transaction of its own first lets the reload re-create it. Only on 5.4, only
    /// when the reload will run (the firewall enabled, the interface in a zone). The image's fw4 also adds sipa_eth0
    /// to the flowtable's devices itself (`fw4-sipa-offload.patch`): netifd only knows it as the l3 device.
    pub async fn fw4_before_ifup(&self, interface: &str) {
        if !self.kernel_release().starts_with("5.4.") || !self.platform.services().enabled("firewall") {
            return;
        }
        if self.fw4_zone(interface).await.is_none() {
            return;
        }
        let nft = |args: &'static [&'static str]| Cmd::new("/usr/sbin/nft").args(args).timeout(STEP).ok();
        let Ok(list) = nft(&["list", "flowtables", "inet", "fw4"]).await else { return };
        if !list.contains("flowtable ft ") {
            return;
        }
        match nft(&["delete", "flowtable", "inet", "fw4", "ft"]).await {
            Ok(_) => tracing::info!("fw4 flowtable deleted before the reload (5.4 kernel, ifup {interface})"),
            Err(e) => tracing::warn!(error = %e, "fw4 flowtable"),
        }
    }
}

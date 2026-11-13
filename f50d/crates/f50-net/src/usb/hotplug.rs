//! netifd's and the kernel's hotplug events for the USB port. Ported from `hotplug.d/iface/10-f50-usb` and
//! `90-unisoc-usb-host` (iface and net).
//!
//! On the LAN's ifup (K9, from the kanoqwq fork, cd4fb28 and 4e70d02): dnsmasq serves the LAN now, so the early DHCP
//! server of preinit goes; the address boot/init and preinit put on the gadget's netdev is taken off it once it is a
//! bridge port (netifd can leave it there, which makes two equal routes for the LAN subnet: replies leave through the
//! wrong device and the host cannot even ping br-lan). rndis0 joins the bridge first: an install whose bridge section
//! predates a switch to RNDIS does not have it (issue #45: with it left out, neither the Wi-Fi clients nor the USB
//! host could use the LAN). Then the gadget is re-enumerated (K10: macOS keeps the link inactive after netifd
//! reconfigures usb0), at most every 20 s, and not when adb is about to join the gadget: adbd's own rebind does the
//! same, and two at once broke a Mac's NCM driver (FINDINGS 237).

use std::time::{Duration, Instant};

use crate::{Net, lock, tools};

use super::{gadget, host, role};

/// See [`Net::hotplug_iface`].
pub(crate) async fn iface(net: &Net, action: &str, interface: &str) {
    if action != "ifup" || interface != net.cfg().lan_interface {
        return;
    }
    handoff(net).await;
    rebind_after_lan(net);
    host::sync_all(net).await;
}

/// See [`Net::hotplug_net`].
pub(crate) async fn net(net: &Net, action: &str, iface: &str) {
    if action == "add" {
        host::sync_one(net, iface).await;
    }
}

/// The LAN handoff: rndis0 into the bridge, the early DHCP server gone, the bridge's address off its gadget ports.
pub(crate) async fn handoff(net: &Net) {
    let br = net.cfg().lan_bridge.clone();
    if net.path("/sys/class/net/rndis0").exists() {
        tools::ip_quiet(&["link", "set", "rndis0", "master", &br]).await;
        tools::ip_quiet(&["link", "set", "rndis0", "up"]).await;
    }
    let pidfile = net.path("/run/f50-early-udhcpd.pid");
    if let Some(pid) =
        f50_core::fsx::read_line(&pidfile).filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        let _ = f50_core::exec::Cmd::new("kill").arg(&pid).timeout(Duration::from_secs(5)).output().await;
    }
    let _ = std::fs::remove_file(pidfile);
    let Some((bridge_ip, _)) = tools::ipv4_of(&br).await else { return };
    for nic in gadget::NETDEVS {
        if !is_port(net, nic) {
            continue;
        }
        // the bridge's address with whatever prefix the port has (preinit's mask can differ from netifd's)
        for (a, p) in tools::ipv4_all(nic).await {
            if a == bridge_ip {
                tools::ip_quiet(&["addr", "del", &format!("{a}/{p}"), "dev", nic]).await;
            }
        }
    }
}

fn is_port(net: &Net, nic: &str) -> bool {
    std::fs::read_link(net.path(format!("/sys/class/net/{nic}/master")))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .is_some_and(|m| m == net.cfg().lan_bridge)
}

/// adb is about to join the gadget: on, adbd there, not the host role, not linked yet.
fn adb_due(net: &Net) -> bool {
    !crate::adbd::linked(net)
        && net.path(&net.cfg().adbd).exists()
        && crate::adbd::wanted(net)
        && role::now(net).as_deref() != Some("host")
}

fn rebind_after_lan(net: &Net) {
    {
        let mut last = lock(&net.inner.last_rebind);
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(20)) {
            return;
        }
        *last = Some(Instant::now());
    }
    if !net.path(&net.cfg().gadget).join("UDC").exists() {
        return;
    }
    let Some(udc) = gadget::udc_name(net) else { return };
    // the gadget netdevs that are bridge ports now are made ports again after the rebind (a user who took usb0 out
    // of br-lan keeps it that way)
    let ports: Vec<&'static str> = gadget::NETDEVS.iter().copied().filter(|n| is_port(net, n)).collect();
    let due = adb_due(net);
    let n = net.clone();
    net.spawn("usb-lan-rebind", async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        for _ in 0..12 {
            if !due || crate::adbd::linked(&n) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let guard = gadget::lock(&n).await;
        if due && crate::adbd::linked(&n) {
            drop(guard);
            tools::logger("f50", "usb gadget re-enumerated by adbd (adb joined it)").await;
            return;
        }
        gadget::unbind(&n);
        tokio::time::sleep(Duration::from_secs(1)).await;
        if gadget::bind(&n, &udc, 1).await {
            tools::logger("f50", &format!("usb gadget re-enumerated on {udc}")).await;
        }
        drop(guard);
        tokio::time::sleep(Duration::from_secs(1)).await;
        let br = n.cfg().lan_bridge.clone();
        for nic in ports {
            if n.path(format!("/sys/class/net/{nic}")).exists() {
                tools::ip_quiet(&["link", "set", nic, "master", &br]).await;
                tools::ip_quiet(&["link", "set", nic, "up"]).await;
            }
        }
    });
}

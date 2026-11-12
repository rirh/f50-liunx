//! The configfs gadget boot/init made: what it has bound, unbinding and binding it, and the re-enumerations that make
//! a computer ask for its DHCP lease again.
//!
//! One rebind at a time (FINDINGS 237): whoever unbinds holds the daemon's rebind lock until the gadget is bound
//! again. Two rebinds within the same second (the LAN hook's and adbd's) left a Mac's NCM driver with a dead OUT path
//! on 2 boots in 13. A lock held too long is waited for 15 s at most: the gadget must never stay unbound for a lock.

use std::path::PathBuf;
use std::time::Duration;

use f50_core::fsx;
use serde_json::{Value, json};

use crate::{Net, tools};

/// The gadget's network netdevs.
pub(crate) const NETDEVS: &[&str] = &["usb0", "rndis0"];

/// The network function the gadget has bound now: `ncm`, `ecm`, `rndis`, `multi` (RNDIS and NCM, as boot/init makes
/// them for "multi"), `mixed` (any other mix), or `none` (no gadget bound, or none of these).
pub(crate) fn bound_function(net: &Net) -> String {
    let mut found: Option<&'static str> = None;
    let Ok(gadgets) = std::fs::read_dir(net.path(&net.cfg().gadgets)) else { return "none".into() };
    for g in gadgets.flatten() {
        if fsx::read_line(&g.path().join("UDC")).unwrap_or_default().is_empty() {
            continue;
        }
        let Ok(configs) = std::fs::read_dir(g.path().join("configs")) else { continue };
        for c in configs.flatten() {
            let Ok(links) = std::fs::read_dir(c.path()) else { continue };
            for l in links.flatten() {
                let Ok(target) = std::fs::read_link(l.path()) else { continue };
                let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let f = match name.split('.').next() {
                    Some("ncm") => "ncm",
                    Some("ecm") => "ecm",
                    Some("rndis") => "rndis",
                    _ => continue,
                };
                found = Some(combine(found, f));
            }
        }
    }
    found.unwrap_or("none").into()
}

pub(crate) fn combine(found: Option<&'static str>, f: &'static str) -> &'static str {
    match (found, f) {
        (None, f) => f,
        (Some("ncm"), "rndis") | (Some("rndis"), "ncm") | (Some("multi"), "ncm" | "rndis") => "multi",
        (Some(a), b) if a == b => a,
        _ => "mixed",
    }
}

fn udc_file(net: &Net) -> PathBuf {
    net.path(&net.cfg().gadget).join("UDC")
}

/// The controller: the one bound now, or the first there is.
pub(crate) fn udc_name(net: &Net) -> Option<String> {
    fsx::read_line(&udc_file(net)).filter(|u| !u.is_empty()).or_else(|| {
        let mut names: Vec<String> = std::fs::read_dir(net.path(&net.cfg().udc_class))
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names.into_iter().next()
    })
}

pub(crate) fn bound(net: &Net) -> bool {
    fsx::read_line(&udc_file(net)).is_some_and(|u| !u.is_empty())
}

pub(crate) fn unbind(net: &Net) {
    if bound(net) {
        let _ = std::fs::write(udc_file(net), "\n");
    }
}

/// Binds to `udc`; up to `tries` attempts 0.2 s apart (adbd rewrites its descriptors when no bind came within a
/// second, and a bind in that gap fails).
pub(crate) async fn bind(net: &Net, udc: &str, tries: u32) -> bool {
    for _ in 0..tries.max(1) {
        if std::fs::write(udc_file(net), udc).is_ok() && fsx::read_line(&udc_file(net)).as_deref() == Some(udc) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

/// The rebind lock, waited for 15 s at most (`None`: taken over, it is held too long).
pub(crate) async fn lock(net: &Net) -> Option<tokio::sync::MutexGuard<'_, ()>> {
    tokio::time::timeout(Duration::from_secs(15), net.inner.rebind.lock()).await.ok()
}

/// The gadget netdevs back into the LAN after a rebind (they should survive it: this is a guard), and the LAN
/// handoff once more.
pub(crate) async fn lan_back(net: &Net) {
    let br = net.cfg().lan_bridge.clone();
    for nic in NETDEVS {
        if net.path(format!("/sys/class/net/{nic}")).exists() {
            tools::ip_quiet(&["link", "set", nic, "master", &br]).await;
            tools::ip_quiet(&["link", "set", nic, "up"]).await;
        }
    }
    super::hotplug::handoff(net).await;
}

/// The computer of the gadget: the host MACs of its network functions (the kernel ends them with a NUL).
fn host_macs(net: &Net) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(net.path(&net.cfg().gadget).join("functions")) else { return Vec::new() };
    rd.flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("ncm.") || n.starts_with("ecm.") || n.starts_with("rndis.")
        })
        .filter_map(|e| std::fs::read(e.path().join("host_addr")).ok())
        .map(|b| String::from_utf8_lossy(&b).chars().filter(|c| !c.is_whitespace() && *c != '\0').collect::<String>())
        .map(|m| m.to_ascii_lowercase())
        .filter(|m| !m.is_empty())
        .collect()
}

/// Whether the USB host has a lease: a lease of one of the gadget's host MACs (with "multi" a computer uses one of
/// two, macOS only NCM's). A Wi-Fi client's lease is not one (FINDINGS 237); a gadget whose host MAC cannot be read
/// falls back to any lease.
pub(crate) fn host_has_lease(leases: &str, macs: &[String]) -> bool {
    if leases.trim().is_empty() {
        return false;
    }
    if macs.is_empty() {
        return true;
    }
    let low = leases.to_ascii_lowercase();
    macs.iter().any(|m| low.contains(&format!(" {m} ")))
}

/// See [`Net::usb_rebind_if_no_lease`]: the host starts its DHCP the moment the gadget appears and gives up after a
/// few seconds; a device still booting answers too late, the host falls back to 169.254 and (macOS) stops asking.
/// Re-enumerating looks like a fresh cable to it.
pub(crate) fn spawn_lease_check(net: &Net, delay: Duration) {
    let n = net.clone();
    net.spawn("usb-lease-check", async move {
        tokio::time::sleep(delay).await;
        if super::role::now(&n).as_deref() == Some("host") {
            return;
        }
        let leases = tokio::fs::read_to_string(n.path(&n.cfg().leases)).await.unwrap_or_default();
        if host_has_lease(&leases, &host_macs(&n)) {
            return;
        }
        let Some(udc) = udc_name(&n) else { return };
        let guard = lock(&n).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        unbind(&n);
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut ok = false;
        for _ in 0..10 {
            if bind(&n, &udc, 1).await {
                ok = true;
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        drop(guard);
        tokio::time::sleep(Duration::from_secs(1)).await;
        for nic in NETDEVS {
            tools::ip_quiet(&["link", "set", nic, "up"]).await;
        }
        tools::logger("mu300-usb", &format!("the USB host had no lease: gadget re-enumerated on {udc} ({ok})")).await;
    });
}

/// The link to a computer: the controller's state and speed.
pub(crate) fn link(net: &Net) -> Value {
    let Some(udc) = udc_name(net) else { return Value::Null };
    let d = net.path(&net.cfg().udc_class).join(&udc);
    json!({
        "udc": udc,
        "bound": bound(net),
        "state": fsx::read_line(&d.join("state")),
        "speed": fsx::read_line(&d.join("current_speed")),
        "max_speed": fsx::read_line(&d.join("maximum_speed")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_combine() {
        assert_eq!(combine(None, "ncm"), "ncm");
        assert_eq!(combine(Some("ncm"), "rndis"), "multi");
        assert_eq!(combine(Some("multi"), "ncm"), "multi");
        assert_eq!(combine(Some("ecm"), "ecm"), "ecm");
        assert_eq!(combine(Some("ecm"), "ncm"), "mixed");
    }

    #[test]
    fn leases_of_the_host() {
        let l = "1700000000 aa:bb:cc:dd:ee:ff 192.168.77.200 mac *\n";
        assert!(host_has_lease(l, &["aa:bb:cc:dd:ee:ff".into()]));
        assert!(!host_has_lease(l, &["11:22:33:44:55:66".into()]));
        assert!(host_has_lease(l, &[]));
        assert!(!host_has_lease("", &[]));
    }
}

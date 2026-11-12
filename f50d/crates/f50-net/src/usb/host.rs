//! In the host role: USB network adapters (a USB Ethernet dongle behind a powered hub) as ports of the LAN bridge.
//!
//! Only physical USB NICs explicitly saved as LAN ports are brought into br-lan by themselves (at boot, on the LAN's
//! ifup and when the NIC appears); a new one is added by the user (`f50.usb adapter_add`), and only when nothing else
//! owns it (kanoqwq's db4e5d2). Ported from `device-usb net-list|net-add|net-sync|host-rescan`.

use std::path::Path;
use std::time::Duration;

use f50_core::{Error, Result, fsx};
use f50_platform::uci::{Change, Section};
use serde_json::{Value, json};

use crate::{Net, rules, tools};

/// A host-side USB Ethernet NIC: Ethernet (ARPHRD_ETHER), not wireless (a USB Wi-Fi stick is an Ethernet-type device
/// too), not the gadget's own netdev (its parent is on the gadget bus, whatever its name), and on the USB bus.
pub(crate) fn usb_net(net: &Net, name: &str) -> bool {
    if !rules::ifname_ok(name) {
        return false;
    }
    let d = net.path(format!("/sys/class/net/{name}"));
    if !d.is_dir() || fsx::read_line(&d.join("type")).as_deref() != Some("1") {
        return false;
    }
    if d.join("wireless").exists() || d.join("phy80211").exists() {
        return false;
    }
    let sub = std::fs::canonicalize(d.join("device/subsystem")).unwrap_or_default();
    if sub.ends_with("bus/gadget") {
        return false;
    }
    if sub.ends_with("bus/usb") {
        return true;
    }
    let path = std::fs::canonicalize(d.join("device")).unwrap_or_default();
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.strip_prefix("usb").is_some_and(|n| n.starts_with(|ch: char| ch.is_ascii_digit()))
    })
}

fn netdevs(net: &Net) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(net.path("/sys/class/net"))
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The `device` section of the LAN bridge.
fn bridge_section<'a>(net: &Net, sections: &'a [Section]) -> Option<&'a Section> {
    sections.iter().find(|s| s.stype == "device" && s.get("name") == Some(net.cfg().lan_bridge.as_str()))
}

fn has_port(section: Option<&Section>, name: &str) -> bool {
    section.is_some_and(|s| {
        s.list("ports")
            .iter()
            .flat_map(|p| p.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .any(|p| p == name)
    })
}

fn names(v: &str, nic: &str) -> bool {
    v.split_whitespace().any(|f| f == nic || f.strip_prefix(nic).is_some_and(|r| r.starts_with('.')))
}

/// Who else has this interface: a bridge other than the LAN's it is a port of now, a network section whose device,
/// ifname or ports name it (or a VLAN of it), or an interface netifd runs on it. `Err` when netifd cannot be asked:
/// then nobody can say it is free.
async fn owner(net: &Net, sections: &[Section], lan: Option<&Section>, name: &str) -> Result<Option<String>> {
    let master = std::fs::read_link(net.path(format!("/sys/class/net/{name}/master")))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    if let Some(m) = master.filter(|m| m != &net.cfg().lan_bridge) {
        return Ok(Some(m));
    }
    for s in sections {
        for opt in ["device", "ifname", "ports"] {
            if opt == "ports" && lan.is_some_and(|l| l.name == s.name) {
                continue;
            }
            if s.list(opt).iter().any(|v| names(v, name)) {
                return Ok(Some(s.name.clone()));
            }
        }
    }
    let dump = net.platform().netifd().dump().await?;
    let busy = dump.iter().any(|i| {
        ["device", "l3_device"].iter().any(|k| i.get(*k).and_then(Value::as_str).is_some_and(|d| names(d, name)))
    });
    Ok(busy.then(|| "netifd".to_owned()))
}

async fn attach(net: &Net, name: &str) -> bool {
    tools::ip_quiet(&["link", "set", "dev", name, "up"]).await
        && tools::ip_quiet(&["link", "set", "dev", name, "master", &net.cfg().lan_bridge]).await
}

/// Every USB NIC saved as a LAN port joins the bridge (the role readback can lag behind host netdev events, so the
/// role is not asked).
pub(crate) async fn sync_all(net: &Net) {
    let Ok(sections) = net.platform().uci().load("network").await else { return };
    let lan = bridge_section(net, &sections);
    for n in netdevs(net) {
        if usb_net(net, &n) && has_port(lan, &n) {
            attach(net, &n).await;
        }
    }
}

/// One NIC, when it is saved as a LAN port.
pub(crate) async fn sync_one(net: &Net, name: &str) {
    if !usb_net(net, name) {
        return;
    }
    let Ok(sections) = net.platform().uci().load("network").await else { return };
    if has_port(bridge_section(net, &sections), name) {
        attach(net, name).await;
    }
}

/// `f50.usb adapters`.
pub(crate) async fn adapters(net: &Net) -> Result<Value> {
    if super::role::now(net).as_deref() != Some("host") {
        return Ok(json!({ "devices": [], "host_only": true }));
    }
    let sections = net.platform().uci().load("network").await?;
    let lan = bridge_section(net, &sections);
    let mut out = Vec::new();
    for name in netdevs(net).into_iter().filter(|n| usb_net(net, n)) {
        let joined = has_port(lan, &name);
        let (eligible, by) = if joined {
            attach(net, &name).await;
            (true, None)
        } else {
            match owner(net, &sections, lan, &name).await {
                Ok(None) => (true, None),
                Ok(Some(o)) => (false, Some(o)),
                Err(_) => (false, Some("unknown".to_owned())),
            }
        };
        // an adapter another network has is listed, but not brought up or offered
        if eligible && !joined {
            tools::ip_quiet(&["link", "set", "dev", &name, "up"]).await;
        }
        let carrier = fsx::read_line(&net.path(format!("/sys/class/net/{name}/carrier"))).as_deref() == Some("1");
        out.push(json!({ "name": name, "carrier": carrier, "in_lan": joined, "eligible": eligible, "owner": by }));
    }
    Ok(json!({ "devices": out, "host_only": false }))
}

/// `f50.usb adapter_add`: the NIC into br-lan's ports (UCI, netifd reloads) and into the bridge now.
pub(crate) async fn add(net: &Net, name: &str) -> Result<Value> {
    if super::role::now(net).as_deref() != Some("host") {
        return Err(Error::failed("the USB host role is required"));
    }
    if !usb_net(net, name) {
        return Err(Error::bad_value("iface"));
    }
    let sections = net.platform().uci().load("network").await?;
    let lan = bridge_section(net, &sections).ok_or_else(|| Error::unsupported("the LAN bridge's device section"))?;
    if !has_port(Some(lan), name) {
        let by = owner(net, &sections, Some(lan), name)
            .await
            .map_err(|_| Error::busy("could not determine the adapter's owner; try again later"))?;
        if let Some(o) = by {
            return Err(Error::busy(format!("the adapter belongs to another network ({o}) and cannot join the LAN")));
        }
        tools::ip(&["link", "set", "dev", name, "up"]).await?;
        let c = Change::AddList { section: lan.name.clone(), option: "ports".into(), value: name.into() };
        net.platform().uci().apply("network", &[c]).await?;
        net.platform().netifd().reload().await?;
    }
    if !attach(net, name).await {
        return Err(Error::failed("cannot attach the USB interface to the LAN bridge"));
    }
    Ok(json!({ "added": name }))
}

fn rescan_marker(net: &Net) -> std::path::PathBuf {
    net.run_dir().join("usb-host-rescan-done")
}

pub(crate) fn clear_rescan(net: &Net) {
    let _ = std::fs::remove_file(rescan_marker(net));
}

/// The first hub may enumerate before its downstream Ethernet dongle has power: one rescan, 2 s after the switch,
/// when no USB NIC is there - the first external hub re-authorised once (a fresh downstream port scan without
/// touching the role). Never holds up the boot, never polls.
pub(crate) fn spawn_rescan(net: &Net) {
    let n = net.clone();
    net.spawn("usb-host-rescan", async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if super::role::now(&n).as_deref() != Some("host") || netdevs(&n).iter().any(|d| usb_net(&n, d)) {
            return;
        }
        if rescan_marker(&n).exists() || f50_core::fsx::private_dir(&n.run_dir()).is_err() {
            return;
        }
        let _ = fsx::write_atomic(&rescan_marker(&n), b"", 0o600);
        let bus = n.path("/sys/bus/usb/devices");
        let Ok(rd) = std::fs::read_dir(&bus) else { return };
        let mut hubs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| is_external_hub(p)).collect();
        hubs.sort();
        if let Some(hub) = hubs.first() {
            let auth = hub.join("authorized");
            if std::fs::write(&auth, "0\n").is_ok() && std::fs::write(&auth, "1\n").is_ok() {
                let name = hub.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                tools::logger("unisoc-usb", &format!("rescanned host hub {name} after missing USB NIC")).await;
            }
        }
    });
}

fn is_external_hub(p: &Path) -> bool {
    let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let top = f50_core::rpc::matches("[0-9]+-[0-9]+", &name);
    top && fsx::read_line(&p.join("bDeviceClass")).as_deref() == Some("09") && p.join("authorized").exists()
}

/// What is plugged into the port in the host role: each USB device with its product and speed (Mbit/s).
pub(crate) fn devices(net: &Net) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(net.path("/sys/bus/usb/devices")) else { return Vec::new() };
    let mut v: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            n.starts_with(|c: char| c.is_ascii_digit()) && !n.contains(':') && p.join("product").exists()
        })
        .map(|p| {
            json!({
                "port": p.file_name().map(|s| s.to_string_lossy().into_owned()),
                "manufacturer": fsx::read_line(&p.join("manufacturer")),
                "product": fsx::read_line(&p.join("product")),
                "id": format!("{}:{}", fsx::read_line(&p.join("idVendor")).unwrap_or_default(), fsx::read_line(&p.join("idProduct")).unwrap_or_default()),
                "speed": fsx::read_line(&p.join("speed")),
            })
        })
        .collect();
    v.sort_by(|a, b| a["port"].as_str().cmp(&b["port"].as_str()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_vlans() {
        assert!(names("eth1", "eth1"));
        assert!(names("lan1 eth1.10", "eth1"));
        assert!(!names("eth10", "eth1"));
    }
}

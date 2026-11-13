//! The USB-C port: its role (DWC3 role switch: device, the USB network and console to a computer; or host, USB
//! NICs through a powered hub), the network function the gadget offers a computer (NCM, ECM, RNDIS or NCM + RNDIS,
//! made by boot/init from `/etc/f50/usb-net`), and the host-side NICs that join the LAN.
//!
//! - [`role`]: the role now, the boot policy, the switch (after the reply: the RPC may ride this very link);
//! - [`gadget`]: what the gadget has bound, unbind/bind, the re-enumerations (FINDINGS 237, K10, K12, K67);
//! - [`host`]: USB NICs, their owners, joining br-lan;
//! - [`hotplug`]: netifd's and the kernel's events.
//!
//! The F50 has no battery and no OTG boost to drive: host only works with something powering the port. Ported from
//! `device-usb` (the F50's raw role switch), `mu300-usb net`/`net-name` and `mu300-usb-reset`; the U30 Air's charger
//! boost and USB-C power delivery are not carried over.

pub(crate) mod gadget;
pub(crate) mod host;
pub(crate) mod hotplug;
pub(crate) mod role;

use f50_core::rpc::Params;
use f50_core::{Error, Result, fsx};
use serde_json::{Value, json};

use crate::Net;

pub(crate) const MODES: &[&str] = &["ncm", "ecm", "rndis", "multi"];

/// The mode boot/init makes from the next boot on (`default`: NCM), and whether that is for one boot only.
pub(crate) fn net_choice(net: &Net) -> (String, bool) {
    let m = fsx::read_line(&net.legacy("usb-net")).unwrap_or_default();
    if MODES.contains(&m.as_str()) { (m, net.legacy("usb-net.once").exists()) } else { ("default".into(), false) }
}

/// What boot/init made this boot.
pub(crate) fn net_applied(net: &Net) -> Option<String> {
    let a: String =
        fsx::read_line(&net.path(&net.cfg().usb_net_applied))?.chars().filter(char::is_ascii_lowercase).collect();
    MODES.contains(&a.as_str()).then_some(a)
}

/// `usb` or `eth`: what a Linux computer names the link.
pub(crate) fn net_name(net: &Net) -> &'static str {
    if fsx::read_line(&net.legacy("usb-net-name")).as_deref() == Some("eth") { "eth" } else { "usb" }
}

fn write_word(net: &Net, name: &str, word: &str) -> Result<()> {
    let p = net.legacy(name);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::io(d.display(), &e))?;
    }
    fsx::write_atomic(&p, format!("{word}\n").as_bytes(), 0o644)
}

/// The network mode for the next boot (`default`: the file goes, NCM), for good or `once`.
pub(crate) fn set_net(net: &Net, mode: &str, once: bool) -> Result<()> {
    if role::now(net).as_deref() == Some("host") {
        return Err(Error::failed("the USB network mode is unavailable in the host role"));
    }
    if role::boot_host(net) {
        return Err(Error::failed("switch the host role at boot off before choosing a USB network mode"));
    }
    if mode == "default" {
        let _ = std::fs::remove_file(net.legacy("usb-net"));
        let _ = std::fs::remove_file(net.legacy("usb-net.once"));
        return Ok(());
    }
    write_word(net, "usb-net", mode)?;
    if once {
        fsx::write_atomic(&net.legacy("usb-net.once"), b"", 0o644)
    } else {
        let _ = std::fs::remove_file(net.legacy("usb-net.once"));
        Ok(())
    }
}

pub(crate) fn set_net_name(net: &Net, name: &str) -> Result<()> {
    if name == "eth" {
        write_word(net, "usb-net-name", "eth")
    } else {
        let _ = std::fs::remove_file(net.legacy("usb-net-name"));
        Ok(())
    }
}

/// `f50.usb get`.
pub(crate) async fn get(net: &Net) -> Result<Value> {
    let role = role::now(net);
    let (mode, once) = net_choice(net);
    let current = if role.as_deref() == Some("host") { "none".to_owned() } else { gadget::bound_function(net) };
    Ok(json!({
        "role": role.as_deref().unwrap_or("unavailable"),
        "role_boot": if role::boot_host(net) { "host" } else { "device" },
        "host_supported": net.path(&net.cfg().usb_role).exists(),
        "net_mode": mode,
        "net_scope": if once { "once" } else { "permanent" },
        "net_now": net_applied(net),
        "net_current": current,
        "net_name": net_name(net),
        "link": gadget::link(net),
        "devices": host::devices(net),
        "adb": crate::adbd::summary(net),
    }))
}

/// `f50.usb set`: the role (now; with `scope permanent` the host role at every boot too: device is the boot
/// default), the network mode (`scope` once or permanent) and the name. The role switch comes after the reply.
pub(crate) async fn set(net: &Net, p: &Params) -> Result<Value> {
    let scope = p.str("scope").unwrap_or("permanent");
    if p.str("role").is_none() && p.str("net").is_none() && p.str("name").is_none() {
        return Err(Error::missing("role"));
    }
    if let Some(m) = p.str("net") {
        set_net(net, m, scope == "once")?;
    }
    if let Some(n) = p.str("name") {
        set_net_name(net, n)?;
    }
    let mut pending = false;
    if let Some(r) = p.str("role") {
        pending = role::set(net, r, r == "host" && scope == "permanent")?;
    }
    let (mode, once) = net_choice(net);
    Ok(json!({ "pending": pending, "net_mode": mode, "once": once, "net_name": net_name(net) }))
}

/// See [`Net::boot_usb`].
pub(crate) async fn boot(net: &Net) -> Result<()> {
    // a one-boot USB network mode is used up once boot/init has made it (it says so in usb-net-applied)
    if net.legacy("usb-net.once").exists() && net_applied(net).is_some() {
        let _ = std::fs::remove_file(net.legacy("usb-net"));
        let _ = std::fs::remove_file(net.legacy("usb-net.once"));
        tracing::info!("the USB network mode for one boot was used: NCM (the default) from the next boot");
    }
    if role::boot_host(net) {
        // the host role at boot excludes a gadget mode of its own
        let _ = std::fs::remove_file(net.legacy("usb-net"));
        let _ = std::fs::remove_file(net.legacy("usb-net.once"));
        if role::now(net).as_deref() != Some("host") {
            role::write(net, "host")?;
        }
        host::spawn_rescan(net);
    }
    host::sync_all(net).await;
    Ok(())
}

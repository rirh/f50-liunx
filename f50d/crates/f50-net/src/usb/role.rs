//! The port's role through the DWC3 role switch (`/sys/class/usb_role/25100000.dwc3-role-switch/role`), as
//! `device-usb` wrote it for the F50. Device is the boot default; host at every boot is the only persistent role
//! (`/etc/f50d/usb-role`). The switch is written a moment after the reply: a synchronous write can tear down the very
//! RPC carrying its response (the browser then reports a failure after success).

use std::time::Duration;

use f50_core::{Error, Result, fsx};

use super::host;
use crate::Net;

fn policy_path(net: &Net) -> std::path::PathBuf {
    net.platform().paths().state().join("usb-role")
}

/// The role now (`host`, `device`, `none`), `None` without a role switch.
pub(crate) fn now(net: &Net) -> Option<String> {
    let r = net.platform().sysfs().read(&net.cfg().usb_role)?;
    // "[host] device" on some kernels: the chosen one
    let chosen = r.split_once('[').and_then(|(_, rest)| rest.split_once(']')).map(|(c, _)| c.to_owned());
    Some(chosen.unwrap_or(r))
}

/// The host role at every boot.
pub(crate) fn boot_host(net: &Net) -> bool {
    fsx::read_line(&policy_path(net)).as_deref() == Some("host")
}

/// Writes the role switch.
pub(crate) fn write(net: &Net, role: &str) -> Result<()> {
    if !net.path(&net.cfg().usb_role).exists() {
        return Err(Error::unsupported("USB role switch"));
    }
    net.platform().sysfs().write(&net.cfg().usb_role, &format!("{role}\n")).map_err(|e| e.context("USB role switch"))
}

/// The role (and the boot policy) saved, the switch scheduled; whether a switch is pending.
pub(crate) fn set(net: &Net, role: &str, at_boot: bool) -> Result<bool> {
    if !net.path(&net.cfg().usb_role).exists() {
        return Err(Error::unsupported("USB role switch"));
    }
    let p = policy_path(net);
    if at_boot {
        fsx::private_dir(&net.platform().paths().state())?;
        fsx::write_atomic(&p, b"host\n", 0o600)?;
        // the host role at boot replaces the gadget's mode policy
        let _ = std::fs::remove_file(net.legacy("usb-net"));
        let _ = std::fs::remove_file(net.legacy("usb-net.once"));
    } else {
        let _ = std::fs::remove_file(&p);
    }
    if now(net).as_deref() == Some(role) {
        return Ok(false);
    }
    let role = role.to_owned();
    net.later("usb-role", Duration::from_millis(250), move |net| async move { apply(&net, &role).await });
    Ok(true)
}

/// The switch itself; in the host role one rescan for a NIC behind a hub that came up late.
pub(crate) async fn apply(net: &Net, role: &str) -> Result<()> {
    if now(net).as_deref() != Some(role) {
        write(net, role).map_err(|e| {
            tracing::warn!(role, error = %e, "USB role switch failed");
            e
        })?;
    }
    if role == "host" {
        host::spawn_rescan(net);
    } else {
        host::clear_rescan(net);
    }
    Ok(())
}

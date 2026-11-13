//! adb as a function of the USB gadget (FunctionFS), next to the network and the serial console.
//!
//! The gadget is made and bound by boot/init; adb joins later, so adding it rebinds the gadget once, which looks like
//! a replug to the computer. FunctionFS wants its descriptors before the bind: the function goes into the
//! configuration once adbd has written them (`ep1` exists). Interfaces: the network function, the console, then adb
//! (ff/42/01): adding adb changes no number. bcdDevice 0x0303 (0x0304 with RNDIS, 0x0306 with RNDIS + NCM) while
//! adb is in the gadget: Windows caches the MS OS descriptors per VID/PID/bcdDevice, and adbd asks for WinUSB as
//! Android's does. The MS OS descriptors stay on once turned on: f_ncm/f_rndis keep a stale os_desc_table after an
//! unbind, and a bind without them would free it twice at the next unbind (the oops of 2026-10-22, kernel patch 0027).
//! Ported from `f50-adbd` (`with_adb`, `without_adb`, `rebind`, `prepare`, `teardown`, `usable`).

use std::path::PathBuf;
use std::time::Duration;

use f50_core::{Error, Result};

use crate::usb::gadget as g;
use crate::{Net, tools};

fn gadget(net: &Net) -> PathBuf {
    net.path(&net.cfg().gadget)
}

fn function(net: &Net) -> PathBuf {
    gadget(net).join("functions/ffs.adb")
}

fn link(net: &Net) -> PathBuf {
    gadget(net).join("configs/c.1/f3")
}

pub(crate) fn linked(net: &Net) -> bool {
    std::fs::symlink_metadata(link(net)).is_ok()
}

pub(crate) fn ffs_ready(net: &Net) -> bool {
    net.path(&net.cfg().ffs).join("ep1").exists()
}

fn rndis(net: &Net) -> bool {
    std::fs::read_link(gadget(net).join("configs/c.1/f1")).is_ok_and(|t| t.to_string_lossy().contains("rndis"))
}

/// "multi" (boot/init): RNDIS in f1 and NCM beside it in f4.
fn multi(net: &Net) -> bool {
    rndis(net) && std::fs::symlink_metadata(gadget(net).join("configs/c.1/f4")).is_ok()
}

fn put(net: &Net, rel: &str, v: &str) {
    let _ = std::fs::write(gadget(net).join(rel), v);
}

/// The configuration with adb (unbound only: configfs refuses links and descriptors of a bound gadget).
fn with_adb(net: &Net) -> bool {
    if !linked(net) && std::os::unix::fs::symlink(function(net), link(net)).is_err() {
        return false;
    }
    if multi(net) {
        put(net, "bcdDevice", "0x0306");
    } else if rndis(net) {
        put(net, "bcdDevice", "0x0304");
    } else {
        put(net, "bcdDevice", "0x0303");
        put(net, "os_desc/b_vendor_code", "0xcd");
        put(net, "os_desc/qw_sign", "MSFT100");
        put(net, "os_desc/use", "1");
        let os = gadget(net).join("os_desc/c.1");
        if std::fs::symlink_metadata(&os).is_err() {
            let _ = std::os::unix::fs::symlink(gadget(net).join("configs/c.1"), os);
        }
    }
    true
}

fn without_adb(net: &Net) {
    let _ = std::fs::remove_file(link(net));
    put(
        net,
        "bcdDevice",
        if multi(net) {
            "0x0305"
        } else if rndis(net) {
            "0x0302"
        } else {
            "0x0301"
        },
    );
}

/// How a rebind ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rebind {
    Bound,
    /// Bound, but without the adb that was wanted (adbd not ready, or the gadget would not bind with it).
    WithoutAdb,
    Unbound,
}

/// Unbind, adb in (`want`, when adbd is ready) or out, bind again, under the rebind lock. A gadget that will not bind
/// with adb is bound without it: the network and the console come first.
pub(crate) async fn rebind(net: &Net, want: bool) -> Rebind {
    let Some(udc) = g::udc_name(net) else {
        tracing::warn!("adbd: no USB device controller");
        return Rebind::Unbound;
    };
    let guard = g::lock(net).await;
    g::unbind(net);
    if want && ffs_ready(net) && with_adb(net) && g::bind(net, &udc, 10).await {
        drop(guard);
        g::lan_back(net).await;
        tracing::info!(udc, "gadget bound with adb");
        return Rebind::Bound;
    }
    without_adb(net);
    let ok = g::bind(net, &udc, 30).await;
    drop(guard);
    if !ok {
        tracing::warn!(udc, "could not bind the gadget");
        return Rebind::Unbound;
    }
    g::lan_back(net).await;
    if want {
        tracing::info!(udc, "gadget bound without adb (adbd is not ready)");
        return Rebind::WithoutAdb;
    }
    Rebind::Bound
}

fn mounted(net: &Net, target: &str, fstype: &str) -> bool {
    let m = std::fs::read_to_string(net.path("/proc/mounts")).unwrap_or_default();
    m.lines().any(|l| {
        let w: Vec<&str> = l.split_whitespace().collect();
        w.get(1) == Some(&target) && w.get(2) == Some(&fstype)
    })
}

/// configfs, the function and FunctionFS's mount.
pub(crate) async fn prepare(net: &Net) -> Result<()> {
    if !gadget(net).is_dir() {
        // init unmounts configfs before switch_root
        let _ = std::fs::create_dir_all(net.path("/sys/kernel/config"));
        let _ = tools::mount("configfs", "configfs", "/sys/kernel/config").await;
    }
    if !gadget(net).is_dir() {
        return Err(Error::unsupported(format!("a USB gadget at {}", net.cfg().gadget.display())));
    }
    if !function(net).is_dir() && std::fs::create_dir(function(net)).is_err() {
        return Err(Error::unsupported("the FunctionFS function (a kernel without USB_CONFIGFS_F_FS?)"));
    }
    let ffs = net.cfg().ffs.to_string_lossy().into_owned();
    let _ = std::fs::create_dir_all(net.path(&net.cfg().ffs));
    if !mounted(net, &ffs, "functionfs") {
        tools::mount("functionfs", "adb", &ffs)
            .await
            .map_err(|e| e.context(format!("cannot mount FunctionFS at {ffs}")))?;
    }
    Ok(())
}

/// Out of the gadget: unbound, unlinked, bound again; then the function and its mount go.
pub(crate) async fn teardown(net: &Net) {
    if linked(net) {
        rebind(net, false).await;
    } else if !g::bound(net)
        && crate::usb::role::now(net).as_deref() != Some("host")
        && let Some(u) = g::udc_name(net)
        && g::bind(net, &u, 30).await
    {
        g::lan_back(net).await;
    }
    let ffs = net.cfg().ffs.to_string_lossy().into_owned();
    if mounted(net, &ffs, "functionfs") {
        tools::umount(&ffs).await;
    }
    if function(net).is_dir() && !linked(net) {
        let _ = std::fs::remove_dir(function(net));
    }
}

/// Why adb cannot run here, if it cannot. FunctionFS registers its filesystem only with its first function: before
/// that, the kernel's list of built-in modules tells, or making the function.
pub(crate) fn usable(net: &Net) -> Result<()> {
    if !f50_core::exec::executable(&net.path(&net.cfg().adbd)) {
        return Err(Error::unsupported(format!("adbd ({})", net.cfg().adbd.display())));
    }
    let fs = std::fs::read_to_string(net.path("/proc/filesystems")).unwrap_or_default();
    if fs.split_whitespace().any(|w| w == "functionfs") || function(net).is_dir() {
        return Ok(());
    }
    if let Some(dir) = net.platform().modules().module_dir() {
        let b = std::fs::read_to_string(dir.join("modules.builtin")).unwrap_or_default();
        if b.lines().any(|l| l.ends_with("/usb_f_fs.ko")) {
            return Ok(());
        }
    }
    if std::fs::create_dir(function(net)).is_ok() {
        let _ = std::fs::remove_dir(function(net));
        return Ok(());
    }
    Err(Error::unsupported("FunctionFS in this kernel (CONFIG_USB_CONFIGFS_F_FS)"))
}

/// Waits for adbd's first descriptors, `steps` looks 0.2 s apart.
pub(crate) async fn wait_ffs(net: &Net, steps: u32) {
    for _ in 0..steps {
        if ffs_ready(net) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

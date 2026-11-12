//! The modem's Android vendor daemons, in a chroot at [`HwConfig::vendor_root`](crate::HwConfig::vendor_root)
//! (`init.d/mu300-vendor`, `android-vendor-start`, `android-run`).
//!
//! [`stage`] is everything `android-vendor-start` did before its `exec`: the imported files' modes ([`modes`]), the
//! wait for the modem stack, the liblog sink, the property area, the partition links, the device nodes' owners
//! ([`crate::perms`]) for the one decision of [`crate::accounts`], and the chroot ([`chroot`]). [`exec`] is
//! `android-run`: one daemon, exec'd in the chroot as the account the nodes were given to ([`daemons`], [`caps`]).
//!
//! Not ported: `mu300-modem-coldboot` (a U30 Air charger boot; the F50 has none of it: FINDINGS 196), the V50's SIM
//! switches, and making the account with `useradd` (see [`crate::accounts`]).

pub(crate) mod caps;
pub(crate) mod chroot;
pub(crate) mod daemons;
pub(crate) mod modes;

use std::time::{Duration, Instant};

use f50_core::{Error, Result};

use crate::accounts::{self, VendorIds};
use crate::report::Steps;
use crate::{Hw, perms};
use daemons::VendorDaemon;

impl Hw {
    pub(crate) fn vendor_layout(&self) -> chroot::Layout {
        let c = self.config();
        chroot::Layout {
            root: self.dev(&c.vendor_root),
            dev: self.dev("/dev"),
            sys: self.dev("/sys"),
            mountinfo: self.dev("/proc/self/mountinfo"),
            logdw: self.dev(&c.logdw),
            vendor_log: self.dev(&c.vendor_log),
            cmdline_out: self.dev(&c.android_cmdline),
            proc_cmdline: self.dev("/proc/cmdline"),
            bootargs: self.dev("/proc/device-tree/chosen/bootargs"),
            initramfs_run: self.dev(&c.initramfs_run),
        }
    }

    pub(crate) fn vendor_ids(&self) -> VendorIds {
        VendorIds::resolve(&self.dev("/etc"), &self.config().vendor_account)
    }
}

/// Runs `f` off the runtime (file walks, mounts).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| Error::failed(format!("blocking step: {e}")))
}

/// Prepares the chroot; the caller then execs [`VendorDaemon::ModemControl`] (`exec` below).
pub(crate) async fn stage(hw: &Hw, steps: &mut Steps) {
    hw.mark("vendor-start");
    let layout = hw.vendor_layout();
    let kmsg = hw.dev("/dev/kmsg");

    let root = layout.root.clone();
    match blocking(move || modes::repair(&root)).await {
        Ok(m) if m.wrong == 0 => {
            steps.skipped("vendor-modes", "the imported Android files are as the daemons need them")
        }
        Ok(m) if m.left == 0 => {
            let msg = format!(
                "{} imported Android files and directories had an owner or a mode the vendor daemons cannot work with \
                 (the first: {}); now root's, 0755/0644/0444",
                m.wrong,
                m.first_wrong.map(|p| p.display().to_string()).unwrap_or_default()
            );
            crate::timeline::kmsg(&kmsg, "f50d vendor", &msg);
            hw.mark("vendor-modes-repaired");
            steps.done("vendor-modes", msg);
        }
        Ok(m) => {
            let msg = format!(
                "{} imported Android files and directories have an owner or a mode the vendor daemons cannot work with, \
                 and that could not be changed (the first: {}); the modem may not start",
                m.left,
                m.first_left.map(|p| p.display().to_string()).unwrap_or_default()
            );
            crate::timeline::kmsg(&kmsg, "f50d vendor", &msg);
            hw.mark("vendor-modes-wrong");
            steps.failed("vendor-modes", msg);
        }
        Err(e) => steps.failed("vendor-modes", e.message()),
    }

    // on a mainline kernel the modem stack is loaded from userspace (the hardware stage), so this can come first
    let t0 = Instant::now();
    let stack = || layout.dev.join("modem").exists() || layout.dev.join("pmsys").exists();
    while !stack() && t0.elapsed() < hw.config().modem_stack_wait {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if stack() {
        steps.done("modem-stack", format!("present after {}s", t0.elapsed().as_secs()));
    } else {
        steps.failed("modem-stack", "no /dev/modem or /dev/pmsys: starting the daemons anyway");
    }
    hw.mark("vendor-stack-present");

    let l = layout.clone();
    match blocking(move || l.start_logdw()).await {
        Ok(Ok(true)) => steps.done("logdw", "started"),
        Ok(Ok(false)) => steps.skipped("logdw", "already listening"),
        Ok(Err(e)) => steps.failed("logdw", e),
        Err(e) => steps.failed("logdw", e.message()),
    }

    let l = layout.clone();
    match blocking(move || l.bind_properties()).await {
        Ok(Ok(true)) => steps.done("properties", "bound"),
        Ok(Ok(false)) => steps.skipped("properties", "already there"),
        Ok(Err(e)) => steps.fatal("properties", e),
        Err(e) => steps.fatal("properties", e.message()),
    }
    hw.mark("vendor-props");

    let l = layout.clone();
    match blocking(move || l.link_partitions()).await {
        Ok(Ok(n)) => steps.done("partition-links", format!("{n} partitions")),
        Ok(Err(e)) => steps.failed("partition-links", e),
        Err(e) => steps.failed("partition-links", e.message()),
    }

    let ids = hw.vendor_ids();
    let (root, etc) = (hw.dev("/"), hw.dev("/etc"));
    match blocking(move || perms::apply_all(&root, ids, accounts::gpu_gid(&etc))).await {
        Ok(a) if a.failed == 0 => {
            steps.done("node-owners", format!("{ids:?}: {} nodes, {} changed", a.seen, a.changed))
        }
        Ok(a) => steps.failed(
            "node-owners",
            format!("{ids:?}: {} of {} nodes not changed ({})", a.failed, a.seen, a.first_failure.unwrap_or_default()),
        ),
        Err(e) => steps.failed("node-owners", e.message()),
    }
    hw.mark("vendor-perms");

    let l = layout.clone();
    match blocking(move || l.mount_inside().and_then(|()| l.android_cmdline())).await {
        Ok(Ok(slot)) => steps.done("chroot", format!("ready; the daemons load slot {}'s modem images", slot.letter())),
        Ok(Err(e)) => steps.fatal("chroot", e),
        Err(e) => steps.fatal("chroot", e.message()),
    }
    hw.mark("vendor-chroot-ready");
}

/// Replaces this process with `d` in the chroot, once the chroot's `/dev` is mounted; returns only the reason it could
/// not.
pub(crate) async fn exec(hw: &Hw, d: VendorDaemon) -> Error {
    let layout = hw.vendor_layout();
    let dev = layout.root.join("dev");
    let t0 = Instant::now();
    while !crate::sys::is_mount_point(&layout.mountinfo, &dev) && t0.elapsed() < hw.config().chroot_wait {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let ids = hw.vendor_ids();
    tracing::info!("starting {} as {ids:?}", d.name());
    daemons::exec(&layout.root, d, ids)
}

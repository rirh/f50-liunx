//! The ZTE F50's boot-time bring-up on OpenWrt: kernel modules, device permissions, the vendor daemons the modem
//! needs, the Wi-Fi's rules, boot success marks.
//!
//! The work runs in four [`Stage`]s, each a `f50d boot <stage>` process that procd (or preinit) starts, in the order
//! the shell layer's init scripts had (`f50d/openwrt/init.d`, `f50d/openwrt/preinit`):
//!
//! | stage | when | what must be up | replaces |
//! |---|---|---|---|
//! | [`Stage::Early`] | preinit, `preinit_essential` hook `05` (background, 420 s) | `/proc`, `/sys`, `/dev` (preinit's), `/run/f50` (initramfs) | `05_mu300_early_recorder`, `early-recorder` |
//! | [`Stage::Vendor`] | `START=09`, procd instance `modem_control` | preinit's `/dev`, `/proc`, `/sys`, `/run`; *not* `/var/log` (S10) | `mu300-vendor`, `android-vendor-start` |
//! | [`Stage::Hardware`] | `START=12`, `boot()`, in the background | S10 `boot` (`/var`, `/tmp`, UCI); before netifd (S20) | `mu300-hw`'s module helper, regdb, board check, `extra-modules` |
//! | [`Stage::Post`] | `START=99`, procd one-shot instance | everything; counts the boot good after 30 s | `mu300-post`'s `bootok`, `mu300-next-boot --rearm` |
//!
//! Vendor runs before Hardware on purpose: its one job on the boot path is to release the CP, which then needs about
//! fifteen seconds before its AT channel answers, and it waits for the modem stack itself (up to 90 s), so starting
//! earlier cannot make it wrong. The leading zero matters: rc.common embeds START verbatim, `S9` sorted after `S19`
//! and delayed the CP by ~31 s.
//!
//! Besides the stages: [`Hw::exec_vendor`] (one vendor daemon, exec'd in the chroot), [`Hw::hotplug_wcn`] (the Wi-Fi
//! firmware's reset), [`Hw::load_ipa_delegate`] (for the dial), [`Hw::arm_trial`] (before a boot image write),
//! [`modprobe_helper`] (the kernel's module helper) and the read-only `f50.boot` object.

mod accounts;
mod board;
pub mod bootctl;
mod config;
mod modules;
mod object;
mod perms;
mod recorder;
mod report;
mod slot;
mod stages;
mod storage;
mod sys;
mod timeline;
mod vendor;
mod wcn;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use f50_core::paths::Paths;
use f50_core::rpc::Object;
use f50_core::{Error, Result};
use f50_platform::Platform;
use serde::{Deserialize, Serialize};

pub use accounts::VendorIds;
pub use bootctl::{ArmOutcome, BootInfo};
pub use config::HwConfig;
pub use modules::dele::DelegateState;
pub use report::{Outcome, StageReport, Step};
pub use slot::Slot;
pub use vendor::daemons::VendorDaemon;
pub use wcn::{WcnEvent, WcnOutcome};

/// A boot stage, in the order procd runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// preinit: the early recorder.
    Early,
    /// S09: the vendor daemons' chroot (then `modem_control` is exec'd).
    Vendor,
    /// S12: the module helper, the Wi-Fi's rules, the vendor drivers.
    Hardware,
    /// S99: the boot counts as good.
    Post,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Early, Stage::Vendor, Stage::Hardware, Stage::Post];

    pub const fn as_str(self) -> &'static str {
        match self {
            Stage::Early => "early",
            Stage::Vendor => "vendor",
            Stage::Hardware => "hardware",
            Stage::Post => "post",
        }
    }
}

impl FromStr for Stage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Stage::ALL.into_iter().find(|x| x.as_str() == s).ok_or_else(|| Error::not_found("boot stage"))
    }
}

/// The bring-up. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Hw {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    cfg: HwConfig,
    platform: Platform,
}

impl Hw {
    pub fn new(cfg: HwConfig, platform: Platform) -> Hw {
        Hw { inner: Arc::new(Inner { cfg, platform }) }
    }

    pub fn config(&self) -> &HwConfig {
        &self.inner.cfg
    }

    pub(crate) fn platform(&self) -> &Platform {
        &self.inner.platform
    }

    /// A device path under the platform's root.
    pub(crate) fn dev(&self, p: impl AsRef<Path>) -> PathBuf {
        self.inner.platform.paths().sys(p)
    }

    pub(crate) fn cmdline_sources(&self) -> Vec<PathBuf> {
        self.inner.cfg.cmdline_sources.iter().map(|p| self.dev(p)).collect()
    }

    pub(crate) fn mark(&self, words: &str) {
        timeline::mark(&self.dev(&self.inner.cfg.timeline), &self.dev("/proc/uptime"), words);
    }

    fn reports_dir(&self) -> PathBuf {
        self.inner.platform.paths().run().join("boot")
    }

    pub(crate) fn bootctl(&self) -> bootctl::mark::BootCtl {
        let c = &self.inner.cfg;
        bootctl::mark::BootCtl {
            paths: self.inner.platform.paths().clone(),
            run: self.dev(&c.initramfs_run),
            conf: self.dev(&c.default_boot),
            disk_dirs: c.disk_dirs.clone(),
            cmdline_sources: self.cmdline_sources(),
            default_attempts: c.default_attempts,
            lock_wait: c.boot_lock_wait,
        }
    }

    /// Runs one stage. Its steps never stop the boot: a failure is in the report (`ok: false` when the stage could
    /// not do what it is for), which is also kept for `f50.boot get`.
    pub async fn stage(&self, stage: Stage) -> Result<StageReport> {
        let mut steps = report::Steps::new(stage, timeline::uptime_secs(&self.dev("/proc/uptime")));
        match stage {
            Stage::Early => stages::early(self, &mut steps).await,
            Stage::Vendor => vendor::stage(self, &mut steps).await,
            Stage::Hardware => stages::hardware(self, &mut steps).await,
            Stage::Post => stages::post(self, &mut steps).await,
        }
        let r = steps.finish();
        report::save(&self.reports_dir(), &r);
        Ok(r)
    }

    /// Replaces this process with a vendor daemon in the chroot (after [`Stage::Vendor`] for `modem_control`; on
    /// their own for the other two, which wait for the chroot). Returns only the reason it could not.
    pub async fn exec_vendor(&self, d: VendorDaemon) -> Error {
        vendor::exec(self, d).await
    }

    /// A good boot: the Linux slot re-armed (or locked), the initramfs' boot count cleared.
    pub async fn mark_boot_good(&self) -> Result<ArmOutcome> {
        let r = self.bootctl().rearm().await;
        self.mark(if r.is_ok() { "boot-good" } else { "boot-good-FAILED" });
        r
    }

    /// Before a new boot image is written (the update): the next boots are the attempts, locked or not, until one is
    /// good. `NotDefaultLinux`: nothing armed, and a reboot goes to Android by itself.
    pub async fn arm_trial(&self) -> Result<ArmOutcome> {
        self.bootctl().trial().await
    }

    /// The boot control and the last stage reports (`f50.boot get`).
    pub async fn boot_info(&self) -> Result<BootInfo> {
        let (bc, dir) = (self.bootctl(), self.reports_dir());
        tokio::task::spawn_blocking(move || bc.info(report::load_all(&dir)))
            .await
            .map_err(|e| Error::failed(format!("boot info: {e}")))
    }

    /// A `misc` hotplug event: the Wi-Fi firmware's reset reopens Wi-Fi once. The hotplug script must not wait for
    /// it (procd runs hotplug scripts one at a time).
    pub async fn hotplug_wcn(&self, ev: &WcnEvent) -> Result<WcnOutcome> {
        wcn::reopen(&self.dev(&self.inner.cfg.wcn_lock), &self.dev("/proc"), ev).await
    }

    /// Loads the IPA delegate once `attached()` (the modem's `AT+CGATT?` answer is `1`) says the packet domain is
    /// up; for the dial, between the registration and the PDP context (FINDINGS 13f, 241). `Err`: the PDP context
    /// must not be activated now.
    pub async fn load_ipa_delegate<F, Fut>(&self, attached: F) -> Result<DelegateState>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let dir = self.platform().modules().module_dir();
        let ko = dir.map(|d| vec![d.join("sipa-dele.ko"), d.join("extra/sipa-dele.ko")]).unwrap_or_default();
        let d = modules::dele::Delegate {
            ko,
            sysfs: self.dev("/sys/bus/platform/drivers/sipa_delegate"),
            proc_modules: self.dev("/proc/modules"),
            wait: self.inner.cfg.dele_wait,
        };
        modules::dele::load(&d, attached).await
    }

    /// The ubus objects: `f50.boot` (read only).
    pub fn objects(&self) -> Vec<Object> {
        vec![object::boot(self.clone())]
    }
}

/// The kernel's module helper (`HELPER -q -- REQUEST`), for a process started by the kernel through the
/// [`HwConfig::modprobe_helper`] link: no runtime is needed. Refused: returns 1 (the exit status). Handed on: this
/// process becomes `/sbin/modprobe` with the same arguments; 127 only if that fails.
pub fn modprobe_helper(cfg: &HwConfig, paths: &Paths, args: &[OsString]) -> i32 {
    let krel = f50_core::fsx::read_line(&paths.sys("/proc/sys/kernel/osrelease")).unwrap_or_default();
    let g = modules::guard::Guard {
        cache: paths.run().join("modprobe-guard"),
        moddir: paths.sys(format!("/lib/modules/{krel}")),
        krel,
    };
    let allow: Vec<PathBuf> = cfg.autoload_allow.iter().map(|a| paths.sys(a)).collect();
    modules::guard::run_helper(&g, &allow, &cfg.modprobe, &paths.sys("/dev/kmsg"), args)
}

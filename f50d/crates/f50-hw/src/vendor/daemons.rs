//! The three Android vendor daemons the modem needs, and how each is started (`android-run`).
//!
//! The binary is exec'd directly, never through the linker or a wrapper: `sprd_modem_loader` rejects every ioctl
//! unless the task's name (comm) is exactly `modem_control` (FINDINGS 5, requirement 4).

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::str::FromStr;

use f50_core::{Error, Result};
use serde::Serialize;

use super::caps::{self, CAP_BLOCK_SUSPEND, CAP_CHOWN, CAP_SYS_CHROOT, CAP_SYS_TIME, CapMask, Drop};
use crate::accounts::VendorIds;

/// A vendor daemon f50d starts in the chroot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorDaemon {
    /// The modem's NV server: the CP asks it for its data while it boots, and a CP that does not get it stalls with
    /// "wait modem alive timeout". Root, as on Android.
    CpDiskserver,
    /// Loads pm_sys and the modem through Trusty; without it the PM co-processor's watchdog cuts the power ~290 s
    /// after boot (FINDINGS 5).
    ModemControl,
    /// The CP's time and RTC requests.
    Refnotify,
}

impl VendorDaemon {
    pub const fn name(self) -> &'static str {
        match self {
            VendorDaemon::CpDiskserver => "cp_diskserver",
            VendorDaemon::ModemControl => "modem_control",
            VendorDaemon::Refnotify => "refnotify",
        }
    }

    /// What it keeps on Android (`None`: it never switches users and stays root).
    fn caps(self) -> Option<CapMask> {
        match self {
            VendorDaemon::CpDiskserver => None,
            VendorDaemon::ModemControl => Some(caps::mask(&[CAP_SYS_CHROOT, CAP_CHOWN, CAP_BLOCK_SUSPEND])),
            VendorDaemon::Refnotify => Some(caps::mask(&[CAP_SYS_CHROOT, CAP_SYS_TIME])),
        }
    }

    pub(crate) fn drop_for(self, ids: VendorIds) -> Drop {
        match (self.caps(), ids) {
            (Some(caps), VendorIds::Account { uid, gid }) => Drop::Account { uid, gid, caps },
            _ => Drop::Root,
        }
    }
}

impl FromStr for VendorDaemon {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "cp_diskserver" => Ok(VendorDaemon::CpDiskserver),
            "modem_control" => Ok(VendorDaemon::ModemControl),
            "refnotify" => Ok(VendorDaemon::Refnotify),
            _ => Err(Error::not_found("vendor daemon")),
        }
    }
}

/// The bionic linker's search path in the chroot.
const LD_LIBRARY_PATH: &str = "/apex/com.android.runtime/lib64/bionic:/system/lib64:/vendor/lib64";

/// Replaces this process with the daemon, in the chroot at `root`; returns only the reason it could not.
///
/// The environment is cleared: OpenWrt's procd preloads `/lib/libsetlbf.so` (`LD_PRELOAD`), which the bionic linker
/// cannot load. Programs otherwise start through `f50_core::exec::Cmd`; this one replaces the process instead.
pub(crate) fn exec(root: &Path, d: VendorDaemon, ids: VendorIds) -> Error {
    let drop = d.drop_for(ids);
    let root_owned = root.to_path_buf();
    let mut cmd = std::process::Command::new(format!("/vendor/bin/{}", d.name()));
    cmd.env_clear().env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin").env("LD_LIBRARY_PATH", LD_LIBRARY_PATH);
    #[allow(unsafe_code)]
    // SAFETY: `Command::exec` runs the closure in this process right before execve, without forking, so no
    // async-signal-safety rule applies; the closure only makes system calls (chroot, setres*id, prctl, capset).
    unsafe {
        cmd.pre_exec(move || caps::enter(&root_owned, drop));
    }
    let e = cmd.exec();
    Error::failed(format!("cannot start {} in {}: {e}", d.name(), root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_decision_for_nodes_and_daemons() {
        let acct = VendorIds::Account { uid: 999, gid: 991 };
        assert_eq!(VendorDaemon::CpDiskserver.drop_for(acct), Drop::Root);
        assert_eq!(VendorDaemon::ModemControl.drop_for(VendorIds::Android), Drop::Root);
        assert_eq!(
            VendorDaemon::ModemControl.drop_for(acct),
            Drop::Account { uid: 999, gid: 991, caps: (1 << 18) | 0x10_0000_0001 }
        );
        assert_eq!("refnotify".parse::<VendorDaemon>().ok(), Some(VendorDaemon::Refnotify));
        assert!("sh".parse::<VendorDaemon>().is_err());
    }
}

//! Who the vendor daemons are: one decision, used both for the device nodes and for the daemons themselves.
//!
//! `modem_control` and `refnotify` switch to Android's system user on their own (`setgroups`, `setuid(1000)` - the
//! number is in the binaries - then `capset`), and uid 1000 is the first human account on a distribution, so the
//! shell gave them an account of their own, `mu300vnd` (FINDINGS 41). FINDINGS 41a (issue #314) is why this is one
//! decision: when the nodes went to `mu300vnd` while the daemons still ran as 1000, the modem never started (EACCES on
//! `/dev/modem`, and five minutes later the PM watchdog cut the power, FINDINGS 5).
//!
//! The shell used the account only where it existed *and* `setpriv` could switch to it; f50d switches itself
//! (`vendor::caps`), so the rule here is: the account is used where `/etc/passwd` and `/etc/group` both have it,
//! else the daemons start as root in the chroot and switch to uid 1000 themselves, and the nodes are 1000's. f50d does
//! not create the account: OpenWrt has no `useradd`, and an image that wants it ships it (`systems/base/assemble.sh`).

use std::path::Path;

use serde::Serialize;

/// The ids the vendor daemons run as, and the device nodes belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VendorIds {
    /// The vendor account: f50d starts the daemons as it, with only the capabilities they keep on Android.
    Account { uid: u32, gid: u32 },
    /// No account: the daemons start as root and switch to Android's system user (1000) themselves.
    Android,
}

/// Android's system user, which the daemons switch to themselves.
pub const ANDROID_SYSTEM: u32 = 1000;

impl VendorIds {
    /// The decision, from `etc` (`/etc`): the account's uid in `passwd` and gid in `group`, both or neither.
    pub fn resolve(etc: &Path, account: &str) -> VendorIds {
        match (id_of(account, &etc.join("passwd")), id_of(account, &etc.join("group"))) {
            (Some(uid), Some(gid)) => VendorIds::Account { uid, gid },
            _ => VendorIds::Android,
        }
    }

    /// The owner of the vendor's nodes (`$V`).
    pub fn uid(self) -> u32 {
        match self {
            VendorIds::Account { uid, .. } => uid,
            VendorIds::Android => ANDROID_SYSTEM,
        }
    }

    /// The group of the vendor's nodes (`$VG`).
    pub fn gid(self) -> u32 {
        match self {
            VendorIds::Account { gid, .. } => gid,
            VendorIds::Android => ANDROID_SYSTEM,
        }
    }
}

/// The id (third field) of `name` in a passwd- or group-format file, as `ueventd-perms.sh`'s `id_of` reads it (no
/// NSS: OpenWrt and the initramfs have none, and these are local system accounts).
pub(crate) fn id_of(name: &str, file: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines().find_map(|l| {
        let mut f = l.split(':');
        (f.next()? == name).then_some(())?;
        f.nth(1)?.parse().ok()
    })
}

/// The GPU's group: `video`, else root (`$GPU`).
pub(crate) fn gpu_gid(etc: &Path) -> u32 {
    id_of("video", &etc.join("group")).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_or_neither() {
        let etc = std::env::temp_dir().join(format!("f50-hw-acct-{}", std::process::id()));
        std::fs::create_dir_all(&etc).expect("mkdir");
        std::fs::write(
            etc.join("passwd"),
            "root:x:0:0:root:/root:/bin/ash\nmu300vnd:x:999:999::/nonexistent:/bin/false\n",
        )
        .expect("passwd");
        std::fs::write(etc.join("group"), "root:x:0:\nvideo:x:44:\n").expect("group");
        assert_eq!(VendorIds::resolve(&etc, "mu300vnd"), VendorIds::Android);
        assert_eq!(VendorIds::Android.uid(), 1000);
        assert_eq!(gpu_gid(&etc), 44);
        std::fs::write(etc.join("group"), "root:x:0:\nmu300vnd:x:991:\n").expect("group");
        assert_eq!(VendorIds::resolve(&etc, "mu300vnd"), VendorIds::Account { uid: 999, gid: 991 });
        assert_eq!(gpu_gid(&etc), 0);
        let _ = std::fs::remove_dir_all(&etc);
    }
}

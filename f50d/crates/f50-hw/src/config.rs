//! Where the F50's bring-up finds things, as one value the binary can override.
//!
//! Every path here is an absolute *device* path; [`Hw`](crate::Hw) resolves it under the platform's root
//! ([`f50_core::paths::Paths::sys`]) so tests can move it. Several of them are not f50d's to choose: the initramfs
//! (`boot/init`, not ported) publishes the slot and the boot control blocks under `/run/f50`, counts boots in
//! `.f50/` on the Linux disk and reads `/etc/f50/default-boot`. An initramfs built before the rename uses `/run/f50`,
//! `.f50/` and `/etc/f50` instead: [`Paths::sys`](f50_core::paths::Paths::sys) takes those where the F50 names
//! are not there, so both meet f50d at the same files.

use std::path::PathBuf;
use std::time::Duration;

/// The bring-up's configuration. `Default` is the ZTE F50 on OpenWrt.
#[derive(Debug, Clone)]
pub struct HwConfig {
    /// The imported Android vendor tree the modem's daemons run in (chroot). Defaults to `/opt/f50/android`
    /// because that is where the installer (and `systems/openwrt/build-rootfs.sh`) unpacks it; f50d does not move it.
    pub vendor_root: PathBuf,
    /// The liblog sink the vendor daemons write to (`/dev/socket/logdw`; FINDINGS 5, requirement 5).
    pub logdw: PathBuf,
    /// Where logdw's output goes (the vendor daemons' own log).
    pub vendor_log: PathBuf,
    /// The vendor daemons' own account; used only where `/etc/passwd` and `/etc/group` both have it (FINDINGS 41, 41a).
    pub vendor_account: String,
    /// The command line the vendor daemons see (Android's slot), bind-mounted onto the chroot's `/proc/cmdline`.
    pub android_cmdline: PathBuf,
    /// How long the vendor stage waits for the modem stack (`/dev/modem` or `/dev/pmsys`).
    pub modem_stack_wait: Duration,
    /// How long a vendor daemon waits for the chroot's `/dev` to be mounted.
    pub chroot_wait: Duration,

    /// What the initramfs publishes: `linux-slot`, `misc-dev`, `misc-bc-*.bin` (and the boot timeline).
    pub initramfs_run: PathBuf,
    /// The default boot (`linux` keeps Linux armed); the initramfs reads it too.
    pub default_boot: PathBuf,
    /// Where the Linux disk's `.f50/` (boot-attempts, boot-tries, boot-lock) is looked for, in order.
    pub disk_dirs: Vec<PathBuf>,
    /// Failed boots in a row before Android when `.f50/boot-attempts` says nothing usable.
    pub default_attempts: u8,
    /// How long a change of misc waits for another one (the lock) before giving up.
    pub boot_lock_wait: Duration,
    /// The boot timeline (`t=<uptime> words`, one line per mark).
    pub timeline: PathBuf,
    /// The kernel command line sources, in order (the mainline kernel replaces `/proc/cmdline`; LK's bootargs stay in
    /// the device tree).
    pub cmdline_sources: Vec<PathBuf>,

    /// The kernel's module helper f50d installs: a link to the f50d binary that runs the guard (FINDINGS 163).
    pub modprobe_helper: PathBuf,
    /// The real modprobe the guard hands requests on to.
    pub modprobe: PathBuf,
    /// Any of these existing turns the guard off.
    pub autoload_allow: Vec<PathBuf>,
    /// How long the IPA delegate waits for the packet domain before refusing (FINDINGS 13f).
    pub dele_wait: Duration,

    /// How long after the post stage starts the boot counts as good (the shell's `sleep 30`).
    pub post_delay: Duration,
    /// How long the early recorder keeps writing (preinit started it with 420 s).
    pub recorder_window: Duration,
    /// Where the early recorder writes in the Linux slot's boot partition: this many bytes in (48 MiB) ...
    pub recorder_offset: u64,
    /// ... and at most this many (8 MiB).
    pub recorder_max: u64,
    /// The Wi-Fi reopen's lock (one reopen at a time across processes).
    pub wcn_lock: PathBuf,
}

impl Default for HwConfig {
    fn default() -> Self {
        HwConfig {
            vendor_root: "/opt/f50/android".into(),
            logdw: "/opt/f50/bin/logdw".into(),
            vendor_log: "/var/log/android-logcat.txt".into(),
            vendor_account: "f50vnd".into(),
            android_cmdline: "/run/f50-cmdline.android".into(),
            modem_stack_wait: Duration::from_secs(90),
            chroot_wait: Duration::from_secs(60),
            initramfs_run: "/run/f50".into(),
            default_boot: "/etc/f50/default-boot".into(),
            disk_dirs: vec!["/mnt/f50-disk".into(), "/".into()],
            default_attempts: 5,
            boot_lock_wait: Duration::from_secs(60),
            timeline: "/run/f50/boot-timeline".into(),
            cmdline_sources: vec!["/proc/cmdline".into(), "/proc/device-tree/chosen/bootargs".into()],
            modprobe_helper: "/usr/libexec/f50d/modprobe-guard".into(),
            modprobe: "/sbin/modprobe".into(),
            autoload_allow: vec!["/etc/f50d/autoload-allow-all".into(), "/etc/f50/autoload-allow-all".into()],
            dele_wait: Duration::from_secs(180),
            post_delay: Duration::from_secs(30),
            recorder_window: Duration::from_secs(420),
            recorder_offset: 12288 * 4096,
            recorder_max: 2048 * 4096,
            wcn_lock: "/run/f50d-wcn-reopen.lock".into(),
        }
    }
}

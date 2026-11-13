//! What the updater is configured with ([`UpdateConfig`], defaults for the F50 on OpenWrt) and where it keeps
//! things ([`Layout`]: every path, under the platform's root so tests can move it).
//!
//! The Linux partition (`Paths::disk`) holds each system as a directory (`openwrt`, `openwrt-luci`), the copy kept
//! for a rollback beside it (`<os>.old`), what a rollback left (`<os>.broken`), the boot step's state (`boot/`,
//! shared with the boot image's init, which settles `boot/modules.pending` itself: docs/research/FINDINGS.md 177)
//! and the staging directory of a run.

use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::paths::Paths;

/// Settings of the updater; `Default` is the F50 running OpenWrt.
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    /// The GitHub repository releases come from (`owner/name`).
    pub repo: String,
    /// GitHub's API for that repository (`.../repos/owner/name`: `releases/latest` and `releases` below it).
    pub api_url: String,
    /// Where a release's files are (`.../releases/download`: `TAG/FILE` below it).
    pub download_url: String,
    /// The systems this updater may update (directories of that name on the Linux partition).
    pub systems: Vec<String>,
    /// The device name a kernel bundle must list in its `./devices` (a bundle without the file is the F50's).
    pub device: String,
    /// The project's release key (minisign/usign Ed25519 public key), an absolute device path: a file of the
    /// installed image, never one a release brings (FINDINGS 86).
    pub release_key: PathBuf,
    /// Another place for the release files (a mirror), an absolute device path: `NAME=URL` lines, root's own file.
    pub source_file: PathBuf,
    /// How often the background check for a newer release runs (0: never).
    pub notice_interval: Duration,
    /// The first check after the start (the network is rarely up at once).
    pub notice_delay: Duration,
    /// Largest offline upload accepted, in bytes.
    pub upload_max: u64,
    /// Where uhttpd's cgi-io keeps an upload until it is complete (it needs the room too).
    pub upload_tmp: PathBuf,
    /// The command that arms the Linux slot for a trial boot after a new boot image: program (absolute device path)
    /// and arguments. Exit 0 armed, 3 the default boot is Android (no trial). f50d's own `boot trial` by default.
    pub trial_command: Vec<String>,
    /// Locks of other updaters that may still be installed (the shell `mu300-update`): taken too, when present.
    pub legacy_locks: Vec<PathBuf>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        let repo = "dikeckaan/mu300-linux".to_owned();
        UpdateConfig {
            api_url: format!("https://api.github.com/repos/{repo}"),
            download_url: format!("https://github.com/{repo}/releases/download"),
            repo,
            systems: vec!["openwrt".into(), "openwrt-luci".into()],
            device: "f50".into(),
            release_key: "/opt/f50/lib/release.pub".into(),
            source_file: "/etc/f50d/update-source".into(),
            notice_interval: Duration::from_secs(6 * 3600),
            notice_delay: Duration::from_secs(300),
            upload_max: 2047 << 20,
            upload_tmp: "/tmp".into(),
            trial_command: vec!["/usr/sbin/f50d".into(), "boot".into(), "trial".into()],
            legacy_locks: vec!["/run/mu300-update/lock".into()],
        }
    }
}

/// The kernels a release carries, and the retired ones with the kernel that takes their place.
pub const KERNELS: &[&str] = &["5.4", "6.18", "7.3"];
pub const KERNELS_RETIRED: &[(&str, &str)] = &[("7.2", "7.3")];

/// Every path the updater uses.
#[derive(Debug, Clone)]
pub struct Layout {
    pub paths: Paths,
    /// The Linux partition.
    pub disk: PathBuf,
    /// The staging directory of a run (root's alone, removed when a run ends well).
    pub stage: PathBuf,
    /// The record of the step that must finish even if the daemon dies (see `apply::journal`).
    pub journal: PathBuf,
    /// The boot step's state: `installed.tag`, `prev.body`, `modules.pending`, `kernel` (the choice), ...
    pub boot: PathBuf,
    /// The upload directory (root's alone) and its two names.
    pub upload_dir: PathBuf,
    pub upload: PathBuf,
    pub staged: PathBuf,
    /// Runtime files (RAM): the lock, the cached release list, the notice, what an upload was found to be.
    pub lock_dir: PathBuf,
    pub releases: PathBuf,
    pub notice: PathBuf,
    pub inspect: PathBuf,
    pub recovery: PathBuf,
}

impl Layout {
    pub fn new(paths: &Paths) -> Self {
        let disk = paths.disk();
        let run = paths.run();
        let upload_dir = disk.join(".f50-upload");
        Layout {
            paths: paths.clone(),
            stage: disk.join(".f50-update"),
            journal: disk.join(".f50-update.journal"),
            boot: disk.join("boot"),
            upload: upload_dir.join("update.tar"),
            staged: upload_dir.join("staged.tar"),
            upload_dir,
            lock_dir: run.join("update-lock"),
            releases: run.join("update-releases.json"),
            notice: run.join("update-notice.json"),
            inspect: run.join("update-inspect.json"),
            recovery: run.join("update-recovery.json"),
            disk,
        }
    }

    pub fn system(&self, os: &str) -> PathBuf {
        self.disk.join(os)
    }

    /// `<os>.old`, `<os>.new`, `<os>.broken`.
    pub fn sibling(&self, os: &str, suffix: &str) -> PathBuf {
        self.disk.join(format!("{os}.{suffix}"))
    }

    pub fn boot_file(&self, name: &str) -> PathBuf {
        self.boot.join(name)
    }

    /// An absolute device path under the root.
    pub fn sys(&self, absolute: impl AsRef<Path>) -> PathBuf {
        self.paths.sys(absolute)
    }

    /// The kernel this device runs: what `boot/kernel` says, a retired one's successor, else 5.4.
    pub fn kernel_choice(&self) -> &'static str {
        let chosen = f50_core::fsx::read_line(&self.boot_file("kernel")).unwrap_or_default();
        if let Some(k) = KERNELS.iter().find(|k| **k == chosen) {
            return k;
        }
        KERNELS_RETIRED.iter().find(|(old, _)| *old == chosen).map_or("5.4", |(_, new)| new)
    }
}

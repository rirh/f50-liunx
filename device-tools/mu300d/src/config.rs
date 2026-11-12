//! The daemon's configuration: environment variables, then command-line flags (`serve` only).
//!
//! Every path the daemon touches comes from here, so tests point it at a fake bin directory and a scratch system
//! root. Nothing has a development fallback: an unset variable means the device's own path.

use std::env;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    /// Prefix for every system path (`/etc/...`, `/proc/...`, `/run/...`): empty on the device. `MU300_SYSROOT`.
    pub sysroot: PathBuf,
    /// The device tools. `MU300_BIN`, default /opt/mu300/bin.
    pub bin: PathBuf,
    /// The daemon's runtime directory (jobs, the AT-collector cache). `MU300D_RUN`, default /run/mu300d.
    pub run_dir: PathBuf,
    /// PATH for every child process. `MU300D_PATH`.
    pub child_path: OsString,
    /// The control panel's adapters (lock, cell, device-usb). `MU300_UNISOC_DIR`, default
    /// SYSROOT/usr/libexec/unisoc-modem.
    pub unisoc_dir: PathBuf,
    /// The adapters' runtime directory (cell.json, sig.json, lock.json, at-history). `MU300_DASH_DIR`, default
    /// SYSROOT/var/run/unisoc-modem.
    pub dash_dir: PathBuf,
    /// The data partition (shown in status.get's storage). `MU300_DISK`, default /mnt/mu300-disk.
    pub disk: PathBuf,
    /// This program, for the detached job runner and the AT collector. `MU300D_SELF`, default the running binary.
    pub self_exe: PathBuf,
    /// Addresses `serve` listens on. `MU300D_LISTEN` (comma-separated), default 127.0.0.1:8300; `--listen`.
    pub listen: Vec<SocketAddr>,
    /// Also listen on the LAN address (port of the first listen address). `--lan`.
    pub lan: bool,
    /// Read methods without a session over HTTP. `MU300D_OPEN_READS=1`, `--open-reads`. Off by default.
    pub open_reads: bool,
    /// A session ends after this long unused. `MU300D_SESSION_IDLE` (seconds), default 1800.
    pub session_idle: Duration,
    /// `call` lets a non-root caller run write methods (tests only). `MU300D_NOROOT=1`.
    pub noroot: bool,
    /// The password checker (device-tools/pwcheck). `MU300D_PWCHECK`, default BIN/mu300-pwcheck.
    pub pwcheck: PathBuf,
    /// Override of the radio collector program. `MU300_CELL_BIN`, default UNISOC_DIR/cell.
    pub cell_bin: PathBuf,
    /// Override of the lock adapter. `MU300_LOCK_BIN`, default UNISOC_DIR/lock.
    pub lock_bin: PathBuf,
}

fn var(k: &str) -> Option<String> {
    env::var(k).ok().filter(|v| !v.is_empty())
}

fn flag(k: &str) -> bool {
    matches!(var(k).as_deref(), Some("1" | "true" | "yes"))
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let sysroot = PathBuf::from(var("MU300_SYSROOT").unwrap_or_default());
        let bin = PathBuf::from(var("MU300_BIN").unwrap_or_else(|| "/opt/mu300/bin".into()));
        let unisoc_dir = var("MU300_UNISOC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| join_sys(&sysroot, "/usr/libexec/unisoc-modem"));
        let dash_dir =
            var("MU300_DASH_DIR").map(PathBuf::from).unwrap_or_else(|| join_sys(&sysroot, "/var/run/unisoc-modem"));
        let child_path = var("MU300D_PATH").map(OsString::from).unwrap_or_else(|| {
            OsString::from(format!("/usr/sbin:/usr/bin:/sbin:/bin:{}:/opt/mu300/busybox-bin", bin.display()))
        });
        let listen = match var("MU300D_LISTEN") {
            Some(l) => parse_listen(&l)?,
            None => vec![SocketAddr::from(([127, 0, 0, 1], 8300))],
        };
        let idle = match var("MU300D_SESSION_IDLE") {
            Some(s) => s.parse::<u64>().map_err(|_| format!("MU300D_SESSION_IDLE: not a number: {s}"))?,
            None => 1800,
        };
        let self_exe = match var("MU300D_SELF") {
            Some(p) => PathBuf::from(p),
            None => env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?,
        };
        Ok(Config {
            pwcheck: var("MU300D_PWCHECK").map(PathBuf::from).unwrap_or_else(|| bin.join("mu300-pwcheck")),
            cell_bin: var("MU300_CELL_BIN").map(PathBuf::from).unwrap_or_else(|| unisoc_dir.join("cell")),
            lock_bin: var("MU300_LOCK_BIN").map(PathBuf::from).unwrap_or_else(|| unisoc_dir.join("lock")),
            run_dir: PathBuf::from(var("MU300D_RUN").unwrap_or_else(|| "/run/mu300d".into())),
            disk: PathBuf::from(var("MU300_DISK").unwrap_or_else(|| "/mnt/mu300-disk".into())),
            open_reads: flag("MU300D_OPEN_READS"),
            noroot: flag("MU300D_NOROOT"),
            session_idle: Duration::from_secs(idle.max(60)),
            lan: false,
            sysroot,
            bin,
            unisoc_dir,
            dash_dir,
            child_path,
            self_exe,
            listen,
        })
    }

    /// A system path under the system root.
    pub fn sys(&self, abs: &str) -> PathBuf {
        join_sys(&self.sysroot, abs)
    }

    /// A device tool in BIN.
    pub fn tool(&self, name: &str) -> PathBuf {
        self.bin.join(name)
    }

    pub fn jobs_dir(&self) -> PathBuf {
        self.run_dir.join("jobs")
    }
}

fn join_sys(root: &Path, abs: &str) -> PathBuf {
    if root.as_os_str().is_empty() { PathBuf::from(abs) } else { root.join(abs.trim_start_matches('/')) }
}

pub fn parse_listen(s: &str) -> Result<Vec<SocketAddr>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<SocketAddr>().map_err(|_| format!("not an address:port: {p}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sys_paths() {
        assert_eq!(join_sys(Path::new(""), "/etc/x"), PathBuf::from("/etc/x"));
        assert_eq!(join_sys(Path::new("/tmp/r"), "/etc/x"), PathBuf::from("/tmp/r/etc/x"));
    }

    #[test]
    fn listen_list() {
        assert_eq!(parse_listen("127.0.0.1:1, 192.168.77.1:2").unwrap().len(), 2);
        assert!(parse_listen("nope").is_err());
    }
}

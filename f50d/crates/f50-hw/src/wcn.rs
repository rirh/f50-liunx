//! The Wi-Fi chip's (SC2355, WCN) boot-time rules and its recovery after a firmware assert.
//!
//! - [`load_regdb`] (`regdb-load`): cfg80211 asks for `regulatory.db` before the rootfs is mounted; the direct load
//!   fails and the request waits 60 s in the sysfs firmware fallback, blocking `iw reg reload`. Pending requests are
//!   answered through `/sys/class/firmware`, then the database reloaded. A kernel without that fallback (mainline) has
//!   no `/sys/class/firmware`: nothing is waited for (it cost 6 s of every boot).
//! - The country goes in before the phy appears, so netifd's own start of hostapd already runs under it.
//! - [`reopen`] (`hotplug.d/misc/10-mu300-wcn`, issue #94): after `sprd_wlan_combo`'s recovery the chip is off until
//!   Wi-Fi is opened again; one `wifi down; wifi up`. One reopen at a time: a reset can send FW_ERROR more than once,
//!   and two overlapping reopens race each other through netifd. If Wi-Fi does not come back, the driver's watchdog
//!   reboots the device about a minute later. Based on kanoqwq/mu300-linux@23361ce.

use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};
use serde::Serialize;

/// A `misc` hotplug event, as procd passes it (`ACTION`, `SOURCE`, `EVENT`).
#[derive(Debug, Clone, Default)]
pub struct WcnEvent {
    pub action: String,
    pub source: String,
    pub event: String,
}

/// What a hotplug event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WcnOutcome {
    /// Not the Wi-Fi firmware's error.
    Ignored,
    /// Another reopen is running; it reopens the chip that is there by then.
    AlreadyRunning,
    Reopened,
}

impl WcnEvent {
    fn is_fw_error(&self) -> bool {
        self.action == "change" && self.source == "unisocwl" && self.event == "FW_ERROR"
    }
}

/// The cross-process lock: a directory naming its holder; one left by a dead holder is taken over.
struct Lock(PathBuf);

impl Lock {
    fn take(dir: &Path, proc_dir: &Path) -> Option<Lock> {
        if std::fs::create_dir(dir).is_err() {
            let holder = std::fs::read_to_string(dir.join("pid")).ok();
            let alive =
                holder.as_deref().map(str::trim).filter(|p| !p.is_empty()).is_some_and(|p| proc_dir.join(p).exists());
            if alive {
                return None;
            }
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir(dir).ok()?;
        }
        let _ = std::fs::write(dir.join("pid"), format!("{}\n", std::process::id()));
        Some(Lock(dir.to_path_buf()))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) async fn reopen(lock: &Path, proc_dir: &Path, ev: &WcnEvent) -> Result<WcnOutcome> {
    if !ev.is_fw_error() {
        return Ok(WcnOutcome::Ignored);
    }
    tracing::warn!("Wi-Fi firmware error, reopening Wi-Fi");
    let Some(_lock) = Lock::take(lock, proc_dir) else {
        tracing::info!("a Wi-Fi reopen is already running");
        return Ok(WcnOutcome::AlreadyRunning);
    };
    tokio::time::sleep(Duration::from_secs(1)).await;
    let wifi = |a: &str| Cmd::new("/sbin/wifi").arg(a).timeout(Duration::from_secs(60)).ok();
    let down = wifi("down").await;
    wifi("up").await.map_err(|e| e.context("wifi up"))?;
    if let Err(e) = down {
        tracing::warn!("wifi down: {e}");
    }
    Ok(WcnOutcome::Reopened)
}

/// Feeds pending regulatory database requests, then `iw reg reload` (up to 10 tries).
pub(crate) async fn load_regdb(sys: &Path, firmware_dirs: &[PathBuf]) -> Result<String> {
    let class = sys.join("class/firmware");
    let mut fed = Vec::new();
    if class.is_dir() {
        for f in ["regulatory.db", "regulatory.db.p7s"] {
            let Some(src) = firmware_dirs.iter().map(|d| d.join(f)).find(|p| p.exists()) else { continue };
            let req = class.join(f);
            for _ in 0..10 {
                if req.is_dir() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if !req.is_dir() {
                continue;
            }
            let r = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                std::fs::write(req.join("loading"), "1")?;
                std::fs::write(req.join("data"), std::fs::read(&src)?)?;
                std::fs::write(req.join("loading"), "0")
            })
            .await;
            if matches!(r, Ok(Ok(()))) {
                fed.push(f);
            }
        }
    }
    for _ in 0..10 {
        if Cmd::new("iw").args(["reg", "reload"]).ok().await.is_ok() {
            return Ok(if fed.is_empty() {
                "reloaded".into()
            } else {
                format!("fed {} to the pending firmware request; reloaded", fed.join(", "))
            });
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(Error::failed("iw reg reload failed"))
}

/// The configured country, before the phy appears (5 GHz channels are NO-IR under the world rules).
pub(crate) async fn set_country(country: &str) -> Result<String> {
    if !(country.len() == 2 && country.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())) {
        return Err(Error::bad_value("country"));
    }
    Cmd::new("iw").args(["reg", "set", country]).ok().await.map(|_| format!("country {country}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_reopen_at_a_time_and_a_dead_holder_does_not_block() {
        let d = std::env::temp_dir().join(format!("f50-hw-wcn-{}", std::process::id()));
        let (lock, proc_dir) = (d.join("lock"), d.join("proc"));
        std::fs::create_dir_all(&proc_dir).expect("mkdir");
        let first = Lock::take(&lock, &proc_dir);
        assert!(first.is_some());
        std::fs::create_dir_all(proc_dir.join(std::process::id().to_string())).expect("alive");
        assert!(Lock::take(&lock, &proc_dir).is_none());
        std::mem::forget(first);
        let _ = std::fs::remove_dir_all(&proc_dir);
        assert!(Lock::take(&lock, &proc_dir).is_some(), "the holder is gone: taken over");
        assert!(!WcnEvent { action: "add".into(), source: "unisocwl".into(), event: "FW_ERROR".into() }.is_fw_error());
        let _ = std::fs::remove_dir_all(&d);
    }
}

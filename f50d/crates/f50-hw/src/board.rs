//! `/etc/board.json` is JSON, or made again before the wifi-scripts read it (`openwrt-board-check`, kanoqwq f6b0668d).
//!
//! Seen on a V50 after a restore: a broken board.json made the wifi-scripts' `find_phy_by_path()` fail, netifd marked
//! the radio `retry_setup_failed`, and no access point came up. `board_detect` writes it from the device tree; the
//! wireless settings live in `/etc/config/wireless` and are not touched. A valid file is left as it is. A broken one
//! is kept beside it as `board.json.corrupt.<pid>`, and the new one moved into place only once it is valid itself.
//! (`board.d/05_mu300_network`, which makes usb0 the LAN port, stays a file of the image: board_detect runs it.)

use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};

const MAX: u64 = 1024 * 1024;

/// A JSON object of at most 1 MiB.
fn valid(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX)
        && std::fs::read(p)
            .ok()
            .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
            .is_some_and(|v| v.is_object())
}

/// `Ok(true)`: made again; `Ok(false)`: valid already.
pub(crate) async fn check(config: &Path, detector: &Path) -> Result<bool> {
    if valid(config) {
        return Ok(false);
    }
    if std::fs::symlink_metadata(config).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Error::failed("board.json is a symlink: not replacing it"));
    }
    if !f50_core::exec::executable(detector) {
        return Err(Error::failed("board.json is not valid and there is no board_detect to make it again"));
    }
    let pid = std::process::id();
    let temp = config.with_file_name(format!("board.json.rebuild.{pid}"));
    if temp.exists() {
        return Err(Error::busy("a rebuild of board.json is in progress"));
    }
    let r = async {
        Cmd::new(detector).arg(&temp).timeout(Duration::from_secs(30)).ok().await?;
        if !valid(&temp) {
            return Err(Error::failed("board_detect gave no valid JSON: board.json left as it was"));
        }
        let io = |what: &str, e: std::io::Error| Error::io(what, &e);
        if config.is_file() {
            // the broken one kept as evidence, never over an earlier one
            let backup = config.with_file_name(format!("board.json.corrupt.{pid}"));
            if !backup.exists() {
                std::fs::copy(config, &backup).map_err(|e| io("keep the broken board.json", e))?;
            }
        }
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o644)).map_err(|e| io("chmod", e))?;
        std::fs::rename(&temp, config).map_err(|e| io("move board.json into place", e))?;
        if let Some(dir) = config.parent()
            && let Ok(d) = std::fs::File::open(dir)
        {
            let _ = d.sync_all();
        }
        Ok(true)
    }
    .await;
    let _ = std::fs::remove_file(&temp);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_object_is_valid() {
        let d = std::env::temp_dir().join(format!("f50-hw-board-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("mkdir");
        let f = d.join("board.json");
        std::fs::write(&f, "{\"model\":{}}").expect("w");
        assert!(valid(&f));
        std::fs::write(&f, "[1]").expect("w");
        assert!(!valid(&f));
        std::fs::write(&f, "{\"model\":").expect("w");
        assert!(!valid(&f));
        let _ = std::fs::remove_dir_all(&d);
    }
}

//! Which device this is, and the facts that do not change while it runs: model, release, kernel.
//!
//! f50d runs on the ZTE F50 only. The rules are mu300-device's: the boot's label (`/run/mu300/device`) when it says
//! `u30air`, else the device tree, where only the U30 Air has `charger_policy_service` and only the ZTE V50 has the
//! AW9523B LED controller at 0x5b on I2C3 (docs/research/FINDINGS.md 95). Either of those is refused with a clear
//! message; anything else (a tree without either node, or none at all) is an F50, as mu300-device answered.

use std::path::Path;

use f50_core::{Error, Result};
use f50_platform::Platform;

/// The one device f50d supports, as the API names it.
pub(crate) const DEVICE: &str = "f50";

/// Another ZTE board this repository once supported, told apart from the F50.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Other {
    U30Air,
    V50,
}

/// `Ok` on an F50; `unsupported` on a U30 Air or a V50.
pub(crate) fn require_f50(platform: &Platform) -> Result<()> {
    match detect(platform.paths().root()) {
        None => Ok(()),
        Some(Other::U30Air) => Err(Error::unsupported("this is a ZTE U30 Air; f50d runs on the ZTE F50 only")),
        Some(Other::V50) => Err(Error::unsupported("this is a ZTE V50; f50d runs on the ZTE F50 only")),
    }
}

/// The other board this is, or `None` for an F50.
pub(crate) fn detect(root: &Path) -> Option<Other> {
    let dt = root.join("proc/device-tree");
    let label = f50_core::fsx::read_line(&root.join("run/mu300/device")).unwrap_or_default();
    if label == "u30air" || dt.join("charger_policy_service").is_dir() {
        return Some(Other::U30Air);
    }
    v50_node(&dt).then_some(Other::V50)
}

/// The V50's AW9523B node; its parent is `soc:ap-ahb` or `ap-ahb`, by kernel.
fn v50_node(dt: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dt.join("soc")) else { return false };
    rd.flatten().any(|e| e.path().join("i2c@2270000/aw9523b_led@5b").is_dir())
}

/// The model string: OpenWrt's board detection, else the device tree's (NUL-terminated).
pub(crate) fn model(platform: &Platform) -> Option<String> {
    let sys = platform.sysfs();
    sys.read("/tmp/sysinfo/model").filter(|m| !m.is_empty()).or_else(|| {
        let raw = std::fs::read(sys.path("/proc/device-tree/model")).ok()?;
        let s = String::from_utf8_lossy(&raw).trim_end_matches('\0').trim().to_owned();
        (!s.is_empty()).then_some(s)
    })
}

/// The image's release: f50d's own note (`/etc/f50d/release`), else OpenWrt's `DISTRIB_DESCRIPTION`.
pub(crate) fn release(platform: &Platform) -> Option<String> {
    let own = f50_core::fsx::read_line(&platform.paths().state().join("release")).filter(|r| !r.is_empty());
    own.or_else(|| {
        let text = std::fs::read_to_string(platform.paths().sys("/etc/openwrt_release")).ok()?;
        distrib_description(&text)
    })
}

/// `DISTRIB_DESCRIPTION='OpenWrt 25.12.0 r1234'` -> `OpenWrt 25.12.0 r1234`.
pub(crate) fn distrib_description(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix("DISTRIB_DESCRIPTION=")?;
        let v = v.trim().trim_matches(|c| c == '\'' || c == '"');
        (!v.is_empty()).then(|| v.to_owned())
    })
}

/// The running kernel's release.
pub(crate) fn kernel(platform: &Platform) -> Option<String> {
    platform.sysfs().read("/proc/sys/kernel/osrelease").filter(|k| !k.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("f50-system-dev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmp");
        d
    }

    #[test]
    fn f50_unless_the_tree_says_otherwise() {
        let r = tmp("f50");
        assert_eq!(detect(&r), None);
        std::fs::create_dir_all(r.join("proc/device-tree/soc/ap-ahb/i2c@2270000/aw9523b_led@5b")).expect("dt");
        assert_eq!(detect(&r), Some(Other::V50));
        std::fs::create_dir_all(r.join("proc/device-tree/charger_policy_service")).expect("dt");
        assert_eq!(detect(&r), Some(Other::U30Air));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn openwrt_release() {
        let t = "DISTRIB_ID='OpenWrt'\nDISTRIB_DESCRIPTION='OpenWrt 25.12.0 r1'\n";
        assert_eq!(distrib_description(t).as_deref(), Some("OpenWrt 25.12.0 r1"));
        assert_eq!(distrib_description("DISTRIB_DESCRIPTION=''"), None);
    }
}

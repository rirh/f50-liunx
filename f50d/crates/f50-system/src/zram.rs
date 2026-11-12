//! Compressed swap in RAM (zram), set up at start unless switched off (docs/research/FINDINGS.md 162).
//!
//! Settings (all optional) in `/etc/f50d/zram.conf`, the shell's keys: `ZRAM_ENABLE=1`, `ZRAM_SIZE_PERCENT=50` (of
//! MemTotal: ~700 MiB of swap on the F50's ~1.4 GiB, which costs nothing while empty), `ZRAM_ALGORITHM=lz4` (zstd
//! where the kernel offers it; one it lacks falls back to lz4, else the kernel's default), `ZRAM_PRIORITY=100`,
//! `ZRAM_SWAPPINESS=100`. A change of size or compressor takes effect at the next boot. busybox's `swapon` has no
//! `--show` (FINDINGS 162), so "already on" is read from /proc/swaps.

use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};
use f50_platform::Platform;
use serde_json::{Value, json};

use crate::settings::{self, ConfFile};

const ZRAM: &str = "/sys/block/zram0";
const DEV: &str = "/dev/zram0";

/// The settings, each checked and clamped as the shell did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZramConf {
    pub enable: bool,
    pub size_percent: u64,
    pub algorithm: String,
    pub priority: u32,
    pub swappiness: u32,
}

/// A number in `min..=max`, else `default`.
fn num(v: Option<&str>, min: u64, max: u64, default: u64) -> u64 {
    v.and_then(|v| v.parse().ok()).filter(|n| (min..=max).contains(n)).unwrap_or(default)
}

impl ZramConf {
    pub(crate) fn parse(text: &str) -> Self {
        // digits, letters, - and _ only, as the shell's sed took them
        let word = |k: &str| {
            settings::get(text, k)
                .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        };
        ZramConf {
            enable: word("ZRAM_ENABLE") != Some("0"),
            size_percent: num(word("ZRAM_SIZE_PERCENT"), 1, 200, 50),
            algorithm: word("ZRAM_ALGORITHM").unwrap_or("lz4").to_owned(),
            priority: u32::try_from(num(word("ZRAM_PRIORITY"), 0, 32_767, 100)).unwrap_or(100),
            swappiness: u32::try_from(num(word("ZRAM_SWAPPINESS"), 0, 200, 100)).unwrap_or(100),
        }
    }
}

/// `comp_algorithm`'s words, and the chosen one (in brackets).
pub(crate) fn algorithms(text: &str) -> (Vec<String>, Option<String>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let all = words.iter().map(|w| w.trim_matches(|c| c == '[' || c == ']').to_owned()).collect();
    let cur = words
        .iter()
        .find(|w| w.starts_with('['))
        .or(if words.len() == 1 { words.first() } else { None })
        .map(|w| w.trim_matches(|c| c == '[' || c == ']').to_owned());
    (all, cur)
}

fn active(platform: &Platform) -> bool {
    std::fs::read_to_string(platform.sysfs().path("/proc/swaps"))
        .unwrap_or_default()
        .lines()
        .any(|l| l.split_whitespace().next() == Some(DEV))
}

fn vm_tune(platform: &Platform, swappiness: u32) {
    let sys = platform.sysfs();
    // 5.4 takes at most 100 (200 since 5.8)
    if sys.write("/proc/sys/vm/swappiness", &swappiness.to_string()).is_err() {
        let _ = sys.write("/proc/sys/vm/swappiness", "100");
    }
    // one page at a time: read-ahead from a RAM device only costs
    let _ = sys.write("/proc/sys/vm/page-cluster", "0");
}

/// zram-start: on unless switched off; already on is only tuned.
pub(crate) async fn start(platform: &Platform) -> Result<()> {
    let c = ZramConf::parse(&ConfFile::new(platform, "zram.conf").read());
    if !c.enable {
        tracing::info!("zram: switched off in zram.conf");
        return Ok(());
    }
    if active(platform) {
        vm_tune(platform, c.swappiness);
        return Ok(());
    }
    let sys = platform.sysfs();
    if !sys.path(ZRAM).exists() {
        let _ = platform.modules().load("zram", &["num_devices=1"]).await;
    }
    if !sys.path(ZRAM).exists() {
        return Err(Error::unsupported("zram in this kernel"));
    }
    let mem: u64 = crate::facts::memory(platform).total_kb.ok_or_else(|| Error::failed("cannot read MemTotal"))?;
    let size_kb = mem * c.size_percent / 100;
    let _ = sys.write(format!("{ZRAM}/reset"), "1");
    // the compressor before the size
    let (offered, _) = algorithms(&sys.read(format!("{ZRAM}/comp_algorithm")).unwrap_or_default());
    for a in [c.algorithm.as_str(), "lz4"] {
        if offered.iter().any(|o| o == a) && sys.write(format!("{ZRAM}/comp_algorithm"), a).is_ok() {
            break;
        }
    }
    let (_, cur) = algorithms(&sys.read(format!("{ZRAM}/comp_algorithm")).unwrap_or_default());
    if cur.as_deref() != Some(c.algorithm.as_str()) {
        tracing::info!(
            "zram: {} not offered by this kernel ({}), using {}",
            c.algorithm,
            offered.join(" "),
            cur.as_deref().unwrap_or("?")
        );
    }
    sys.write(format!("{ZRAM}/disksize"), &format!("{size_kb}K")).map_err(|e| e.context(format!("size {DEV}")))?;
    let dev = sys.path(DEV);
    Cmd::new("mkswap").arg(&dev).timeout(Duration::from_secs(30)).ok().await.map_err(|e| e.context("mkswap"))?;
    Cmd::new("swapon")
        .arg("-p")
        .arg(c.priority.to_string())
        .arg(&dev)
        .timeout(Duration::from_secs(30))
        .ok()
        .await
        .map_err(|e| e.context("swapon"))?;
    vm_tune(platform, c.swappiness);
    tracing::info!(
        "zram on: {} MiB ({} % of RAM), {}, priority {}",
        size_kb / 1024,
        c.size_percent,
        cur.as_deref().unwrap_or("?"),
        c.priority
    );
    Ok(())
}

/// For `f50.system info`: on or off, size, compressor, and what the swapped-out pages cost.
pub(crate) fn status(platform: &Platform) -> Value {
    let sys = platform.sysfs();
    let on = active(platform);
    let (_, alg) = algorithms(&sys.read(format!("{ZRAM}/comp_algorithm")).unwrap_or_default());
    // mm_stat: orig_data_size compr_data_size mem_used_total ... (bytes)
    let mm: Vec<u64> = sys
        .read(format!("{ZRAM}/mm_stat"))
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    let c = ZramConf::parse(&ConfFile::new(platform, "zram.conf").read());
    json!({
        "on": on,
        "enabled": c.enable,
        "size_bytes": if on { sys.read_num::<u64>(format!("{ZRAM}/disksize")) } else { None },
        "algorithm": if on { alg } else { None },
        "stored_bytes": if on { mm.first().copied() } else { None },
        "ram_used_bytes": if on { mm.get(2).copied() } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_clamped() {
        let c = ZramConf::parse("ZRAM_SIZE_PERCENT=500\nZRAM_ALGORITHM=zstd\nZRAM_PRIORITY=x\nZRAM_SWAPPINESS=150\n");
        assert_eq!(
            (c.enable, c.size_percent, c.algorithm.as_str(), c.priority, c.swappiness),
            (true, 50, "zstd", 100, 150)
        );
        assert!(!ZramConf::parse("ZRAM_ENABLE=0\n").enable);
        assert_eq!(ZramConf::parse("ZRAM_ALGORITHM=a;b\n").algorithm, "lz4");
    }

    #[test]
    fn comp_algorithm() {
        assert_eq!(
            algorithms("lzo lzo-rle [lz4] zstd"),
            (vec!["lzo".into(), "lzo-rle".into(), "lz4".into(), "zstd".into()], Some("lz4".into()))
        );
        assert_eq!(algorithms("lz4").1.as_deref(), Some("lz4"));
        assert_eq!(algorithms("").1, None);
    }
}

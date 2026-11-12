//! Live figures read from /proc and /sys: uptime, CPU time and clocks, memory, storage.
//!
//! The load average is not reported: three vendor kernel threads sit in D state and pin it around 3.5 for ever
//! (dashboard-info's note). CPU time is reported as the kernel's cumulative counters instead; a reader subtracts two
//! readings for the busy share, so its poll interval never skews the number.

use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_platform::Platform;
use serde::Serialize;

/// Seconds since boot.
pub(crate) fn uptime(platform: &Platform) -> Option<u64> {
    let line = platform.sysfs().read("/proc/uptime")?;
    let secs = line.split_whitespace().next()?;
    secs.split('.').next()?.parse().ok()
}

/// The CPUs' time since boot, in clock ticks, and their clocks.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CpuTimes {
    /// The number of CPUs.
    pub n: usize,
    /// user + nice + system + idle + iowait + irq + softirq + steal.
    pub total: u64,
    /// idle + iowait.
    pub idle: u64,
    /// One entry per cpufreq policy (cluster): current and hardware maximum, kHz.
    pub freqs: Vec<Freq>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Freq {
    pub cur: Option<u64>,
    pub max: Option<u64>,
}

/// `/proc/stat`'s first line as (total, idle), and the CPU count.
pub(crate) fn parse_stat(text: &str) -> Option<(u64, u64, usize)> {
    let mut lines = text.lines();
    let first = lines.next()?;
    let mut f = first.split_whitespace();
    if f.next()? != "cpu" {
        return None;
    }
    let v: Vec<u64> = f.take(8).map(|x| x.parse().unwrap_or(0)).collect();
    let total = v.iter().sum();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    let n = lines.filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit)).count();
    Some((total, idle, n))
}

pub(crate) fn cpu_times(platform: &Platform) -> Option<CpuTimes> {
    let text = std::fs::read_to_string(platform.sysfs().path("/proc/stat")).ok()?;
    let (total, idle, n) = parse_stat(&text)?;
    let freqs = crate::cpu::policies(platform)
        .iter()
        .map(|p| Freq {
            cur: f50_core::fsx::read_num(&p.join("scaling_cur_freq")),
            max: f50_core::fsx::read_num(&p.join("cpuinfo_max_freq")),
        })
        .collect();
    Some(CpuTimes { n, total, idle, freqs })
}

/// Memory, kB.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Memory {
    pub total_kb: Option<u64>,
    pub avail_kb: Option<u64>,
    pub swap_total_kb: Option<u64>,
    pub swap_free_kb: Option<u64>,
}

pub(crate) fn parse_meminfo(text: &str) -> Memory {
    let field = |k: &str| {
        text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix(':')?.split_whitespace().next()?.parse().ok())
    };
    Memory {
        total_kb: field("MemTotal"),
        avail_kb: field("MemAvailable"),
        swap_total_kb: field("SwapTotal"),
        swap_free_kb: field("SwapFree"),
    }
}

pub(crate) fn memory(platform: &Platform) -> Memory {
    parse_meminfo(&std::fs::read_to_string(platform.sysfs().path("/proc/meminfo")).unwrap_or_default())
}

/// One filesystem, kB.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Disk {
    pub size_kb: u64,
    pub used_kb: u64,
    pub avail_kb: u64,
}

/// `df -kP`'s second line.
pub(crate) fn parse_df(text: &str) -> Option<Disk> {
    let line = text.lines().nth(1)?;
    let f: Vec<&str> = line.split_whitespace().collect();
    Some(Disk { size_kb: f.get(1)?.parse().ok()?, used_kb: f.get(2)?.parse().ok()?, avail_kb: f.get(3)?.parse().ok()? })
}

/// The filesystem `path` is on (`None` when the path is missing or df says nothing usable). A statfs would need
/// unsafe code, which this crate keeps to its input module; busybox's df is cheap and bounded here.
pub(crate) async fn disk(path: &Path) -> Option<Disk> {
    if !path.exists() {
        return None;
    }
    let out = Cmd::new("df").arg("-kP").arg(path).timeout(Duration::from_secs(3)).max_output(4096).ok().await.ok()?;
    parse_df(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_and_meminfo() {
        let s = "cpu  10 2 30 400 5 6 7 8 0 0\ncpu0 1 1 1 1 1 1 1 1\ncpu1 1 1 1 1 1 1 1 1\nintr 1\n";
        assert_eq!(parse_stat(s), Some((468, 405, 2)));
        assert_eq!(parse_stat("intr 1\n"), None);
        let m = parse_meminfo("MemTotal:  1400000 kB\nMemFree: 1 kB\nMemAvailable:   700000 kB\n");
        assert_eq!((m.total_kb, m.avail_kb, m.swap_total_kb), (Some(1_400_000), Some(700_000), None));
    }

    #[test]
    fn df() {
        let t = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/loop0 1000 400 600 40% /\n";
        assert_eq!(parse_df(t), Some(Disk { size_kb: 1000, used_kb: 400, avail_kb: 600 }));
        assert_eq!(parse_df("header only\n"), None);
    }
}

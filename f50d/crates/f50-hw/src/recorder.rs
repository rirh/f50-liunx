//! The early boot recorder (`early-recorder`, preinit's `05_f50_early_recorder`).
//!
//! For the first minutes of a boot, the kernel log and the services' state are copied into the persistent log area
//! of the Linux slot's boot partition (48 MiB in, at most 8 MiB) every few seconds, so a boot that dies before the
//! system log is anywhere still leaves evidence `scripts/device/collect-logs.sh` reads from Android (it stops at
//! `F50-PERSIST-END`, so older bytes after it do no harm).
//!
//! Hardware lessons kept: the partition is the internal disk's, never a card's of the same GPT name (FINDINGS 31l).
//! Only what there is gets written, not the whole 8 MiB window (that was half a gigabyte of flash per boot, and I/O
//! the modem's NV server queued behind). procd replaces `/dev` once preinit is over and fills it again from the
//! kernel's events: a write in that moment once created the partition's node as a plain file in RAM, and every later
//! write went there (FINDINGS 86) - so no write while the node is missing, and a plain file in its place is replaced
//! by the node. Every 2 s for the first 90 s (where the early hangs happened), then every 30 s (FINDINGS 235). Pinned
//! to the little cores, where there is a capacity to tell them by (kanoqwq f0b5ad43).

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};

use crate::storage::Storage;
use crate::timeline::uptime_secs;

/// Where the recorder writes and what it reads.
#[derive(Debug, Clone)]
pub(crate) struct Recorder {
    pub dev: PathBuf,
    pub sys: PathBuf,
    pub proc_uptime: PathBuf,
    pub cpu_dir: PathBuf,
    pub part: String,
    pub offset: u64,
    pub max: u64,
    pub window: Duration,
    pub vendor_log: PathBuf,
    pub tmp: PathBuf,
}

/// Is `node` the block device, so a write goes to the eMMC? A plain file in its place is replaced by the node.
pub(crate) fn node_ok(node: &Path, sysdir: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(node) else { return false };
    if m.file_type().is_block_device() {
        return true;
    }
    if !m.file_type().is_file() {
        return false;
    }
    let Some(mm) = f50_core::fsx::read_line(&sysdir.join("dev")) else { return false };
    let Some((maj, min)) = mm.split_once(':') else { return false };
    let (Ok(maj), Ok(min)) = (maj.parse::<u32>(), min.parse::<u32>()) else { return false };
    let _ = std::fs::remove_file(node);
    let _ = crate::sys::mknod_block(node, 0o600, maj, min);
    std::fs::symlink_metadata(node).is_ok_and(|m| m.file_type().is_block_device())
}

/// The CPUs of the lowest `cpu_capacity`, empty when none says.
pub(crate) fn little_cpus(cpu_dir: &Path) -> Vec<usize> {
    let mut caps: Vec<(usize, u64)> = std::fs::read_dir(cpu_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let n = e.ok()?.file_name().into_string().ok()?;
            let id: usize = n.strip_prefix("cpu")?.parse().ok()?;
            let cap = f50_core::fsx::read_num(&cpu_dir.join(&n).join("cpu_capacity"))?;
            Some((id, cap))
        })
        .collect();
    caps.sort_unstable();
    let Some(min) = caps.iter().map(|c| c.1).min() else { return Vec::new() };
    caps.into_iter().filter(|c| c.1 == min).map(|c| c.0).collect()
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

async fn run(program: &str, args: &[&str], max: usize) -> String {
    Cmd::new(program)
        .args(args)
        .max_output(max)
        .timeout(Duration::from_secs(10))
        .run()
        .await
        .map(|o| o.stdout)
        .unwrap_or_default()
}

impl Recorder {
    async fn snapshot(&self, uptime: &str) -> Vec<u8> {
        let mut s = format!("F50-PERSIST-BEGIN uptime={uptime} (f50d early recorder)\n");
        s += &run("ip", &["-br", "addr"], 64 * 1024).await;
        s += "--- ps\n";
        s += &run("ps", &["w"], 256 * 1024).await;
        s += "--- logread\n";
        // logd hands over its last 300 lines, not all of them to be cut here
        let lr = run("logread", &["-l", "300"], 256 * 1024).await;
        s += &lr.lines().filter(|l| !l.contains("PR_CAPBSET")).collect::<Vec<_>>().join("\n");
        s += "\n--- android logcat\n";
        s += &tail_lines(&std::fs::read_to_string(&self.vendor_log).unwrap_or_default(), 150);
        s += "\n--- f50 logs\n";
        let mut logs: Vec<PathBuf> = std::fs::read_dir(&self.tmp)
            .into_iter()
            .flatten()
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("f50-") && n.ends_with(".log"))
            })
            .collect();
        logs.sort();
        for l in logs {
            s += &format!(
                "==> {} <==\n{}\n",
                l.display(),
                tail_lines(&std::fs::read_to_string(&l).unwrap_or_default(), 40)
            );
        }
        s += "--- dmesg\n";
        s += &run("dmesg", &["-s", "1000000"], 1_000_000).await;
        s += "\nF50-PERSIST-END\n";
        let mut b = s.into_bytes();
        b.extend_from_slice(&[0u8; 65536]);
        b
    }

    fn write(&self, node: &Path, data: &[u8]) -> std::io::Result<()> {
        let n = data.len().min(usize::try_from(self.max).unwrap_or(usize::MAX));
        let mut f = OpenOptions::new().write(true).open(node)?;
        if !f.metadata()?.file_type().is_block_device() {
            return Err(std::io::Error::other("not the block device"));
        }
        f.seek(SeekFrom::Start(self.offset))?;
        f.write_all(&data[..n])?;
        f.sync_all()
    }

    /// Writes rounds until the window is over; `Ok` names what it wrote to (or why nothing).
    pub(crate) async fn record(&self) -> Result<String> {
        let Some((part, sysdir)) = Storage::new(&self.sys).find(&self.part) else {
            return Ok(format!("no {} on the internal disk", self.part));
        };
        let node = self.dev.join(&part);
        if !std::fs::symlink_metadata(&node).is_ok_and(|m| m.file_type().is_block_device()) {
            return Ok(format!("{} is not a block device", node.display()));
        }
        let little = little_cpus(&self.cpu_dir);
        if !little.is_empty()
            && let Err(e) = crate::sys::pin_process(&little)
        {
            tracing::debug!("recorder not pinned: {e}");
        }
        let start = uptime_secs(&self.proc_uptime).unwrap_or(0.0);
        let end = start.floor() + self.window.as_secs_f64();
        let mut rounds = 0u32;
        loop {
            let now = uptime_secs(&self.proc_uptime).unwrap_or(end);
            if now.floor() >= end {
                break;
            }
            let data = self.snapshot(&crate::timeline::uptime_text(&self.proc_uptime).unwrap_or_default()).await;
            let (rec, node2, sys2) = (self.clone(), node.clone(), sysdir.clone());
            // the node checked again afterwards: /dev can be replaced between the check and the write
            let r = tokio::task::spawn_blocking(move || {
                let r = if node_ok(&node2, &sys2) { rec.write(&node2, &data) } else { Ok(()) };
                node_ok(&node2, &sys2);
                r
            })
            .await
            .map_err(|e| Error::failed(e.to_string()))?;
            if let Err(e) = r {
                tracing::warn!("early recorder: {}: {e}", node.display());
            }
            rounds += 1;
            let pause = if now < 90.0 { 2 } else { 30 };
            tokio::time::sleep(Duration::from_secs(pause)).await;
        }
        Ok(format!("{rounds} rounds into {}", node.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lowest_capacity() {
        let d = std::env::temp_dir().join(format!("f50-hw-cpus-{}", std::process::id()));
        for (c, cap) in [(0, Some(1024)), (1, Some(512)), (2, None), (3, Some(512))] {
            std::fs::create_dir_all(d.join(format!("cpu{c}"))).expect("mkdir");
            if let Some(cap) = cap {
                std::fs::write(d.join(format!("cpu{c}/cpu_capacity")), format!("{cap}\n")).expect("cap");
            }
        }
        std::fs::create_dir_all(d.join("cpufreq")).expect("mkdir");
        assert_eq!(little_cpus(&d), [1, 3]);
        let _ = std::fs::remove_dir_all(&d);
        assert!(little_cpus(&d).is_empty());
    }

    #[test]
    fn a_missing_node_is_never_made_a_file() {
        let d = std::env::temp_dir().join(format!("f50-hw-node-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sys")).expect("mkdir");
        let node = d.join("mmcblk0p38");
        assert!(!node_ok(&node, &d.join("sys")));
        assert!(!node.exists());
        // a plain file with no dev to make the node from: left alone, still not ok
        std::fs::write(&node, "x").expect("w");
        assert!(!node_ok(&node, &d.join("sys")));
        assert!(node.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}

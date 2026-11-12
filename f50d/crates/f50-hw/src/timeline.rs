//! The boot timeline (`bootmark`), the kernel's uptime, and messages for the kernel log.
//!
//! A mark is one line `t=<uptime seconds, padded to 9> <words>` appended to a file on tmpfs, in one write, so
//! concurrent marks do not tear each other's lines. It never fails: a mark that cannot be written must not stop the
//! boot step that asked for it. The kernel log gets what must survive a log daemon that is not up yet (OpenWrt starts
//! the vendor stage before logd).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// The first field of `/proc/uptime`, as the kernel wrote it ("12.34").
pub(crate) fn uptime_text(proc_uptime: &Path) -> Option<String> {
    let s = std::fs::read_to_string(proc_uptime).ok()?;
    s.split_whitespace().next().map(str::to_owned)
}

pub(crate) fn uptime_secs(proc_uptime: &Path) -> Option<f64> {
    uptime_text(proc_uptime)?.parse().ok()
}

/// The line `bootmark` writes for `words` at `uptime`.
pub(crate) fn mark_line(uptime: &str, words: &str) -> String {
    format!("t={uptime:<9} {words}\n")
}

/// Appends a mark; never fails.
pub(crate) fn mark(timeline: &Path, proc_uptime: &Path, words: &str) {
    if let Some(dir) = timeline.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let line = mark_line(&uptime_text(proc_uptime).unwrap_or_default(), words);
    if let Ok(mut f) = OpenOptions::new().append(true).create(true).open(timeline) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// One line into the kernel log (best effort), and into tracing.
pub(crate) fn kmsg(dev_kmsg: &Path, tag: &str, msg: &str) {
    tracing::info!("{tag}: {msg}");
    if let Ok(mut f) = OpenOptions::new().write(true).open(dev_kmsg) {
        let _ = f.write_all(format!("{tag}: {msg}\n").as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_is_the_shells_line() {
        assert_eq!(mark_line("12.34", "vendor-start"), "t=12.34     vendor-start\n");
        assert_eq!(mark_line("", "x"), "t=          x\n");
    }
}

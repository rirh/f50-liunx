//! Keeping the modem's log channel empty.
//!
//! The CP streams its own debug log into a SIPC "spool" channel (label `slog_nr` in the device tree). On Android
//! slogmodem reads it for ever. When nothing does, the ring fills up (257 KB were found sitting in it) and the modem
//! stops servicing SIPC altogether about ninety seconds into every boot: AT answered at 67 s and was silent by 89 s,
//! and restarting the vendor daemons did not bring it back (the shell's `modem-log-drain`).
//!
//! The driver registers the channel as a char device (`spool-<dst>-<channel>` in /proc/devices) but creates no node,
//! so the node is made here and read into nothing. It runs in the keeper because the keeper is the process that
//! never restarts: a drain that stops with f50d would let the ring fill while f50d is down.
#![allow(unsafe_code)]

#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::fs::File;
use std::io::Read;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "linux")]
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::time::Duration;

/// The spool channel's name in /proc/devices on the F50, and the node made for it.
pub const SLOG_DEVICE: &str = "spool-5-5";
pub const SLOG_NODE: &str = "/dev/slog_nr";

fn major_of(proc_devices: &str, name: &str) -> Option<u32> {
    proc_devices.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let maj = it.next()?.parse().ok()?;
        (it.next()? == name).then_some(maj)
    })
}

#[cfg(target_os = "linux")]
fn make_node(node: &Path, major: u32) -> std::io::Result<()> {
    if std::fs::symlink_metadata(node).is_ok_and(|m| m.file_type().is_char_device()) {
        return Ok(());
    }
    let _ = std::fs::remove_file(node);
    let c = CString::new(node.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
    // SAFETY: `c` is a valid NUL-terminated path; mknod only creates a device node from the arguments.
    let rc = unsafe { libc::mknod(c.as_ptr(), libc::S_IFCHR | 0o600, libc::makedev(major, 0)) };
    if rc == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// The spool channel exists only on the device's Linux kernel.
#[cfg(not(target_os = "linux"))]
fn make_node(_node: &Path, _major: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not Linux"))
}

/// Drains the channel for ever (blocking: the keeper runs it on a thread of its own). Gives up quietly when this
/// kernel has no such channel after two minutes, as the shell did.
pub fn drain(name: &str, node: &Path) {
    let mut major = None;
    for _ in 0..60 {
        major = std::fs::read_to_string("/proc/devices").ok().and_then(|d| major_of(&d, name));
        if major.is_some() {
            break;
        }
        // the modem stack may still be loading
        std::thread::sleep(Duration::from_secs(2));
    }
    let Some(major) = major else {
        tracing::info!(device = name, "no modem log channel in this kernel; nothing to drain");
        return;
    };
    if let Err(e) = make_node(node, major) {
        tracing::warn!(node = %node.display(), error = %e, "cannot make the modem log node; not draining");
        return;
    }
    let mut buf = vec![0u8; 4096];
    loop {
        // a read returns 0 when the channel closes (a modem restart): open it again
        if let Ok(mut f) = File::open(node) {
            while matches!(f.read(&mut buf), Ok(n) if n > 0) {}
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::major_of;

    #[test]
    fn finds_the_spool_major() {
        let d = "Character devices:\n  1 mem\n241 spool-5-5\n242 spool-5-6\n";
        assert_eq!(major_of(d, "spool-5-5"), Some(241));
        assert_eq!(major_of(d, "spool-9-9"), None);
    }
}

//! The kernel calls the bring-up makes directly: mounts, a block device node, write access, CPU affinity.
//!
//! Linux only; elsewhere (the host's `cargo check`) every call answers `Unsupported`. Mount points are read from
//! `/proc/self/mountinfo` (safe code).
#![allow(unsafe_code)]

use std::io;
use std::path::{Path, PathBuf};

/// The mount points of this process's namespace (`/proc/self/mountinfo`, field 5, octal escapes undone).
pub(crate) fn mount_points(mountinfo: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(mountinfo) else { return Vec::new() };
    text.lines().filter_map(|l| l.split(' ').nth(4)).map(|p| PathBuf::from(unescape(p))).collect()
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn is_mount_point(mountinfo: &Path, p: &Path) -> bool {
    mount_points(mountinfo).iter().any(|m| m == p)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    fn c(p: &Path) -> io::Result<CString> {
        CString::new(p.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }

    fn check(r: libc::c_int) -> io::Result<()> {
        if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    pub(crate) fn mount(src: &Path, target: &Path, fstype: Option<&str>, flags: libc::c_ulong) -> io::Result<()> {
        let (s, t) = (c(src)?, c(target)?);
        let ty =
            fstype.map(|f| CString::new(f).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))).transpose()?;
        let ty_ptr = ty.as_ref().map_or(std::ptr::null(), |t| t.as_ptr());
        // SAFETY: every pointer is a NUL-terminated string that outlives the call (or null for the type of a bind
        // mount, which the kernel ignores); no data argument is passed.
        check(unsafe { libc::mount(s.as_ptr(), t.as_ptr(), ty_ptr, flags, std::ptr::null()) })
    }

    pub(crate) fn mknod_block(p: &Path, mode: u32, major: u32, minor: u32) -> io::Result<()> {
        let cp = c(p)?;
        let dev = libc::makedev(major, minor);
        // SAFETY: a NUL-terminated path that outlives the call; mknod(2) touches no memory of ours.
        check(unsafe { libc::mknod(cp.as_ptr(), libc::S_IFBLK | mode, dev) })
    }

    pub(crate) fn writable(p: &Path) -> bool {
        let Ok(cp) = c(p) else { return false };
        // SAFETY: a NUL-terminated path that outlives the call; access(2) only reads it.
        unsafe { libc::access(cp.as_ptr(), libc::W_OK) == 0 }
    }

    /// Every thread of this process on `cpus` (threads made later inherit it from the one that makes them).
    pub(crate) fn pin_process(cpus: &[usize]) -> io::Result<()> {
        // SAFETY: cpu_set_t is plain data; zeroed is the empty set, and CPU_SET writes only inside it.
        let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
        for &cpu in cpus {
            // SAFETY: as above; CPU_SET ignores a cpu beyond the set's size.
            unsafe { libc::CPU_SET(cpu, &mut set) };
        }
        let mut last = Ok(());
        for e in std::fs::read_dir("/proc/self/task")?.flatten() {
            let Some(tid) = e.file_name().to_str().and_then(|s| s.parse::<libc::pid_t>().ok()) else { continue };
            // SAFETY: the set is initialised and lives across the call, with its exact size.
            let r = unsafe { libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &set) };
            if r != 0 {
                last = Err(io::Error::last_os_error());
            }
        }
        last
    }

    pub(crate) const MS_BIND: libc::c_ulong = libc::MS_BIND;
    pub(crate) const MS_REC: libc::c_ulong = libc::MS_REC;
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::io;
    use std::path::Path;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Linux only")
    }

    pub(crate) fn mount(_: &Path, _: &Path, _: Option<&str>, _: u64) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn mknod_block(_: &Path, _: u32, _: u32, _: u32) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn writable(p: &Path) -> bool {
        std::fs::OpenOptions::new().write(true).open(p).is_ok()
    }

    pub(crate) fn pin_process(_: &[usize]) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) const MS_BIND: u64 = 4096;
    pub(crate) const MS_REC: u64 = 16384;
}

pub(crate) use imp::{mknod_block, pin_process, writable};

/// `mount --bind src target`.
pub(crate) fn bind(src: &Path, target: &Path) -> io::Result<()> {
    imp::mount(src, target, None, imp::MS_BIND)
}

/// `mount --rbind src target`.
pub(crate) fn rbind(src: &Path, target: &Path) -> io::Result<()> {
    imp::mount(src, target, None, imp::MS_BIND | imp::MS_REC)
}

/// `mount -t fstype fstype target` (proc, sysfs).
pub(crate) fn mount_fs(fstype: &str, target: &Path) -> io::Result<()> {
    imp::mount(Path::new(fstype), target, Some(fstype), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mountinfo_paths_are_unescaped() {
        assert_eq!(unescape(r"/mnt/a\040b"), "/mnt/a b");
        assert_eq!(unescape(r"/x\"), r"/x\");
        let f = std::env::temp_dir().join(format!("f50-hw-mi-{}", std::process::id()));
        std::fs::write(&f, "22 1 0:21 / /opt/f50/android/dev rw - devtmpfs dev rw\n").expect("write");
        assert!(is_mount_point(&f, Path::new("/opt/f50/android/dev")));
        assert!(!is_mount_point(&f, Path::new("/opt")));
        let _ = std::fs::remove_file(&f);
    }
}

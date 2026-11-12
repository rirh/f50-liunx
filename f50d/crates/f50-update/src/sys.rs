//! The few kernel calls std has no safe wrapper for: free space of a filesystem (statvfs) and an advisory lock
//! (flock). Everything else goes through std.
#![allow(unsafe_code)]

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Bytes available to us and the size of the filesystem `path` is on.
#[allow(clippy::useless_conversion, clippy::unnecessary_cast)] // the field types differ between libcs
pub fn space(path: &Path) -> io::Result<(u64, u64)> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut st = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: `c` is a valid NUL-terminated path and `st` points to writable memory of the struct's size; statvfs
    // only writes into it.
    let rc = unsafe { libc::statvfs(c.as_ptr(), st.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: statvfs returned 0, so it filled the struct.
    let st = unsafe { st.assume_init() };
    let frsize = st.f_frsize as u64;
    Ok(((st.f_bavail as u64).saturating_mul(frsize), (st.f_blocks as u64).saturating_mul(frsize)))
}

/// KiB available on the filesystem of `path` (0 when it cannot be read).
pub fn free_kb(path: &Path) -> u64 {
    space(path).map_or(0, |(free, _)| free / 1024)
}

/// An exclusive flock(2) on `file`, without waiting: `Ok(false)` when another open file description holds it. The
/// kernel releases it when the descriptor is closed, however the process ends.
pub fn try_lock(file: &File) -> io::Result<bool> {
    // SAFETY: the descriptor belongs to `file`, which outlives the call; flock touches no memory.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) { Ok(false) } else { Err(e) }
}

/// Our effective user id.
pub fn euid() -> u32 {
    // SAFETY: geteuid(2) has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

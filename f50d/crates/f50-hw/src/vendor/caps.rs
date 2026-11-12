//! Entering the vendor chroot and, for the vendor account, dropping to it with only the capabilities a daemon keeps
//! on Android - what `android-run` did with `setpriv --reuid --regid --clear-groups --no-new-privs --inh-caps
//! --ambient-caps --bounding-set ... chroot` (FINDINGS 41). `CAP_SYS_CHROOT` stays in the set as it did there (the
//! daemons' own `capset` drops it). Their own `setgroups`/`setuid(1000)` then fail; they log it and carry on.
//!
//! Called between the last allocation and `execve`, in the thread that execs (capabilities are per thread; the
//! ambient set survives the exec of a file without capabilities and becomes the daemon's effective set).
#![allow(unsafe_code)]

/// A capability set as a bit mask (`1 << CAP_*`).
pub(crate) type CapMask = u64;

pub(crate) const CAP_CHOWN: u32 = 0;
pub(crate) const CAP_SYS_CHROOT: u32 = 18;
pub(crate) const CAP_SYS_TIME: u32 = 25;
pub(crate) const CAP_BLOCK_SUSPEND: u32 = 36;

pub(crate) const fn mask(caps: &[u32]) -> CapMask {
    let mut m = 0;
    let mut i = 0;
    while i < caps.len() {
        m |= 1 << caps[i];
        i += 1;
    }
    m
}

/// Who the daemon becomes inside the chroot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drop {
    /// Stay root (cp_diskserver always; every daemon without a vendor account).
    Root,
    /// The vendor account, no other groups, no new privileges, exactly `caps`.
    Account { uid: u32, gid: u32, caps: CapMask },
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::Drop;

    const PR_SET_KEEPCAPS: libc::c_int = 8;
    const PR_CAPBSET_DROP: libc::c_int = 24;
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    const PR_CAP_AMBIENT: libc::c_int = 47;
    const PR_CAP_AMBIENT_RAISE: libc::c_ulong = 2;
    const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;

    #[repr(C)]
    struct Header {
        version: u32,
        pid: libc::c_int,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }

    fn ok(r: libc::c_int) -> io::Result<()> {
        if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    fn last_cap() -> u32 {
        std::fs::read_to_string("/proc/sys/kernel/cap_last_cap").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(40)
    }

    pub(crate) fn enter(root: &Path, drop: Drop) -> io::Result<()> {
        let r = CString::new(root.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: a NUL-terminated path that outlives the calls; chroot(2)/chdir(2) only read it.
        ok(unsafe { libc::chroot(r.as_ptr()) })?;
        // SAFETY: a static NUL-terminated string.
        ok(unsafe { libc::chdir(c"/".as_ptr()) })?;
        let Drop::Account { uid, gid, caps } = drop else { return Ok(()) };
        // the bounding set first, while CAP_SETPCAP is still effective
        for cap in 0..=last_cap() {
            if caps & (1 << cap) == 0 {
                // SAFETY: prctl(2) with integer arguments only.
                let r = unsafe { libc::prctl(PR_CAPBSET_DROP, libc::c_ulong::from(cap), 0, 0, 0) };
                if r != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        // SAFETY: setgroups(2) with an empty list reads no memory.
        ok(unsafe { libc::setgroups(0, std::ptr::null()) })?;
        // SAFETY: prctl(2) with integer arguments only.
        ok(unsafe { libc::prctl(PR_SET_KEEPCAPS, 1, 0, 0, 0) })?;
        // SAFETY: integer arguments only.
        ok(unsafe { libc::setresgid(gid, gid, gid) })?;
        // SAFETY: integer arguments only.
        ok(unsafe { libc::setresuid(uid, uid, uid) })?;
        let lo = (caps & 0xffff_ffff) as u32;
        let hi = (caps >> 32) as u32;
        let mut hdr = Header { version: LINUX_CAPABILITY_VERSION_3, pid: 0 };
        let data = [
            Data { effective: lo, permitted: lo, inheritable: lo },
            Data { effective: hi, permitted: hi, inheritable: hi },
        ];
        // SAFETY: capset(2) reads a version-3 header and two data structs, both laid out as the kernel's
        // (repr(C)) and alive across the call.
        ok(unsafe { libc::syscall(libc::SYS_capset, &mut hdr as *mut Header, data.as_ptr()) } as libc::c_int)?;
        for cap in 0..64u32 {
            if caps & (1 << cap) != 0 {
                // SAFETY: prctl(2) with integer arguments only.
                ok(unsafe { libc::prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_RAISE, libc::c_ulong::from(cap), 0, 0) })?;
            }
        }
        // SAFETY: prctl(2) with integer arguments only.
        ok(unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) })?;
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::Drop;

    pub(crate) fn enter(_: &Path, _: Drop) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Linux only"))
    }
}

/// chroot into `root`, then become what `drop` says. Only for the process that is about to exec.
pub(crate) fn enter(root: &std::path::Path, drop: Drop) -> std::io::Result<()> {
    imp::enter(root, drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sets_android_keeps() {
        // FINDINGS 41, measured: modem_control CapEff 0x1000000001, refnotify 0x2000000 (chroot's bit besides)
        assert_eq!(mask(&[CAP_CHOWN, CAP_BLOCK_SUSPEND]), 0x10_0000_0001);
        assert_eq!(mask(&[CAP_SYS_TIME]), 0x200_0000);
        assert_eq!(mask(&[CAP_SYS_CHROOT]), 1 << 18);
    }
}

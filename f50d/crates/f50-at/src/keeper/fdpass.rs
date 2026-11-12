//! Passing a descriptor over a Unix socket (SCM_RIGHTS), with `sendmsg`/`recvmsg`.
#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

const LINE_MAX: usize = 512;

fn cmsg_space() -> usize {
    // SAFETY: CMSG_SPACE only computes a size from its argument.
    unsafe { libc::CMSG_SPACE(size_of::<RawFd>() as u32) as usize }
}

/// Sends `line`, with `fd` attached when given.
pub(super) fn send(s: &UnixStream, line: &str, fd: Option<&File>) -> io::Result<()> {
    let mut data = line.as_bytes().to_vec();
    let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
    let mut ctrl = vec![0u8; cmsg_space()];
    // SAFETY: msghdr is plain data; every pointer set below points into buffers that live until sendmsg returns.
    let mut msg: libc::msghdr = unsafe { zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    if let Some(f) = fd {
        msg.msg_control = ctrl.as_mut_ptr().cast();
        msg.msg_controllen = ctrl.len() as _;
        // SAFETY: the control buffer is CMSG_SPACE(sizeof fd) bytes, so the first header and its data fit in it.
        unsafe {
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(size_of::<RawFd>() as u32) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<RawFd>(), f.as_raw_fd());
        }
    }
    // SAFETY: the socket is open and msg describes valid buffers.
    let n = unsafe { libc::sendmsg(s.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) };
    if n < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// One answer: its text and the descriptor attached to it, if any (taken with close-on-exec).
pub(super) fn recv(s: &UnixStream) -> io::Result<(String, Option<OwnedFd>)> {
    let mut data = vec![0u8; LINE_MAX];
    let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
    let mut ctrl = vec![0u8; cmsg_space()];
    // SAFETY: as in `send`; the buffers outlive recvmsg.
    let mut msg: libc::msghdr = unsafe { zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = ctrl.as_mut_ptr().cast();
    msg.msg_controllen = ctrl.len() as _;
    // the received descriptor is close-on-exec from the start on Linux (another thread may exec at any moment)
    #[cfg(target_os = "linux")]
    let flags = libc::MSG_CMSG_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = 0;
    // SAFETY: the socket is open and msg describes valid, writable buffers.
    let n = unsafe { libc::recvmsg(s.as_raw_fd(), &mut msg, flags) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut fd = None;
    // SAFETY: the kernel filled msg; CMSG_FIRSTHDR/CMSG_NXTHDR walk only inside msg_control/msg_controllen, and an
    // SCM_RIGHTS header carries at least one descriptor, which is ours to own from here on.
    unsafe {
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let raw = std::ptr::read_unaligned(libc::CMSG_DATA(c).cast::<RawFd>());
                #[cfg(not(target_os = "linux"))]
                libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC);
                fd = Some(OwnedFd::from_raw_fd(raw));
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    let len = usize::try_from(n).unwrap_or(0).min(data.len());
    Ok((String::from_utf8_lossy(&data[..len]).into_owned(), fd))
}

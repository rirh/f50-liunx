//! The kernel side of a SIPC AT channel: open, raw mode, non-blocking, and `poll(2)`.
//!
//! The only module of this crate with `unsafe` (termios, `fcntl`, `poll`). Reads and writes go through `&File`, which
//! needs none.
//!
//! - `/dev/stty_nr*` are SIPC channels, not serial ports: the termios read timer is ignored and a blocking read sits
//!   in `sbuf_read` until the modem says something, possibly for ever; `poll()` is implemented by the driver and is
//!   what bash's and busybox ash's `read -t` used successfully (docs/research/FINDINGS.md 13c). So every read and
//!   write here is gated by `poll()` with a timeout, and the descriptor is also put in non-blocking mode so that a
//!   spurious readiness can never park a thread in the driver (whether the driver honours `O_NONBLOCK` is not
//!   measured; the `poll()` gate alone keeps the measured behaviour).
//! - Raw mode is applied to the already-open descriptor, as `stty raw -echo <&3` did, never by opening the path a
//!   second time (`stty -F` is an open and a close, which takes the channel away from its owner: FINDINGS 13c, 13b).
//!   A channel that is not a tty refuses termios; that is ignored, as the shell's `|| true` did.
//! - Until the modem has started its sbuf the open fails with `ENODEV` although the node exists (FINDINGS 138).

#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// Opens a channel read-write, without making it our controlling terminal, then raw and non-blocking.
pub(crate) fn open(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_NOCTTY | libc::O_CLOEXEC).open(path)?;
    prepare(&file, path)?;
    Ok(file)
}

/// Raw and non-blocking, on a descriptor opened here or received from the keeper (both change the open file
/// description, which the keeper's copy shares: the keeper never reads, so that is what we want).
pub(crate) fn prepare(file: &File, path: &Path) -> io::Result<()> {
    if let Err(e) = make_raw(file) {
        tracing::debug!(path = %path.display(), error = %e, "no termios on the AT channel (not a tty); left as it is");
    }
    set_nonblocking(file)
}

/// `stty raw -echo` on the open descriptor.
fn make_raw(file: &File) -> io::Result<()> {
    let fd = file.as_raw_fd();
    let mut t = MaybeUninit::<libc::termios>::uninit();
    // SAFETY: `fd` is a descriptor we own for the duration of the call and `t` points to writable memory the size of
    // a termios; tcgetattr fully initialises it on success (return 0), checked before `assume_init`.
    if unsafe { libc::tcgetattr(fd, t.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: tcgetattr returned 0, so the struct is initialised.
    let mut t = unsafe { t.assume_init() };
    // SAFETY: `t` is a valid, initialised termios owned by this frame.
    unsafe { libc::cfmakeraw(&mut t) };
    t.c_cc[libc::VMIN] = 1;
    t.c_cc[libc::VTIME] = 0;
    // SAFETY: `fd` is open and `t` is a valid termios that outlives the call.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &t) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_nonblocking(file: &File) -> io::Result<()> {
    let fd = file.as_raw_fd();
    // SAFETY: F_GETFL on a descriptor we own reads its flags and has no other effect.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_SETFL with the flags just read plus O_NONBLOCK only changes this open file description's mode.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// What `poll()` said about the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ready {
    /// Nothing within the timeout (or a signal interrupted the wait).
    No,
    Yes,
    /// The other side hung up (a read then returns 0).
    HangUp,
}

/// Waits up to `timeout` for `events` (`POLLIN` or `POLLOUT`).
pub(crate) fn wait(file: &File, events: libc::c_short, timeout: Duration) -> io::Result<Ready> {
    let mut p = libc::pollfd { fd: file.as_raw_fd(), events, revents: 0 };
    let ms = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
    // SAFETY: `p` is one valid pollfd on this frame, and the count passed is 1.
    let r = unsafe { libc::poll(&mut p, 1, ms) };
    if r < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::Interrupted { Ok(Ready::No) } else { Err(e) };
    }
    if r == 0 {
        return Ok(Ready::No);
    }
    if p.revents & libc::POLLNVAL != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "poll: descriptor not open"));
    }
    if p.revents & events != 0 {
        Ok(Ready::Yes)
    } else if p.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
        Ok(Ready::HangUp)
    } else {
        Ok(Ready::No)
    }
}

/// Writes all of `data` before `deadline`, each chunk once `poll()` says the channel takes it.
pub(crate) fn write_all(file: &File, mut data: &[u8], deadline: Instant) -> io::Result<()> {
    while !data.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the AT channel did not take the command"));
        }
        match wait(file, libc::POLLOUT, left.min(Duration::from_millis(250)))? {
            Ready::No => continue,
            Ready::HangUp => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the AT channel hung up")),
            Ready::Yes => {}
        }
        match (&*file).write(data) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(n) => data = &data[n..],
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

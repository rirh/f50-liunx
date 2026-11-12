//! The kernel's input devices (`/dev/input/event*`) that have keys, read without blocking a thread.
//!
//! The only unsafe code of this crate: one `EVIOCGBIT` ioctl per device, to keep the devices that report keys
//! (mu300-keys did the same), and the fd's registration with tokio's reactor. Reading is a plain `read(2)` of whole `struct input_event`s on a non-blocking fd
//! driven by tokio's reactor, so the reader stops with its task.
#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// `EV_KEY` (`linux/input-event-codes.h`).
const EV_KEY: u16 = 0x01;
/// `struct input_event`: a `struct timeval`, then `__u16 type`, `__u16 code`, `__s32 value`.
const TIME_LEN: usize = std::mem::size_of::<libc::timeval>();
const EVENT_LEN: usize = TIME_LEN + 8;

/// `_IOC(_IOC_READ, 'E', 0x20 + ev, len)`: `EVIOCGBIT(ev, len)` (the asm-generic encoding, arm64's and x86's).
const fn eviocgbit(ev: u8, len: usize) -> u64 {
    const IOC_READ: u64 = 2;
    (IOC_READ << 30) | ((len as u64) << 16) | ((b'E' as u64) << 8) | (0x20 + ev as u64)
}

/// The device reports key events.
fn has_keys(f: &File) -> bool {
    let mut bits: libc::c_ulong = 0;
    // SAFETY: EVIOCGBIT(0, len) writes at most `len` bytes of the event-type bitmask into the buffer; `bits` is a
    // live, writable c_ulong of exactly that size, and `f` keeps the descriptor open for the call.
    let r =
        unsafe { libc::ioctl(f.as_raw_fd(), eviocgbit(0, std::mem::size_of::<libc::c_ulong>()) as _, &raw mut bits) };
    r >= 0 && (bits >> EV_KEY) & 1 == 1
}

/// One input device with keys.
#[derive(Debug)]
pub(crate) struct KeyDevice {
    fd: AsyncFd<File>,
    path: PathBuf,
    buf: Vec<u8>,
}

/// Every `event*` device under `dir` that has keys (at most 16, as mu300-keys).
pub(crate) fn open_key_devices(dir: &Path) -> Vec<KeyDevice> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("event")))
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let f = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(&path).ok()?;
            if !has_keys(&f) {
                return None;
            }
            // SAFETY: `f` is an open File moved into the AsyncFd, which owns it until it is dropped; a File's
            // as_raw_fd() always returns that same descriptor, so it stays valid and unchanged while registered.
            let fd = unsafe { AsyncFd::register_with_interest(f, Interest::READABLE) }.ok()?;
            Some(KeyDevice { fd, path, buf: vec![0; EVENT_LEN * 16] })
        })
        .take(16)
        .collect()
}

impl KeyDevice {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The next key events (code, value) the device has; an error when the device went away.
    pub(crate) async fn next_keys(&mut self) -> io::Result<Vec<(u16, i32)>> {
        loop {
            let mut guard = self.fd.readable().await?;
            let buf = &mut self.buf;
            match guard.try_io(|inner| inner.get_ref().read(buf)) {
                Ok(Ok(0)) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input device closed")),
                Ok(Ok(n)) => return Ok(parse_keys(&self.buf[..n])),
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(e)) => return Err(e),
                Err(_would_block) => {}
            }
        }
    }
}

/// The `EV_KEY` events of a read: (code, value).
pub(crate) fn parse_keys(bytes: &[u8]) -> Vec<(u16, i32)> {
    bytes
        .as_chunks::<EVENT_LEN>()
        .0
        .iter()
        .filter_map(|ev| {
            let t = u16::from_ne_bytes([ev[TIME_LEN], ev[TIME_LEN + 1]]);
            let code = u16::from_ne_bytes([ev[TIME_LEN + 2], ev[TIME_LEN + 3]]);
            let value = i32::from_ne_bytes([ev[TIME_LEN + 4], ev[TIME_LEN + 5], ev[TIME_LEN + 6], ev[TIME_LEN + 7]]);
            (t == EV_KEY).then_some((code, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(t: u16, code: u16, value: i32) -> Vec<u8> {
        let mut v = vec![0u8; TIME_LEN];
        v.extend_from_slice(&t.to_ne_bytes());
        v.extend_from_slice(&code.to_ne_bytes());
        v.extend_from_slice(&value.to_ne_bytes());
        v
    }

    #[test]
    fn only_whole_key_events() {
        let mut b = event(EV_KEY, 116, 1);
        b.extend(event(0, 0, 0)); // EV_SYN
        b.extend(event(EV_KEY, 116, 0));
        b.extend_from_slice(&[1, 2, 3]); // a torn tail
        assert_eq!(parse_keys(&b), vec![(116, 1), (116, 0)]);
        assert_eq!(eviocgbit(0, 8), 0x8008_4520);
    }
}

//! The kernel side of a web shell: a pseudo-terminal pair, a login shell on its slave side, and non-blocking I/O on
//! its master side.
//!
//! The shell is the leader of a new session with the slave as its controlling terminal, so job control, Ctrl-C and
//! window-size changes behave as they do over SSH, and closing the session signals its whole process group.

#![allow(unsafe_code)]

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::process::Stdio;

use tokio::io::unix::AsyncFd;
use tokio::process::{Child, Command};

/// The master side of the terminal, readable and writable without blocking.
#[derive(Debug)]
pub(crate) struct Master {
    fd: AsyncFd<OwnedFd>,
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 }
}

fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(r) }
}

/// A new pseudo-terminal of `cols` × `rows`: (master, slave), both close-on-exec.
fn open(cols: u16, rows: u16) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let mut ws = winsize(cols, rows);
    // SAFETY: openpty(3) writes two descriptors into the integers it is given; the name buffer and termios are null
    // (not wanted), and `ws` is a valid winsize that outlives the call.
    cvt(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), &mut ws) })?;
    // SAFETY: openpty succeeded, so both are open descriptors that nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        // SAFETY: fcntl(2) on a descriptor this function owns; FD_CLOEXEC keeps it out of every other program f50d
        // runs (the shell gets the slave as its stdio, which Command dups without the flag).
        cvt(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    }
    // SAFETY: as above; the master is read and written through the reactor, never blocking a worker thread.
    let flags = cvt(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) })?;
    // SAFETY: as above.
    cvt(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) })?;
    Ok((master, slave))
}

/// Starts `shell` as a login shell in `cwd` on a new terminal of `cols` × `rows`, with `env` as its whole environment.
pub(crate) fn spawn(shell: &str, env: &[(&str, &str)], cwd: &str, cols: u16, rows: u16) -> io::Result<(Master, Child)> {
    let (master, slave) = open(cols, rows)?;
    let mut cmd = Command::new(shell);
    // argv[0] with a leading '-': a login shell (ash and bash read /etc/profile)
    let name = shell.rsplit('/').next().unwrap_or(shell);
    cmd.arg0(format!("-{name}"));
    cmd.env_clear().envs(env.iter().copied()).current_dir(cwd);
    cmd.stdin(Stdio::from(slave.try_clone()?)).stdout(Stdio::from(slave.try_clone()?)).stderr(Stdio::from(slave));
    // SAFETY: the closure runs in the child between fork and exec and only makes async-signal-safe calls: setsid(2)
    // makes the child the leader of a new session (it is not one: fork made it), and ioctl(TIOCSCTTY) on fd 0,
    // which is the slave by then, makes that terminal the session's controlling terminal. sigprocmask/signal
    // restore what a shell expects to inherit: f50d's runtime may block or ignore signals.
    unsafe {
        cmd.pre_exec(|| {
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::sigprocmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            for s in [libc::SIGPIPE, libc::SIGINT, libc::SIGQUIT, libc::SIGTERM, libc::SIGHUP, libc::SIGCHLD] {
                libc::signal(s, libc::SIG_DFL);
            }
            cvt(libc::setsid())?;
            cvt(libc::ioctl(0, libc::TIOCSCTTY as _, 0))?;
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    // SAFETY: the OwnedFd is moved into the AsyncFd and owned by it for its whole life, so the descriptor stays open
    // (and is not reused) while it is registered.
    let fd = unsafe { AsyncFd::register(master) }?;
    Ok((Master { fd }, child))
}

/// Signals the process group `pid` leads (the shell and everything it started in its session's foreground).
pub(crate) fn signal_group(pid: u32, sig: libc::c_int) {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return };
    if pid <= 1 {
        return;
    }
    // SAFETY: kill(2) with a negative pid signals the process group the shell leads (setsid made its pgid its pid);
    // no memory is touched.
    unsafe {
        libc::kill(-pid, sig);
    }
}

impl Master {
    /// Reads what the terminal printed; `Ok(0)` once the shell's side is closed for good (Linux answers EIO then).
    pub(crate) async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.fd.readable().await?;
            let r = guard.try_io(|fd| {
                // SAFETY: read(2) into `buf`, which is valid for `buf.len()` bytes for the whole call.
                let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n.unsigned_abs()) }
            });
            match r {
                Ok(Ok(n)) => return Ok(n),
                Ok(Err(e)) if e.raw_os_error() == Some(libc::EIO) => return Ok(0),
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(e)) => return Err(e),
                Err(_would_block) => {}
            }
        }
    }

    /// Writes all of `data` as typed input.
    pub(crate) async fn write_all(&self, mut data: &[u8]) -> io::Result<()> {
        while !data.is_empty() {
            let mut guard = self.fd.writable().await?;
            let r = guard.try_io(|fd| {
                // SAFETY: write(2) from `data`, valid for `data.len()` bytes for the whole call.
                let n = unsafe { libc::write(fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
                if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n.unsigned_abs()) }
            });
            match r {
                Ok(Ok(n)) => data = &data[n..],
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(e)) => return Err(e),
                Err(_would_block) => {}
            }
        }
        Ok(())
    }

    /// Tells the terminal its new size; the kernel sends SIGWINCH to the foreground process group.
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let ws = winsize(cols, rows);
        // SAFETY: ioctl(TIOCSWINSZ) reads one winsize from `ws`, which is valid for the call.
        cvt(unsafe { libc::ioctl(self.fd.get_ref().as_raw_fd(), libc::TIOCSWINSZ as _, &ws) })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_shell_runs_on_the_terminal_and_sees_its_size() -> io::Result<()> {
        let env = [("PATH", "/usr/bin:/bin"), ("TERM", "dumb"), ("PS1", "")];
        let (m, mut child) = spawn("/bin/sh", &env, "/", 77, 21)?;
        m.write_all(b"stty size; tty -s && echo IS-TTY; exit 3\n").await?;
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        let read = async {
            loop {
                let n = m.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            io::Result::Ok(())
        };
        tokio::time::timeout(std::time::Duration::from_secs(10), read).await.map_err(io::Error::other)??;
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("21 77"), "{text}");
        assert!(text.contains("IS-TTY"), "{text}");
        assert_eq!(child.wait().await?.code(), Some(3));
        Ok(())
    }
}

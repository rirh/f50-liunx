//! The descriptor keeper: the AT channels outlive f50d.
//!
//! Closing a SIPC AT channel can take it away until the next reboot (docs/research/FINDINGS.md 13b), and every exit
//! of a process closes its descriptors: a crash of f50d, procd's respawn after it, `/etc/init.d/f50d restart`, an
//! update that replaces the binary. The shell layer avoided that by keeping the channels in a separate daemon
//! (`f50-atd`) that almost never restarted.
//!
//! f50d does the same with a much smaller holder: `f50d at-keeper` (its own procd service, started before f50d and
//! never restarted with it) opens each allowed device once and keeps it. It never reads or writes: it only hands a
//! duplicate of the descriptor to whoever asks on its socket (SCM_RIGHTS). The daemon reads and writes through its
//! duplicate; when the daemon goes, its duplicate closes, but the keeper's copy keeps the device open, so the
//! driver never sees the channel released. A daemon that starts again gets a duplicate of the very same open file.
//!
//! The recovery rule (a modem that stopped answering everywhere, FINDINGS 13b) still needs a real close and open:
//! `reopen` asks the keeper to drop its copy and open the device again.
//!
//! The keeper also drains the modem's log channel ([`slog`]) for the same reason: it must never stop.
//!
//! Protocol: one request per connection, one line: `get <path>` or `reopen <path>`; the answer is `ok` with the
//! descriptor attached, or `err <message>`. The socket is root's only, in a root-only directory, and only the
//! devices the keeper was started with are served.

mod fdpass;
pub mod slog;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::sys;

/// Where the keeper listens on the device.
pub const KEEPER_SOCKET: &str = "/var/run/f50d/at-keeper.sock";

const REQUEST_MAX: usize = 512;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

static SOCKET: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Which keeper the channels of this process use (`None`: open the devices directly). Set once, by `At::start`.
pub(crate) fn configure(socket: Option<PathBuf>) {
    let _ = SOCKET.set(socket);
}

/// A channel's descriptor: from the keeper when one is configured and running, else opened here. `fresh` asks the
/// keeper for a real close and open (the recovery rule).
pub(crate) fn open(path: &Path, fresh: bool) -> io::Result<File> {
    if let Some(Some(sock)) = SOCKET.get()
        && sock.exists()
    {
        return fetch(sock, path, fresh);
    }
    sys::open(path)
}

fn fetch(sock: &Path, path: &Path, fresh: bool) -> io::Result<File> {
    let mut s = UnixStream::connect(sock)?;
    s.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    s.set_write_timeout(Some(CLIENT_TIMEOUT))?;
    let verb = if fresh { "reopen" } else { "get" };
    writeln!(s, "{verb} {}", path.display())?;
    let (line, fd) = fdpass::recv(&s)?;
    match (line.trim_end().strip_prefix("err "), fd) {
        (Some(msg), _) => Err(io::Error::other(format!("at-keeper: {msg}"))),
        (None, Some(fd)) if line.trim_end() == "ok" => {
            let f = File::from(fd);
            sys::prepare(&f, path)?;
            Ok(f)
        }
        _ => Err(io::Error::other("at-keeper: no descriptor in the answer")),
    }
}

/// The keeper itself: serves `devices` on `socket` until the process is killed. Blocking; it is the whole program
/// of `f50d at-keeper`. Errors are only those of setting the socket up.
pub fn serve(socket: &Path, devices: &[PathBuf]) -> io::Result<()> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    let mut held: HashMap<PathBuf, File> = HashMap::new();
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let _ = conn.set_read_timeout(Some(CLIENT_TIMEOUT));
        let _ = conn.set_write_timeout(Some(CLIENT_TIMEOUT));
        let answer = handle(&conn, devices, &mut held);
        let sent = match &answer {
            Ok(f) => fdpass::send(&conn, "ok\n", Some(f)),
            Err(e) => fdpass::send(&conn, &format!("err {e}\n"), None),
        };
        if let Err(e) = sent {
            tracing::warn!(error = %e, "at-keeper: could not answer");
        }
    }
    Ok(())
}

fn handle<'a>(conn: &UnixStream, devices: &[PathBuf], held: &'a mut HashMap<PathBuf, File>) -> io::Result<&'a File> {
    let mut line = String::new();
    let mut r = BufReader::new(conn.take(REQUEST_MAX as u64));
    r.read_line(&mut line)?;
    let (verb, path) = line.trim_end().split_once(' ').ok_or_else(|| io::Error::other("bad request"))?;
    let path = PathBuf::from(path);
    if !devices.contains(&path) {
        return Err(io::Error::other(format!("{} is not an AT channel this keeper holds", path.display())));
    }
    match verb {
        "get" => {}
        "reopen" => {
            tracing::warn!(path = %path.display(), "at-keeper: closing and opening the channel again (recovery)");
            held.remove(&path);
        }
        _ => return Err(io::Error::other("bad request")),
    }
    if !held.contains_key(&path) {
        let f = sys::open(&path)?;
        tracing::info!(path = %path.display(), "at-keeper: holding the channel");
        held.insert(path.clone(), f);
    }
    held.get(&path).ok_or_else(|| io::Error::other("not held"))
}

//! `f50.shell`: root shells for the web console's terminal, over the same `/ubus` calls (and the same rpcd login and
//! ACL) as every other page. No socket of its own, no WebSocket: uhttpd cannot pass one to f50d.
//!
//! A session is a login shell on a pseudo-terminal ([`pty`]). What it prints goes into the session's scrollback
//! (the last [`SCROLLBACK`] bytes), each byte at an absolute offset; a page reads from the offset it has got to and
//! `read` waits (a long poll, at most [`WAIT_MAX_MS`]) until there is something past it. So a reloaded page attaches
//! again and gets the screen back, and a slow page loses only what scrolled out (`lost` says how much).
//!
//! Input is `write`, the terminal's size `resize`. Bytes cross JSON as base64 ([`b64`]); typed input is a secret
//! field (it may be a password) and never logged. Only the opening and the end of a session are logged.
//!
//! A session nobody has read for [`DETACHED`] is hung up, as is every session when f50d stops; an ended one is
//! forgotten [`ENDED_KEEP`] after it was last read.

mod b64;
mod pty;

use std::collections::{HashMap, VecDeque};
use std::os::unix::process::ExitStatusExt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use f50_core::{Error, Result};
use serde_json::{Value, json};
use tokio::process::Child;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Sessions at once.
const MAX_SESSIONS: usize = 8;
/// What a session keeps of its output.
const SCROLLBACK: usize = 256 * 1024;
/// The most one `read` returns.
const CHUNK: usize = 64 * 1024;
/// The longest `read` waits for output (well inside the ubus call's 30 s and uhttpd's 30 s network timeout).
pub(crate) const WAIT_MAX_MS: i64 = 20_000;
/// A running session nobody has read for this long is hung up.
const DETACHED: Duration = Duration::from_secs(15 * 60);
/// An ended session is forgotten this long after its last read.
const ENDED_KEEP: Duration = Duration::from_secs(60);
const SWEEP: Duration = Duration::from_secs(30);
/// How long an ended shell's last output is waited for, and a hung-up one for its end.
const DRAIN: Duration = Duration::from_millis(300);
const HANGUP: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// The web shells. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Shell {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    tracker: TaskTracker,
    shutdown: CancellationToken,
}

#[derive(Debug)]
struct Session {
    id: String,
    shell: String,
    pid: u32,
    started: u64,
    master: pty::Master,
    out: Mutex<Output>,
    state: watch::Sender<State>,
    seen: Mutex<Instant>,
    stop: CancellationToken,
}

/// The scrollback: `buf` holds the bytes from offset `start` on.
#[derive(Debug, Default)]
struct Output {
    buf: VecDeque<u8>,
    start: u64,
}

impl Output {
    fn end(&self) -> u64 {
        self.start + self.buf.len() as u64
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct State {
    end: u64,
    ended: bool,
    code: Option<i32>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b)).is_err() {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        b = (n as u64 ^ u64::from(std::process::id()).rotate_left(32)).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// root's shell from /etc/passwd when it is there, else ash (OpenWrt's), else sh.
fn root_shell() -> String {
    let from_passwd = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| p.lines().find(|l| l.starts_with("root:")).and_then(|l| l.rsplit(':').next()).map(str::to_owned));
    from_passwd
        .into_iter()
        .chain(["/bin/ash".to_owned(), "/bin/sh".to_owned()])
        .find(|s| s.starts_with('/') && !s.ends_with("/nologin") && !s.ends_with("/false") && is_exec(s))
        .unwrap_or_else(|| "/bin/sh".to_owned())
}

fn is_exec(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

impl Session {
    fn touch(&self) {
        *lock(&self.seen) = Instant::now();
    }

    fn idle(&self) -> Duration {
        lock(&self.seen).elapsed()
    }

    fn push(&self, bytes: &[u8]) {
        let end = {
            let mut o = lock(&self.out);
            o.buf.extend(bytes);
            let over = o.buf.len().saturating_sub(SCROLLBACK);
            if over > 0 {
                o.buf.drain(..over);
                o.start += over as u64;
            }
            o.end()
        };
        self.state.send_modify(|s| s.end = end);
    }

    /// From `from` on (clamped to the scrollback): (first offset returned, the bytes).
    fn slice(&self, from: Option<u64>) -> (u64, Vec<u8>) {
        let o = lock(&self.out);
        let from = from.unwrap_or(o.start).clamp(o.start, o.end());
        let skip = usize::try_from(from - o.start).unwrap_or(usize::MAX);
        (from, o.buf.iter().skip(skip).take(CHUNK).copied().collect())
    }

    /// Copies the terminal's output into the scrollback until the shell ends or the session is stopped, then makes
    /// sure the whole session is gone and records how the shell ended.
    async fn pump(self: Arc<Self>, mut child: Child) {
        let mut buf = vec![0u8; 16 * 1024];
        let mut status = None;
        loop {
            tokio::select! {
                () = self.stop.cancelled() => break,
                st = child.wait() => { status = st.ok(); break; }
                r = self.master.read(&mut buf) => match r {
                    Ok(n) if n > 0 => self.push(&buf[..n]),
                    // every holder of the slave side closed it: the shell is ending
                    _ => break,
                },
            }
        }
        if status.is_some() {
            // the shell is gone: what it printed last may still be in the terminal
            while let Ok(Ok(n)) = tokio::time::timeout(DRAIN, self.master.read(&mut buf)).await {
                if n == 0 {
                    break;
                }
                self.push(&buf[..n]);
            }
        }
        // whatever the shell left running in its session ends with it (SIGHUP, as a closed SSH connection does)
        pty::signal_group(self.pid, libc::SIGHUP);
        if status.is_none() {
            status = tokio::time::timeout(HANGUP, child.wait()).await.ok().and_then(Result::ok);
        }
        if status.is_none() {
            pty::signal_group(self.pid, libc::SIGKILL);
            let _ = child.kill().await;
        }
        let code = status.and_then(|s| s.code().or_else(|| s.signal().map(|n| 128 + n)));
        tracing::info!("shell {}: ended{}", self.id, code.map(|c| format!(" ({c})")).unwrap_or_default());
        self.state.send_modify(|s| {
            s.ended = true;
            s.code = code;
        });
    }
}

impl Shell {
    /// The service; its sessions' tasks run on `tracker` and are hung up at `shutdown`.
    pub(crate) fn start(tracker: &TaskTracker, shutdown: CancellationToken) -> Shell {
        let shell = Shell {
            inner: Arc::new(Inner { sessions: Mutex::default(), tracker: tracker.clone(), shutdown: shutdown.clone() }),
        };
        let s = shell.clone();
        tracker.spawn(async move {
            let mut tick = tokio::time::interval(SWEEP);
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = tick.tick() => s.sweep(),
                }
            }
        });
        shell
    }

    /// Hangs up sessions nobody reads and forgets ended ones that were seen.
    fn sweep(&self) {
        lock(&self.inner.sessions).retain(|_, s| {
            let ended = s.state.borrow().ended;
            let keep = if ended { s.idle() < ENDED_KEEP } else { s.idle() < DETACHED };
            if !keep && !ended {
                tracing::info!("shell {}: nobody reads it, hanging up", s.id);
                s.stop.cancel();
            }
            keep || !ended
        });
    }

    fn get(&self, id: &str) -> Result<Arc<Session>> {
        lock(&self.inner.sessions).get(id).cloned().ok_or_else(|| Error::not_found("shell session"))
    }

    pub(crate) fn open(&self, cols: u16, rows: u16) -> Result<Value> {
        if self.inner.shutdown.is_cancelled() {
            return Err(Error::busy("f50d is stopping"));
        }
        let mut sessions = lock(&self.inner.sessions);
        if sessions.len() >= MAX_SESSIONS {
            return Err(Error::busy(format!("{MAX_SESSIONS} shells are open; close one first")));
        }
        let shell = root_shell();
        let cwd = if std::path::Path::new("/root").is_dir() { "/root" } else { "/" };
        let env = [
            ("PATH", "/usr/sbin:/usr/bin:/sbin:/bin"),
            ("HOME", cwd),
            ("USER", "root"),
            ("LOGNAME", "root"),
            ("SHELL", shell.as_str()),
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("LANG", "C.UTF-8"),
        ];
        let (master, child) =
            pty::spawn(&shell, &env, cwd, cols, rows).map_err(|e| Error::io(format_args!("start {shell}"), &e))?;
        let pid = child.id().unwrap_or(0);
        let id = new_id();
        let session = Arc::new(Session {
            id: id.clone(),
            shell: shell.clone(),
            pid,
            started: unix_now(),
            master,
            out: Mutex::default(),
            state: watch::Sender::new(State::default()),
            seen: Mutex::new(Instant::now()),
            stop: self.inner.shutdown.child_token(),
        });
        sessions.insert(id.clone(), session.clone());
        drop(sessions);
        tracing::info!("shell {id}: opened ({shell}, pid {pid})");
        self.inner.tracker.spawn(session.pump(child));
        Ok(json!({ "id": id, "shell": shell, "pid": pid }))
    }

    /// The output from `offset` on (the start of the scrollback when absent), waiting up to `wait` for some.
    pub(crate) async fn read(&self, id: &str, offset: Option<u64>, wait: Duration) -> Result<Value> {
        let s = self.get(id)?;
        s.touch();
        let mut rx = s.state.subscribe();
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let st = *rx.borrow_and_update();
            let (from, data) = s.slice(offset);
            if !data.is_empty() || st.ended || tokio::time::Instant::now() >= deadline {
                let lost = offset.map_or(0, |o| from.saturating_sub(o));
                // the shell may have ended between the slice and the state: an ended session's output is complete
                // only once `ended` was read before the slice
                let ended = st.ended && data.len() < CHUNK;
                s.touch();
                return Ok(json!({
                    "data": b64::encode(&data),
                    "offset": from + data.len() as u64,
                    "lost": lost,
                    "ended": ended,
                    "code": if ended { st.code } else { None },
                }));
            }
            tokio::select! {
                () = self.inner.shutdown.cancelled() => return Err(Error::busy("f50d is stopping")),
                r = tokio::time::timeout_at(deadline, rx.changed()) => if matches!(r, Ok(Err(_))) {
                    return Err(Error::not_found("shell session"));
                },
            }
        }
    }

    /// Types `data` (base64) into the session.
    pub(crate) async fn write(&self, id: &str, data: &str) -> Result<Value> {
        let s = self.get(id)?;
        let bytes = b64::decode(data).ok_or_else(|| Error::bad_value("data"))?;
        if s.state.borrow().ended {
            return Err(Error::failed("the shell has ended"));
        }
        s.touch();
        tokio::time::timeout(WRITE_TIMEOUT, s.master.write_all(&bytes))
            .await
            .map_err(|_| Error::timeout("the shell taking input"))?
            .map_err(|e| Error::io("write to the shell", &e))?;
        Ok(json!({}))
    }

    pub(crate) fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<Value> {
        let s = self.get(id)?;
        s.touch();
        if !s.state.borrow().ended {
            s.master.resize(cols, rows).map_err(|e| Error::io("resize the terminal", &e))?;
        }
        Ok(json!({}))
    }

    /// Hangs the session up and forgets it.
    pub(crate) fn close(&self, id: &str) -> Result<Value> {
        let s = lock(&self.inner.sessions).remove(id).ok_or_else(|| Error::not_found("shell session"))?;
        s.stop.cancel();
        Ok(json!({}))
    }

    pub(crate) fn list(&self) -> Value {
        let mut out: Vec<Value> = lock(&self.inner.sessions)
            .values()
            .map(|s| {
                let st = *s.state.borrow();
                json!({
                    "id": s.id,
                    "shell": s.shell,
                    "pid": s.pid,
                    "started": s.started,
                    "idle": s.idle().as_secs(),
                    "ended": st.ended,
                    "code": st.code,
                })
            })
            .collect();
        out.sort_by_key(|v| v["started"].as_u64());
        json!({ "sessions": out, "max": MAX_SESSIONS })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(v: &Value) -> String {
        String::from_utf8_lossy(&b64::decode(v["data"].as_str().unwrap_or("")).unwrap_or_default()).into_owned()
    }

    #[tokio::test]
    async fn a_session_runs_reads_back_and_ends() -> Result<()> {
        let tracker = TaskTracker::new();
        let stop = CancellationToken::new();
        let shell = Shell::start(&tracker, stop.clone());
        let id = shell.open(80, 24)?["id"].as_str().unwrap_or_default().to_owned();
        shell.write(&id, &b64::encode(b"echo hel''lo; exit 7\n")).await?;
        let mut all = String::new();
        let mut offset = None;
        let mut code = None;
        for _ in 0..50 {
            let r = shell.read(&id, offset, Duration::from_secs(2)).await?;
            all.push_str(&text(&r));
            offset = r["offset"].as_u64();
            if r["ended"] == json!(true) {
                code = r["code"].as_i64();
                break;
            }
        }
        assert!(all.contains("hello"), "{all}");
        assert_eq!(code, Some(7));
        // the scrollback stays readable from the start until the session is closed
        assert!(text(&shell.read(&id, None, Duration::ZERO).await?).contains("hello"));
        shell.close(&id)?;
        assert!(shell.read(&id, None, Duration::ZERO).await.is_err());
        stop.cancel();
        tracker.close();
        tracker.wait().await;
        Ok(())
    }

    #[tokio::test]
    async fn closing_hangs_up_a_running_shell() -> Result<()> {
        let tracker = TaskTracker::new();
        let stop = CancellationToken::new();
        let shell = Shell::start(&tracker, stop.clone());
        let id = shell.open(80, 24)?["id"].as_str().unwrap_or_default().to_owned();
        let s = shell.get(&id)?;
        shell.close(&id)?;
        let mut rx = s.state.subscribe();
        tokio::time::timeout(Duration::from_secs(10), rx.wait_for(|st| st.ended))
            .await
            .map_err(|_| Error::timeout("hang-up"))?
            .map_err(|_| Error::failed("state dropped"))?;
        stop.cancel();
        tracker.close();
        tracker.wait().await;
        Ok(())
    }
}

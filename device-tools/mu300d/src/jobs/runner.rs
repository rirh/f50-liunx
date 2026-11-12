//! The detached half of a job: `mu300d job-run DIR`. Plain std, no runtime: it only waits on one child.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{RunSpec, write_private};
use crate::sys::now_s;

/// The log is cut back to its last `LOG_KEEP` bytes when it grows past `LOG_MAX`.
const LOG_MAX: u64 = 1024 * 1024;
const LOG_KEEP: usize = 256 * 1024;
/// Between TERM and KILL.
const GRACE: Duration = Duration::from_secs(5);

fn killpg(pid: i32, sig: libc::c_int) {
    // SAFETY: kill(2) on the process group created for the tool.
    unsafe {
        libc::kill(-pid, sig);
    }
}

fn finish(dir: &Path, state: &str, rc: Option<i32>, reason: Option<&str>) {
    if let Some(rc) = rc {
        let _ = write_private(&dir.join("rc"), rc.to_string().as_bytes());
    }
    if let Some(r) = reason {
        let _ = write_private(&dir.join("reason"), r.as_bytes());
    }
    let _ = write_private(&dir.join("finished"), now_s().to_string().as_bytes());
    // last: its presence says the rest is written
    let _ = write_private(&dir.join("state"), state.as_bytes());
}

/// Copy the tool's output into the log, keeping the log bounded.
fn pump(mut r: io::PipeReader, log_path: &Path) {
    let Ok(mut log) = fs::OpenOptions::new().append(true).create(true).mode(0o600).open(log_path) else {
        let _ = io::copy(&mut r, &mut io::sink());
        return;
    };
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let _ = log.write_all(&buf[..n]);
                if log.metadata().map(|m| m.len()).unwrap_or(0) > LOG_MAX {
                    let _ = cut_back(log_path);
                    if let Ok(f) = fs::OpenOptions::new().append(true).open(log_path) {
                        log = f;
                    }
                }
            }
        }
    }
}

fn cut_back(p: &Path) -> io::Result<()> {
    let mut f = fs::OpenOptions::new().read(true).write(true).open(p)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(LOG_KEEP as u64)))?;
    let mut tail = Vec::new();
    f.read_to_end(&mut tail)?;
    f.set_len(0)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(b"[earlier output dropped]\n")?;
    f.write_all(&tail)
}

/// Run the job in DIR to its end. The exit status is for the log of whoever looks; the outcome is in DIR.
pub fn run_job(dir: &Path) -> i32 {
    // our own session: a hangup or a signal to the daemon's group does not reach the job
    // SAFETY: setsid(2) has no memory effects; failure (already a group leader) is harmless here.
    unsafe {
        libc::setsid();
    }
    if write_private(&dir.join("pid"), std::process::id().to_string().as_bytes()).is_err() {
        return 1;
    }
    let spec: RunSpec = match fs::read_to_string(dir.join("spec.json")).ok().and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(s) => s,
        None => {
            finish(dir, "failed", None, Some("the job's instructions are unreadable"));
            return 1;
        }
    };
    // the secret, if any: opened, then removed before the tool runs
    let stdin_path = dir.join("stdin");
    let stdin = match fs::File::open(&stdin_path) {
        Ok(f) => {
            let _ = fs::remove_file(&stdin_path);
            Stdio::from(f)
        }
        Err(_) => Stdio::null(),
    };
    let Some((prog, args)) = spec.argv.split_first() else {
        finish(dir, "failed", None, Some("nothing to run"));
        return 1;
    };
    let (r, w) = match io::pipe() {
        Ok(p) => p,
        Err(_) => {
            finish(dir, "failed", None, Some("could not create a pipe"));
            return 1;
        }
    };
    let w2 = match w.try_clone() {
        Ok(w2) => w2,
        Err(_) => {
            finish(dir, "failed", None, Some("could not create a pipe"));
            return 1;
        }
    };
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .env("PATH", &spec.path)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(stdin)
        .stdout(w)
        .stderr(w2)
        .process_group(0);
    let spawned = cmd.spawn();
    drop(cmd); // our copies of the pipe's write end
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            finish(dir, "failed", None, Some(&format!("could not start {prog}: {}", e.kind())));
            return 1;
        }
    };
    let pid = child.id() as i32;
    let _ = write_private(&dir.join("child"), pid.to_string().as_bytes());
    let log_path = dir.join("log");
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        pump(r, &log_path);
        let _ = done_tx.send(());
    });

    let deadline = Instant::now() + Duration::from_secs(spec.timeout_s.max(1));
    let mut termed: Option<Instant> = None;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {}
            Err(_) => break None,
        }
        let now = Instant::now();
        let cancel = dir.join("cancel").exists();
        if termed.is_none() && (cancel || now >= deadline) {
            timed_out = !cancel;
            killpg(pid, libc::SIGTERM);
            termed = Some(now);
        }
        if termed.is_some_and(|t| now.duration_since(t) >= GRACE) {
            killpg(pid, libc::SIGKILL);
        }
        thread::sleep(Duration::from_millis(200));
    };
    // whatever the tool left in its group goes too
    killpg(pid, libc::SIGKILL);
    let _ = done_rx.recv_timeout(Duration::from_secs(2));

    let cancelled = dir.join("cancel").exists() && !timed_out;
    let rc = status.and_then(|s| s.code().or_else(|| s.signal().map(|n| 128 + n)));
    if cancelled {
        finish(dir, "cancelled", rc, Some("cancelled"));
    } else if timed_out {
        finish(dir, "failed", rc, Some("timed out"));
    } else if rc == Some(0) {
        finish(dir, "done", rc, None);
    } else {
        finish(dir, "failed", rc, Some(&format!("exit status {}", rc.unwrap_or(-1))));
    }
    0
}

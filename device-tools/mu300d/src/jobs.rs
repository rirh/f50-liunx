//! Background jobs: work that can take longer than a request (an update, joining a Wi-Fi network, an SMS submit,
//! a lock apply, an AT scan).
//!
//! A job is a directory under RUN/jobs/<id> and a detached runner (`mu300d job-run DIR`, its own session, reparented
//! to init), so it outlives the request, the HTTP connection and a restart of the daemon, as mu300-api's job_start
//! does with setsid. The runner starts the tool in its own process group, enforces the timeout, honours a cancel
//! request, keeps the log bounded and writes the outcome. A secret goes into a 0600 `stdin` file the runner opens
//! and removes before the tool runs. One job of a kind at a time: a second start is `busy`.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::error::{ApiError, ApiResult, Code};
use crate::api::validate::matches;
use crate::config::Config;
use crate::exec::spawn_detached;
use crate::sanitize::{sanitize_text, tail_bytes};
use crate::sys::now_s;

mod runner;
pub use runner::run_job;

/// Finished jobs kept for job.status.
const KEEP_FINISHED: usize = 20;
/// The log's tail in job.status.
const LOG_TAIL: usize = 4096;
/// The output kept as a finished job's result text.
const RESULT_TEXT: usize = 64 * 1024;
pub const ID_RE: &str = "[a-z0-9][a-z0-9.-]{0,63}";

/// What to run.
#[derive(Debug)]
pub struct JobSpec {
    pub method: &'static str,
    /// Jobs of one kind exclude each other (modem, wifi, update, sms, at).
    pub kind: &'static str,
    pub argv: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub timeout_s: u64,
    pub cancellable: bool,
    pub env: Vec<(String, String)>,
}

impl JobSpec {
    pub fn new(method: &'static str, kind: &'static str, timeout_s: u64, argv: Vec<OsString>) -> Self {
        JobSpec { method, kind, argv, stdin: None, timeout_s, cancellable: true, env: Vec::new() }
    }
    pub fn stdin(mut self, s: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(s.into());
        self
    }
    pub fn not_cancellable(mut self) -> Self {
        self.cancellable = false;
        self
    }
    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }
}

/// The runner's instructions (spec.json).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RunSpec {
    pub argv: Vec<String>,
    pub timeout_s: u64,
    pub path: String,
    pub env: Vec<(String, String)>,
}

/// What job.status reports of a job (meta.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Meta {
    pub id: String,
    pub method: String,
    pub kind: String,
    pub started: u64,
    pub cancellable: bool,
}

/// The run directory, root's own and nobody else's: refused when it is a symlink or another user's.
pub fn private_dir(p: &Path) -> ApiResult<()> {
    let bad = || ApiError::failed(format!("{} is not this user's own directory", p.display()));
    fs::create_dir_all(p).map_err(|_| bad())?;
    let m = fs::symlink_metadata(p).map_err(|_| bad())?;
    // SAFETY: geteuid(2) has no preconditions.
    let me = unsafe { libc::geteuid() };
    if !m.is_dir() || m.uid() != me {
        return Err(bad());
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o700)).map_err(|_| bad())
}

fn write_private(p: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = p.with_extension("tmp");
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(data)?;
    fs::rename(tmp, p)
}

pub(crate) fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    if fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = crate::sys::now_ms() ^ u64::from(std::process::id());
        b = t.to_le_bytes().iter().cycle().take(n).copied().collect();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// An exclusive lock on RUN/jobs/.lock for the check-then-start of a job (held for milliseconds).
struct DirLock {
    _file: fs::File,
}

impl DirLock {
    fn take(dir: &Path) -> ApiResult<Self> {
        let f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(dir.join(".lock"))
            .map_err(|_| ApiError::failed("the job directory is not writable"))?;
        // SAFETY: flock(2) on a file descriptor we own; released when the file is closed.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(ApiError::failed("could not lock the job directory"));
        }
        Ok(DirLock { _file: f })
    }
}

/// The state of a job directory now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Done => "done",
            State::Failed => "failed",
            State::Cancelled => "cancelled",
        }
    }
}

pub(crate) fn alive(pid: i32) -> bool {
    // SAFETY: kill(2) with signal 0 only checks that the process exists.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// The outcome the runner wrote, if it has.
fn final_state(dir: &Path) -> Option<(State, Option<String>)> {
    let s = read_trim(&dir.join("state"))?;
    let st = match s.as_str() {
        "done" => State::Done,
        "cancelled" => State::Cancelled,
        _ => State::Failed,
    };
    Some((st, read_trim(&dir.join("reason"))))
}

fn state_of(dir: &Path, meta: &Meta) -> (State, Option<String>) {
    if let Some(f) = final_state(dir) {
        return f;
    }
    match read_trim(&dir.join("pid")).and_then(|p| p.parse::<i32>().ok()) {
        Some(p) if alive(p) => (State::Running, None),
        // the runner writes its outcome and then exits: look again before calling it interrupted
        Some(_) => final_state(dir).unwrap_or((State::Failed, Some("interrupted: the job's runner is gone".into()))),
        // started a moment ago, its runner has not written its pid yet
        None if now_s().saturating_sub(meta.started) < 10 => (State::Running, None),
        None => (State::Failed, Some("interrupted: the job never started".into())),
    }
}

fn read_meta(dir: &Path) -> Option<Meta> {
    serde_json::from_str(&fs::read_to_string(dir.join("meta.json")).ok()?).ok()
}

fn all_jobs(jobs: &Path) -> Vec<(PathBuf, Meta)> {
    let mut v: Vec<(PathBuf, Meta)> = fs::read_dir(jobs)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| read_meta(&e.path()).map(|m| (e.path(), m)))
        .collect();
    v.sort_by(|a, b| b.1.started.cmp(&a.1.started).then_with(|| b.1.id.cmp(&a.1.id)));
    v
}

/// The id of the job of this kind that runs now.
pub fn running(cfg: &Config, kind: &str) -> Option<String> {
    all_jobs(&cfg.jobs_dir())
        .into_iter()
        .find(|(d, m)| m.kind == kind && state_of(d, m).0 == State::Running)
        .map(|(_, m)| m.id)
}

/// Start a job; its id, or `busy` when one of its kind runs.
pub fn start(cfg: &Config, spec: JobSpec) -> ApiResult<String> {
    private_dir(&cfg.run_dir)?;
    let jobs = cfg.jobs_dir();
    private_dir(&jobs)?;
    let _lock = DirLock::take(&jobs)?;
    if let Some(id) = running(cfg, spec.kind) {
        return Err(ApiError::busy(format!("another {} job runs: {id}", spec.kind)));
    }
    prune(&jobs);
    let id = format!("{}-{}-{}", spec.method.replace('.', "-"), now_s(), random_hex(4));
    let dir = jobs.join(&id);
    fs::create_dir(&dir).map_err(|_| ApiError::failed("could not create the job directory"))?;
    let meta = Meta {
        id: id.clone(),
        method: spec.method.into(),
        kind: spec.kind.into(),
        started: now_s(),
        cancellable: spec.cancellable,
    };
    let run = RunSpec {
        argv: spec.argv.iter().map(|a| a.to_string_lossy().into_owned()).collect(),
        timeout_s: spec.timeout_s,
        path: cfg.child_path.to_string_lossy().into_owned(),
        env: spec.env,
    };
    let io_err = |_| ApiError::failed("could not write the job directory");
    write_private(&dir.join("meta.json"), &serde_json::to_vec(&meta).unwrap_or_default()).map_err(io_err)?;
    write_private(&dir.join("spec.json"), &serde_json::to_vec(&run).unwrap_or_default()).map_err(io_err)?;
    if let Some(s) = &spec.stdin {
        write_private(&dir.join("stdin"), s).map_err(io_err)?;
    }
    let argv = [cfg.self_exe.clone().into_os_string(), "job-run".into(), dir.clone().into_os_string()];
    if spawn_detached(cfg, &argv, &[]).is_err() {
        let _ = fs::remove_file(dir.join("stdin"));
        let _ = write_private(&dir.join("reason"), b"could not start the job runner");
        let _ = write_private(&dir.join("state"), b"failed");
        return Err(ApiError::failed("could not start the job"));
    }
    Ok(id)
}

fn prune(jobs: &Path) {
    let finished: Vec<PathBuf> =
        all_jobs(jobs).into_iter().filter(|(d, m)| state_of(d, m).0 != State::Running).map(|(d, _)| d).collect();
    for d in finished.iter().skip(KEEP_FINISHED) {
        let _ = fs::remove_dir_all(d);
    }
}

fn job_dir(cfg: &Config, id: &str) -> ApiResult<(PathBuf, Meta)> {
    if !matches(ID_RE, id) {
        return Err(ApiError::bad_value("id"));
    }
    let dir = cfg.jobs_dir().join(id);
    let meta = read_meta(&dir).ok_or_else(|| ApiError::field(Code::NotFound, "id", format!("no job {id}")))?;
    Ok((dir, meta))
}

/// The last `max` bytes of the log, from a line start where there is one, sanitised.
fn log_tail(dir: &Path, max: usize) -> String {
    let Ok(mut f) = fs::File::open(dir.join("log")) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(max as u64);
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let text = if from > 0 { text.split_once('\n').map(|(_, r)| r).unwrap_or(&text) } else { &text };
    sanitize_text(tail_bytes(text, max))
}

/// job.status's data.
pub fn status(cfg: &Config, id: &str) -> ApiResult<Value> {
    let (dir, meta) = job_dir(cfg, id)?;
    let (state, reason) = state_of(&dir, &meta);
    let mut v = json!({
        "id": meta.id, "method": meta.method, "kind": meta.kind, "state": state.as_str(),
        "started": meta.started, "cancellable": meta.cancellable && state == State::Running,
        "log_tail": log_tail(&dir, LOG_TAIL),
    });
    if state != State::Running {
        if let Some(f) = read_trim(&dir.join("finished")).and_then(|s| s.parse::<u64>().ok()) {
            v["finished"] = json!(f);
        }
        let rc = read_trim(&dir.join("rc")).and_then(|s| s.parse::<i64>().ok());
        let text = log_tail(&dir, RESULT_TEXT);
        if let Some(r) = reason {
            // a tool that failed says why on its last line; the exit status is the fallback
            let last = crate::sanitize::last_message(&text);
            let why = if r.starts_with("exit status") && !last.is_empty() { last } else { r };
            v["error"] = json!(crate::sanitize::sanitize_message(&why));
        }
        v["result"] = json!({ "rc": rc, "text": text });
    }
    Ok(v)
}

/// job.cancel: the cancel mark, and TERM to the tool's process group (the runner sends KILL after a grace time).
pub fn cancel(cfg: &Config, id: &str) -> ApiResult<Value> {
    let (dir, meta) = job_dir(cfg, id)?;
    if state_of(&dir, &meta).0 != State::Running {
        return Err(ApiError::new(Code::Failed, "the job is not running"));
    }
    if !meta.cancellable {
        return Err(ApiError::new(Code::Forbidden, format!("{} cannot be stopped half way", meta.method)));
    }
    write_private(&dir.join("cancel"), b"1").map_err(|_| ApiError::failed("could not mark the job"))?;
    if let Some(p) = read_trim(&dir.join("child")).and_then(|p| p.parse::<i32>().ok()) {
        // SAFETY: kill(2) on the process group the runner created for the tool.
        unsafe {
            libc::kill(-p, libc::SIGTERM);
        }
    }
    Ok(json!({ "id": meta.id, "cancelling": true }))
}

/// job.list's data: the jobs kept, newest first.
pub fn list(cfg: &Config) -> Value {
    let jobs: Vec<Value> = all_jobs(&cfg.jobs_dir())
        .into_iter()
        .map(|(d, m)| {
            json!({"id": m.id, "method": m.method, "kind": m.kind, "started": m.started,
                   "state": state_of(&d, &m).0.as_str()})
        })
        .collect();
    json!({ "jobs": jobs })
}

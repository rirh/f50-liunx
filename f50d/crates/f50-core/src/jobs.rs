//! Work that outlives a request: a lock apply, an SMS send, joining a Wi-Fi network, an update.
//!
//! A job is a task the daemon owns (it is tracked, so shutdown waits for it), with an id the caller polls through
//! the `f50.job` object. Jobs of one *kind* exclude each other: a second start while one runs is `busy`. A job has
//! a timeout, may be cancellable, keeps a bounded log, and its status is mirrored into the runtime directory so a
//! restarted daemon reports a job it lost as `interrupted` instead of forgetting it.
//!
//! Work that must survive the daemon itself (writing an update to the boot partition) does not belong here alone:
//! the owning crate makes that step restartable and uses a job only to report it.

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::{CancellationToken, WaitForCancellationFuture};
use tokio_util::task::TaskTracker;

use crate::error::{Code, Error, Result};
use crate::fsx;
use crate::rpc::{Access, Field, Object, Params, Rule};
use crate::sanitize::{sanitize_text, tail_bytes};
use crate::time::unix_now;

/// Finished jobs kept for `f50.job status`.
const KEEP_FINISHED: usize = 20;
/// The log kept per job, in bytes (the oldest lines go first).
const LOG_MAX: usize = 16 * 1024;
/// The log's tail in a status reply.
const LOG_TAIL: usize = 4096;

pub const ID_RE: &str = "[a-z][a-z0-9-]{0,47}";

/// What to start.
#[derive(Debug, Clone, Copy)]
pub struct JobSpec {
    /// The method that started it (`f50.sms send`), for the status.
    pub method: &'static str,
    /// Jobs of one kind exclude each other: `radio`, `sms`, `wifi`, `update`, `at`.
    pub kind: &'static str,
    pub timeout: Duration,
    /// May the caller stop it half way? (Never a write to flash or the modem's NV.)
    pub cancellable: bool,
}

impl JobSpec {
    pub const fn new(method: &'static str, kind: &'static str, timeout: Duration) -> Self {
        JobSpec { method, kind, timeout, cancellable: true }
    }

    #[must_use]
    pub const fn not_cancellable(mut self) -> Self {
        self.cancellable = false;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
    /// The daemon stopped (or restarted) while it ran.
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}

/// A job as `f50.job status` reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub id: String,
    pub method: String,
    pub kind: String,
    pub state: JobState,
    pub cancellable: bool,
    pub started: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JobError>,
    pub log: String,
}

#[derive(Debug)]
struct Record {
    status: Mutex<JobStatus>,
    cancel: CancellationToken,
}

impl Record {
    fn lock(&self) -> MutexGuard<'_, JobStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct Inner {
    dir: PathBuf,
    tracker: TaskTracker,
    shutdown: CancellationToken,
    table: Mutex<VecDeque<Arc<Record>>>,
    seq: AtomicU64,
}

/// What a running job gets: its id, its cancellation, and its log.
#[derive(Debug, Clone)]
pub struct JobCtx {
    record: Arc<Record>,
}

impl JobCtx {
    pub fn id(&self) -> String {
        self.record.lock().id.clone()
    }

    /// One line into the job's log (sanitised; the oldest lines go when it is full). Never a secret.
    pub fn log(&self, line: impl AsRef<str>) {
        let mut st = self.record.lock();
        st.log.push_str(&sanitize_text(line.as_ref()));
        st.log.push('\n');
        if st.log.len() > LOG_MAX {
            let keep = tail_bytes(&st.log, LOG_MAX / 2);
            let cut = keep.find('\n').map_or(keep, |i| &keep[i + 1..]).to_owned();
            st.log = cut;
        }
    }

    /// Resolves when the job is cancelled (by the caller or by shutdown). Long steps `select!` on it.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.record.cancel.cancelled()
    }

    pub fn is_cancelled(&self) -> bool {
        self.record.cancel.is_cancelled()
    }
}

/// The daemon's jobs. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Jobs {
    inner: Arc<Inner>,
}

impl Jobs {
    /// The job table, with the statuses a previous daemon left in `run_dir/jobs` (its running jobs become
    /// `interrupted`).
    pub fn new(run_dir: PathBuf, tracker: TaskTracker, shutdown: CancellationToken) -> Self {
        let dir = run_dir.join("jobs");
        let mut table = VecDeque::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut old: Vec<JobStatus> = rd
                .flatten()
                .filter_map(|e| std::fs::read(e.path()).ok())
                .filter_map(|b| serde_json::from_slice(&b).ok())
                .collect();
            old.sort_by_key(|s: &JobStatus| s.started);
            for mut s in old.into_iter().rev().take(KEEP_FINISHED).rev() {
                if s.state == JobState::Running {
                    s.state = JobState::Interrupted;
                    s.finished = Some(unix_now());
                }
                table.push_back(Arc::new(Record { status: Mutex::new(s), cancel: CancellationToken::new() }));
            }
        }
        Jobs { inner: Arc::new(Inner { dir, tracker, shutdown, table: Mutex::new(table), seq: AtomicU64::new(0) }) }
    }

    fn table(&self) -> MutexGuard<'_, VecDeque<Arc<Record>>> {
        self.inner.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn persist(&self, st: &JobStatus) {
        if fsx::private_dir(&self.inner.dir).is_err() {
            return;
        }
        if let Ok(b) = serde_json::to_vec(st) {
            let path = self.inner.dir.join(format!("{}.json", st.id));
            if let Err(e) = fsx::write_atomic(&path, &b, 0o600) {
                tracing::warn!(job = %st.id, error = %e, "could not record the job's status");
            }
        }
    }

    /// Starts `work` as a job and returns its id at once. `busy` when a job of the same kind runs.
    pub fn start<F, Fut, T>(&self, spec: JobSpec, work: F) -> Result<String>
    where
        F: FnOnce(JobCtx) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
        T: Serialize,
    {
        if self.inner.shutdown.is_cancelled() {
            return Err(Error::busy("the daemon is stopping"));
        }
        let record = {
            let mut table = self.table();
            if let Some(r) = table.iter().find(|r| {
                let s = r.lock();
                s.kind == spec.kind && s.state == JobState::Running
            }) {
                let s = r.lock();
                return Err(Error::busy(format!("another {} job runs ({})", spec.kind, s.id)));
            }
            let n = self.inner.seq.fetch_add(1, Ordering::Relaxed);
            let id = format!("{}-{}-{n}", spec.kind, unix_now());
            let record = Arc::new(Record {
                status: Mutex::new(JobStatus {
                    id,
                    method: spec.method.to_owned(),
                    kind: spec.kind.to_owned(),
                    state: JobState::Running,
                    cancellable: spec.cancellable,
                    started: unix_now(),
                    finished: None,
                    result: None,
                    error: None,
                    log: String::new(),
                }),
                cancel: self.inner.shutdown.child_token(),
            });
            table.push_back(record.clone());
            // drop the oldest finished ones beyond the limit; running ones always stay
            while table.len() > KEEP_FINISHED {
                let Some(pos) = table.iter().position(|r| r.lock().state != JobState::Running) else { break };
                if let Some(old) = table.remove(pos) {
                    let _ = std::fs::remove_file(self.inner.dir.join(format!("{}.json", old.lock().id)));
                }
            }
            record
        };
        let id = record.lock().id.clone();
        self.persist(&record.lock().clone());

        let jobs = self.clone();
        let ctx = JobCtx { record: record.clone() };
        let fut = work(ctx);
        self.inner.tracker.spawn(async move {
            let outcome = tokio::select! {
                r = fut => Some(r.and_then(|v| serde_json::to_value(v)
                    .map_err(|e| Error::failed(format!("job result not serialisable: {e}"))))),
                () = record.cancel.cancelled() => None,
                () = tokio::time::sleep(spec.timeout) => Some(Err(Error::timeout(spec.method))),
            };
            let snapshot = {
                let mut st = record.lock();
                st.finished = Some(unix_now());
                match outcome {
                    Some(Ok(v)) => {
                        st.state = JobState::Done;
                        st.result = Some(v);
                    }
                    Some(Err(e)) => {
                        st.state = JobState::Failed;
                        st.error = Some(JobError { code: e.code().as_str().into(), message: e.message().into() });
                    }
                    None if jobs.inner.shutdown.is_cancelled() => st.state = JobState::Interrupted,
                    None => st.state = JobState::Cancelled,
                }
                st.clone()
            };
            if snapshot.state == JobState::Failed {
                tracing::warn!(job = %snapshot.id, method = %snapshot.method, "job failed");
            }
            jobs.persist(&snapshot);
        });
        Ok(id)
    }

    fn find(&self, id: &str) -> Result<Arc<Record>> {
        self.table().iter().find(|r| r.lock().id == id).cloned().ok_or_else(|| Error::not_found("job"))
    }

    pub fn status(&self, id: &str) -> Result<JobStatus> {
        let r = self.find(id)?;
        let mut s = r.lock().clone();
        s.log = tail_bytes(&s.log, LOG_TAIL).to_owned();
        Ok(s)
    }

    /// Every job kept, oldest first, without logs.
    pub fn list(&self) -> Vec<JobStatus> {
        self.table()
            .iter()
            .map(|r| {
                let mut s = r.lock().clone();
                s.log.clear();
                s
            })
            .collect()
    }

    /// The running job of a kind, if any (a page that reloads finds its job again).
    pub fn running(&self, kind: &str) -> Option<String> {
        self.table()
            .iter()
            .map(|r| r.lock())
            .find(|s| s.kind == kind && s.state == JobState::Running)
            .map(|s| s.id.clone())
    }

    pub fn cancel(&self, id: &str) -> Result<()> {
        let r = self.find(id)?;
        let s = r.lock();
        if s.state != JobState::Running {
            return Err(Error::new(Code::Failed, "the job is not running"));
        }
        if !s.cancellable {
            return Err(Error::forbidden(format!("{} cannot be stopped half way", s.method)));
        }
        drop(s);
        r.cancel.cancel();
        Ok(())
    }

    /// The `f50.job` object: status, list, cancel.
    pub fn object(&self) -> Object {
        const ID: &[Field] = &[Field::req("id", Rule::Word { re: ID_RE, max: 48 })];
        let (a, b, c) = (self.clone(), self.clone(), self.clone());
        Object::builder("f50.job")
            .method(
                "status",
                Access::Read,
                ID,
                "A job's state, result or error, and the tail of its log.",
                move |p: Params| {
                    let r = a.status(p.str("id").unwrap_or_default());
                    async move { r }
                },
            )
            .method("list", Access::Read, &[], "Every job kept, oldest first.", move |_p: Params| {
                let r = b.list();
                async move { Ok(r) }
            })
            .method("cancel", Access::Write, ID, "Stops a running job that may be stopped.", move |p: Params| {
                let r = c.cancel(p.str("id").unwrap_or_default()).map(|()| serde_json::json!({}));
                async move { r }
            })
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jobs(tag: &str) -> (Jobs, TaskTracker, PathBuf) {
        let dir = std::env::temp_dir().join(format!("f50-jobs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let t = TaskTracker::new();
        (Jobs::new(dir.clone(), t.clone(), CancellationToken::new()), t, dir)
    }

    async fn settle(j: &Jobs, id: &str) -> JobStatus {
        for _ in 0..200 {
            let s = j.status(id).expect("job");
            if s.state != JobState::Running {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        j.status(id).expect("job")
    }

    #[tokio::test]
    async fn one_job_per_kind_and_results() {
        let (j, _t, dir) = jobs("kind");
        let spec = JobSpec::new("t.slow", "radio", Duration::from_secs(5));
        let id = j
            .start(spec, |ctx| async move {
                ctx.log("working");
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(serde_json::json!({"x": 1}))
            })
            .expect("started");
        assert_eq!(j.start(spec, |_| async { Ok(()) }).expect_err("busy").code(), Code::Busy);
        assert_eq!(j.running("radio").as_deref(), Some(id.as_str()));
        let s = settle(&j, &id).await;
        assert_eq!(s.state, JobState::Done);
        assert_eq!(s.result, Some(serde_json::json!({"x": 1})));
        assert_eq!(s.log, "working\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn cancel_timeout_and_restart() {
        let (j, _t, dir) = jobs("cancel");
        let slow = |ctx: JobCtx| async move {
            ctx.cancelled().await;
            Ok(())
        };
        let id = j.start(JobSpec::new("t.a", "a", Duration::from_secs(5)), slow).expect("a");
        j.cancel(&id).expect("cancel");
        assert_eq!(settle(&j, &id).await.state, JobState::Cancelled);

        let id = j.start(JobSpec::new("t.b", "b", Duration::from_millis(50)), slow).expect("b");
        let s = settle(&j, &id).await;
        assert_eq!((s.state, s.error.map(|e| e.code)), (JobState::Failed, Some("timeout".into())));

        let id = j.start(JobSpec::new("t.c", "c", Duration::from_secs(5)).not_cancellable(), slow).expect("c");
        assert_eq!(j.cancel(&id).expect_err("forbidden").code(), Code::Forbidden);
        // a new daemon finds it running in the mirror: interrupted
        let j2 = Jobs::new(dir.clone(), TaskTracker::new(), CancellationToken::new());
        assert_eq!(j2.status(&id).expect("kept").state, JobState::Interrupted);
        let _ = std::fs::remove_dir_all(dir);
    }
}

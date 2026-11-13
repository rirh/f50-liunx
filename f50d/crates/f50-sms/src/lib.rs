//! SMS on the F50: the message pool, PDU encoding and decoding, receiving and sending (`f50.sms`).
//!
//! Replaces the shell layer's `f50-sms` (the pool), `f50-smsd` (receiving), `f50-cmt-spool` (direct
//! delivery to disk), `sms-pdu.awk` (the codec), `sms` and the SMS methods of `f50dash`.
//!
//! - [`pdu`]: the codec, pure functions;
//! - `pool`: messages on flash, conversations, deduplication, the parts of long messages;
//! - `receiver`: the task that keeps the pool in step with the modem (`+CMT`, `+CMTI`, the periodic sync);
//! - `sender`: a send, as a job;
//! - `api`: the `f50.sms` object.
//!
//! Readings never wait for the modem: `list`, `read` and `unread` are answered from the pool.

mod api;
mod group;
mod hook;
mod md5;
mod modem;
pub mod pdu;
mod pool;
mod receiver;
mod sender;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_at::At;
use f50_core::jobs::{JobSpec, Jobs};
use f50_core::paths::Paths;
use f50_core::rpc::Object;
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::modem::Storage;
use crate::pool::{MessageView, Page, Pool};

/// How SMS runs. [`Default`] is the F50's.
#[derive(Debug, Clone)]
pub struct SmsConfig {
    /// The pool's directory; `None`: `<state>/sms` (`/etc/f50d/sms`).
    pub dir: Option<PathBuf>,
    /// The shell pool copied in on the first start (a device path, under [`Paths`]).
    pub legacy_pool: PathBuf,
    /// The receive hook (a device path, under [`Paths`]); run when it is an executable file.
    pub hook: PathBuf,
    pub hook_timeout: Duration,
    /// Ask for direct delivery (`+CMT`, FINDINGS 51); otherwise messages are stored and announced (`+CMTI`).
    pub direct: bool,
    /// The safety-net sync of the modem's store.
    pub sync_interval: Duration,
    /// How long the parts of a long message wait for their siblings before it is pooled incomplete.
    pub partial_wait: Duration,
    /// A send job's whole budget (the shell's 200 s: four commands with the modem's answers, and some margin).
    pub send_timeout: Duration,
    /// The receiver's longest wait for the control channel: a client stuck on it must not hold a message's
    /// acknowledgement (f50-smsd's 8 s).
    pub rx_wait: Duration,
}

impl Default for SmsConfig {
    fn default() -> Self {
        SmsConfig {
            dir: None,
            legacy_pool: "/etc/f50/sms-pool".into(),
            hook: "/etc/f50/sms-hook".into(),
            hook_timeout: Duration::from_secs(60),
            direct: true,
            sync_interval: Duration::from_secs(60),
            partial_wait: Duration::from_secs(3600),
            send_timeout: Duration::from_secs(200),
            rx_wait: Duration::from_secs(8),
        }
    }
}

/// What the receiver, the sender and the API share.
#[derive(Debug)]
pub(crate) struct Inner {
    pub cfg: SmsConfig,
    pub at: At,
    pub jobs: Jobs,
    pub pool: Pool,
    /// The store as the last sync left it.
    pub storage: Mutex<Option<Storage>>,
    pub sync_now: mpsc::Sender<oneshot::Sender<Result<()>>>,
    pub hooks: mpsc::Sender<hook::HookEvent>,
    /// The concatenation reference of the next long message.
    pub reference: AtomicU8,
}

/// SMS. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Sms {
    inner: Arc<Inner>,
}

impl Sms {
    /// Opens the pool (copying the shell pool in on the first start) and starts the receiver and the hook on
    /// `tracker`; they stop at `shutdown`.
    pub fn start(
        cfg: SmsConfig,
        at: At,
        jobs: Jobs,
        paths: Paths,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Sms> {
        let dir = cfg.dir.clone().unwrap_or_else(|| paths.state().join("sms"));
        let pool = Pool::open(dir, &paths.sys(&cfg.legacy_pool), cfg.partial_wait.as_secs())?;
        let hooks = hook::start(paths.sys(&cfg.hook), cfg.hook_timeout, tracker, shutdown.clone());
        let (sync_now, sync_requests) = mpsc::channel(1);
        let inner = Arc::new(Inner {
            cfg,
            at,
            jobs,
            pool,
            storage: Mutex::new(None),
            sync_now,
            hooks,
            reference: AtomicU8::new((unix_now() % 256) as u8),
        });
        tracker.spawn(receiver::run(inner.clone(), sync_requests, shutdown));
        Ok(Sms { inner })
    }

    /// The `f50.sms` object.
    pub fn objects(&self) -> Vec<Object> {
        vec![api::object(self.clone())]
    }

    /// Conversations, or the messages of one (`peer`), newest first.
    async fn list(&self, page: Page, peer: Option<String>) -> Result<Value> {
        let storage = self.inner.storage.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let mut v = self
            .inner
            .pool
            .with(move |st| {
                Ok(match peer {
                    Some(p) => st.conversation(&p, page),
                    None => st.conversations(page),
                })
            })
            .await?;
        v["sim_storage"] = json!(storage);
        Ok(v)
    }

    /// One message whole; a received one is marked read.
    async fn read(&self, id: u64) -> Result<MessageView> {
        self.inner.pool.with(move |st| st.mark(id, true).map(|m| MessageView::from(&m))).await
    }

    async fn counts(&self) -> Result<(usize, usize)> {
        self.inner.pool.with(|st| Ok(st.counts())).await
    }

    /// Starts a send; the job's id. A number or text that cannot be sent is refused here, before any job.
    fn send(&self, number: &str, text: &str) -> Result<String> {
        let reference = self.inner.reference.fetch_add(1, Ordering::Relaxed);
        let (coding, parts) = pdu::encode_submit(number, text, reference)?;
        let out = sender::Outgoing { number: number.to_owned(), text: text.to_owned(), coding, parts };
        let spec = JobSpec::new("f50.sms send", "sms", self.inner.cfg.send_timeout).not_cancellable();
        let inner = self.inner.clone();
        self.inner.jobs.start(spec, move |ctx| sender::run(inner, ctx, out))
    }

    /// Deletes one message or all; with `sim`, the matching messages in the modem's store too (first: the slots are
    /// found by fingerprint, and the store answers for that).
    async fn delete(&self, id: Option<u64>, sim: bool) -> Result<Value> {
        let mut sim_deleted = 0;
        if sim {
            let fps: HashSet<String> = self
                .inner
                .pool
                .with(move |st| {
                    Ok(match id {
                        Some(i) => vec![st.get(i)?.fingerprint.clone()],
                        None => st.messages().map(|m| m.fingerprint.clone()).collect(),
                    })
                })
                .await?
                .into_iter()
                .collect();
            let mut s = self.inner.at.session("sms", Duration::from_secs(15)).await?;
            let (_, slots) = modem::list(&mut s).await?;
            let pairs: Vec<(u32, String)> = slots.into_iter().map(|s| (s.index, s.pdu)).collect();
            for (fp, idx) in pool::sim_messages(&pairs) {
                if fps.contains(&fp) {
                    for i in idx {
                        if modem::delete_slot(&mut s, i).await {
                            sim_deleted += 1;
                        } else {
                            tracing::warn!(slot = i, "the modem kept a stored message");
                        }
                    }
                }
            }
        }
        let gone = self.inner.pool.with(move |st| st.delete(id)).await?;
        Ok(json!({ "deleted": gone.len(), "sim_deleted": sim_deleted }))
    }

    /// A job that waits for the receiver to restore notifications and sync the modem's store.
    fn sync(&self) -> Result<String> {
        let spec = JobSpec::new("f50.sms sync", "sms", Duration::from_secs(180)).not_cancellable();
        let inner = self.inner.clone();
        self.inner.jobs.start(spec, move |_ctx| async move {
            let (reply, done) = oneshot::channel();
            inner.sync_now.try_send(reply).map_err(|_| Error::busy("the SMS receiver is busy or stopping"))?;
            done.await.map_err(|_| Error::failed("the SMS receiver stopped before sync completed"))??;
            Ok(json!({ "synced": true }))
        })
    }
}

/// A message id from a checked request field.
fn parse_id(v: &str) -> Result<u64> {
    v.parse().map_err(|_| Error::bad_value("id"))
}

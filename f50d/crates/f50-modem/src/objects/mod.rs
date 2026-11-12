//! The ubus objects: `f50.modem` ([`modem`]), `f50.sim` ([`sim`]), `f50.lock` ([`lock`]). Handlers only read the
//! service's state or start jobs; no handler waits for the modem except where a method is a job.

mod lock;
mod modem;
mod sim;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use f50_core::Result;
use f50_core::jobs::{JobCtx, JobSpec};
use f50_core::rpc::Object;
use serde::Serialize;
use serde_json::{Value, json};

use crate::shared::Inner;

/// A radio sequence's time at most: the radio lock's wait, the writes, the restart and its read-backs.
const RADIO_JOB_TIMEOUT: Duration = Duration::from_secs(420);

pub(crate) fn all(inner: &Arc<Inner>) -> Vec<Object> {
    vec![modem::object(inner), sim::object(inner), lock::object(inner)]
}

/// Starts a radio job (kind `radio`: one at a time; never cancellable, it may be writing the modem's NV) and answers
/// `{"job": id}`.
fn radio_job<F, Fut, T>(inner: &Arc<Inner>, method: &'static str, work: F) -> Result<Value>
where
    F: FnOnce(Arc<Inner>, JobCtx) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T>> + Send + 'static,
    T: Serialize,
{
    let me = inner.clone();
    let spec = JobSpec::new(method, "radio", RADIO_JOB_TIMEOUT).not_cancellable();
    let id = inner.jobs.start(spec, move |ctx| work(me, ctx))?;
    Ok(json!({ "job": id }))
}

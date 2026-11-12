//! The unsolicited channel (`/dev/stty_nr0` for the F50's slot 0): read for the whole life of the process, every line
//! published as a [`Urc`].
//!
//! Each `stty_nr<n>` is one sbuf ring in memory shared with the modem; the modem writes into it whether anyone reads
//! or not, and registration and PDP changes, +CMTI, +CUSD answers, +ECIND SIM events are announced on nr0 and
//! nowhere else, the way Android's RIL drains it (docs/research/FINDINGS.md 13b, 26d). Its lines are also the
//! evidence the command channels' reopen rule needs: a nr0 that keeps talking means the modem runs.
//!
//! A +CMT/+CDS header and the PDU line after it are one [`Urc`] (`body`), never logged: the PDU is the message, and
//! in text mode the header carries the sender (docs/research/FINDINGS.md 51). Spooling and acknowledging it is
//! `f50-sms`'s business. A drainer that reaches end of file stays gone on purpose: reopening a SIPC channel is the
//! one operation that can lose it (FINDINGS 13b).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::Urc;
use crate::channel::{Liveness, Source};
use crate::codes;
use crate::reader::Event;
use crate::wire::Wire;

const OPEN_RETRY: Duration = Duration::from_millis(200);
const OPEN_RETRY_SLOW: Duration = Duration::from_secs(2);
const OPEN_WAIT: Duration = Duration::from_secs(240);

#[derive(Debug)]
pub(crate) struct UrcChannel {
    pub(crate) path: PathBuf,
    pub(crate) urcs: broadcast::Sender<Urc>,
    pub(crate) live: Arc<Liveness>,
    pub(crate) tracker: TaskTracker,
    pub(crate) shutdown: CancellationToken,
}

impl UrcChannel {
    pub(crate) async fn run(self) {
        let Some(mut wire) = self.open_wait().await else { return };
        loop {
            let ev = tokio::select! {
                biased;
                () = self.shutdown.cancelled() => break,
                ev = wire.next() => ev,
            };
            match ev {
                Some(Event::Line(line)) => {
                    if codes::is_echo(&line) {
                        continue;
                    }
                    let body = if codes::takes_body(&line) { wire.body().await } else { None };
                    let _ = self.urcs.send(Urc { line, body });
                }
                Some(Event::Prompt | Event::Closed) => {}
                None => {
                    tracing::warn!(path = %self.path.display(), "the unsolicited AT channel has closed; not reopened");
                    // stays open (and silent) until shutdown: closing it could cost the channel for this boot
                    self.shutdown.cancelled().await;
                    break;
                }
            }
        }
        wire.close().await;
    }

    async fn open_wait(&self) -> Option<Wire> {
        let start = Instant::now();
        let mut waiting = false;
        loop {
            match Wire::open(&self.path, "urc", &self.tracker, &self.shutdown, self.live.clone(), Source::Urc, false) {
                Ok(w) => {
                    if waiting {
                        tracing::info!(path = %self.path.display(), after_s = start.elapsed().as_secs(), "unsolicited AT channel ready");
                    }
                    return Some(w);
                }
                Err(e) if !waiting => {
                    tracing::info!(path = %self.path.display(), error = %e, "the unsolicited AT channel is not ready; waiting");
                    waiting = true;
                }
                Err(_) => {}
            }
            let nap = if start.elapsed() < OPEN_WAIT { OPEN_RETRY } else { OPEN_RETRY_SLOW };
            tokio::select! {
                () = self.shutdown.cancelled() => return None,
                () = tokio::time::sleep(nap) => {}
            }
        }
    }
}

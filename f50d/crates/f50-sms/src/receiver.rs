//! Receiving: one task keeps the pool in step with the modem.
//!
//! - At start, and whenever the modem answers again after a restart (`At::ready`), the setup the shell daemon did
//!   (f50-smsd): stored mode (`+CMTI`), a first sync of what the store holds, then direct delivery when the modem
//!   takes it (FINDINGS 51).
//! - `+CMT` (direct delivery): the PDU parked on the disk, absorbed, and only then acknowledged (`AT+CNMA=1`); until
//!   then the modem keeps it. A PDU that cannot be pooled is not acknowledged; three failures in a row give direct
//!   delivery up for the stored path, where the modem re-delivers what was never acknowledged.
//! - `+CMTI` (stored): the slot read (`AT+CMGR`), parked, deleted from the store, absorbed. A full SIM takes no more
//!   messages, so nothing is left on it once the pool has it (FINDINGS 35, 39).
//! - Every `sync_interval`, and on request: the whole store listed and moved into the pool (a missed announcement,
//!   a message the modem kept anyway), the parts waiting past the partial wait pooled, and direct routing checked
//!   (the modem drops it by itself after an acknowledgement that never came).
//!
//! The order is always the same: parked on the disk first (file and directory entry fsynced), then the SIM slot
//! deleted or the network told; a power cut loses neither copy.

use std::sync::Arc;
use std::time::Duration;

use f50_at::Urc;
use f50_core::time::unix_now;
use f50_core::{Code, Error, Result};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::Inner;
use crate::hook;
use crate::modem::{self, Slot};
use crate::pdu::{Decoded, decode};
use crate::pool::Absorbed;

/// Failed `+CMT` rounds in a row before direct delivery is given up.
const MAX_FAILURES: u32 = 3;
const ACK_RETRY: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
struct Rx {
    configured: bool,
    direct: bool,
    failures: u32,
    ack_pending: bool,
}

/// The receiver task: until `shutdown`.
pub(crate) async fn run(
    svc: Arc<Inner>,
    mut sync_requests: mpsc::Receiver<oneshot::Sender<Result<()>>>,
    shutdown: CancellationToken,
) {
    let mut urcs = svc.at.urcs();
    let mut ready = svc.at.ready();
    let mut urcs_open = true;
    let mut ready_open = true;
    let mut tick = tokio::time::interval(svc.cfg.sync_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    tick.reset(); // the first sync is the setup's
    let mut ack_tick = tokio::time::interval(ACK_RETRY);
    ack_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ack_tick.reset();
    let mut rx = Rx::default();
    let mut up = *ready.borrow_and_update();
    if up {
        guarded(&shutdown, periodic(&svc, &mut rx, up)).await;
    }
    loop {
        tokio::select! {
            biased; // park queued deliveries before retrying their unnumbered CNMA acknowledgement
            () = shutdown.cancelled() => break,
            r = ready.changed(), if ready_open => {
                if r.is_err() {
                    ready_open = false;
                    continue;
                }
                let now_up = *ready.borrow_and_update();
                if now_up && !up {
                    // the modem restarted: its notification settings are its defaults again
                    rx.configured = false;
                    guarded(&shutdown, periodic(&svc, &mut rx, now_up)).await;
                } else if !now_up {
                    rx.configured = false;
                    rx.direct = false;
                    rx.ack_pending = false;
                }
                up = now_up;
            }
            u = urcs.recv(), if urcs_open => match u {
                Ok(urc) => {
                    let pending = rx.ack_pending;
                    guarded(&shutdown, on_urc(&svc, &mut rx, urc)).await;
                    if rx.ack_pending && !pending {
                        ack_tick.reset();
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    // an announcement may be among the lost lines: the store is listed instead
                    tracing::warn!(lost = n, "unsolicited lines were lost; syncing the SMS store");
                    guarded(&shutdown, sync_logged(&svc)).await;
                }
                Err(RecvError::Closed) => urcs_open = false,
            },
            _ = ack_tick.tick(), if rx.ack_pending => guarded(&shutdown, acknowledge_pending(&svc, &mut rx)).await,
            _ = tick.tick() => guarded(&shutdown, periodic(&svc, &mut rx, up)).await,
            Some(mut reply) = sync_requests.recv() => guarded(&shutdown, async {
                let result = tokio::select! {
                    biased;
                    () = reply.closed() => None,
                    result = manual_sync(&svc, &mut rx) => Some(result),
                };
                if let Some(result) = result {
                    let _ = reply.send(result);
                }
            }).await,
        }
    }
}

/// `work`, or nothing once shutdown begins: a step cut short is safe (what was parked stays parked, the SIM keeps
/// what was not deleted, the modem keeps what was not acknowledged).
async fn guarded(shutdown: &CancellationToken, work: impl Future<Output = ()>) {
    tokio::select! {
        () = shutdown.cancelled() => {}
        () = work => {}
    }
}

async fn setup(svc: &Inner, rx: &mut Rx) -> Result<()> {
    rx.configured = false;
    modem::notification_mode(&svc.at, false).await?;
    rx.direct = false;
    rx.ack_pending = false;
    let synced = sync(svc).await;
    rx.direct = modem::notification_mode(&svc.at, svc.cfg.direct).await?;
    rx.configured = true;
    rx.failures = 0;
    tracing::info!(direct = rx.direct, "SMS receiving is set up");
    synced
}

async fn periodic(svc: &Inner, rx: &mut Rx, up: bool) {
    if !up {
        // the modem is away: the parts waiting past the partial wait are still pooled
        absorb_logged(svc).await;
        return;
    }
    if let Err(e) = manual_sync(svc, rx).await {
        tracing::warn!(error = %e, "SMS sync failed; retried later");
    }
}

async fn manual_sync(svc: &Inner, rx: &mut Rx) -> Result<()> {
    if !rx.configured {
        return setup(svc, rx).await;
    }
    let synced = sync(svc).await;
    if !modem::notifications_still_on(&svc.at, rx.direct).await? {
        rx.configured = false;
        rx.direct = modem::notification_mode(&svc.at, rx.direct).await?;
        rx.configured = true;
        rx.ack_pending = false;
        tracing::warn!(direct = rx.direct, "the modem had dropped SMS notifications; restored");
    }
    synced
}

async fn on_urc(svc: &Inner, rx: &mut Rx, urc: Urc) {
    let line = urc.line.trim();
    if line.starts_with("+CMTI:") {
        if let Err(e) = on_cmti(svc, line).await {
            tracing::warn!(error = %e, "a stored message could not be read; syncing the store");
            sync_logged(svc).await;
        }
    } else if line.starts_with("+CMT:") {
        rx.ack_pending = false;
        match on_cmt(svc, urc.body.as_deref()).await {
            Ok(()) => {
                rx.ack_pending = true;
                acknowledge_pending(svc, rx).await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "a directly delivered message was not pooled");
                receive_failed(svc, rx).await;
            }
        }
    } else if line.starts_with("+CDS:") {
        // a status report routed here (none is asked for, ds=0): acknowledged so the routing stays
        if let Some(Decoded::StatusReport(r)) = urc.body.as_deref().and_then(decode) {
            tracing::info!(reference = r.reference, status = r.status, "an SMS status report");
        }
        if rx.direct {
            rx.ack_pending = true;
            acknowledge_pending(svc, rx).await;
        }
    }
}

/// A busy send can hold the AT channel longer than the acknowledgement's queue wait. Retry without waiting for
/// another URC: the modem may hold every later message until this acknowledgement arrives.
async fn acknowledge_pending(svc: &Inner, rx: &mut Rx) {
    match modem::acknowledge(&svc.at).await {
        Ok(()) => {
            rx.ack_pending = false;
            rx.failures = 0;
        }
        Err(e) => {
            // Busy guarantees CNMA never reached the modem. With any other failure it may have acknowledged
            // this message already: another positive CNMA could then acknowledge an unparked later message.
            rx.ack_pending = e.code() == Code::Busy;
            if !rx.ack_pending {
                rx.failures = MAX_FAILURES - 1;
            }
            tracing::warn!(error = %e, retry = rx.ack_pending, "SMS acknowledgement failed");
            receive_failed(svc, rx).await;
        }
    }
}

async fn receive_failed(svc: &Inner, rx: &mut Rx) {
    rx.failures += 1;
    if rx.failures >= MAX_FAILURES && rx.direct {
        match modem::stored_mode(&svc.at).await {
            Ok(()) => {
                rx.direct = false;
                rx.ack_pending = false;
                rx.failures = 0;
                tracing::warn!("direct SMS delivery given up; new messages are stored and announced");
            }
            Err(e) => {
                rx.configured = false;
                tracing::warn!(error = %e, "stored SMS delivery could not be restored; retried at the next sync");
            }
        }
    }
}

/// `+CMTI: "SM",3`: the slot read, parked, deleted, absorbed.
async fn on_cmti(svc: &Inner, line: &str) -> Result<()> {
    let (mem, index) = modem::parse_cmti(line).ok_or_else(|| Error::failed("an announcement that does not read"))?;
    let mut s = svc.at.session("sms", svc.cfg.rx_wait).await?;
    let slot = modem::read_slot(&mut s, mem.as_deref(), index).await?;
    let Some(slot) = slot else { return Err(Error::not_found("stored message")) };
    let store = mem.unwrap_or_else(|| "?".into());
    if park(svc, vec![slot], &store).await?.contains(&index) {
        modem::delete_slot(&mut s, index).await;
    }
    drop(s);
    absorb_logged(svc).await;
    Ok(())
}

/// `+CMT: ,<length>` and its PDU: parked and absorbed before the caller acknowledges it.
async fn on_cmt(svc: &Inner, body: Option<&str>) -> Result<()> {
    let pdu = body.map(str::trim).filter(|p| crate::pdu::is_pdu_line(p)).ok_or_else(|| Error::failed("no PDU"))?;
    let (pdu, now) = (pdu.to_owned(), unix_now());
    let parked = svc.pool.with(move |st| st.park(&pdu, true, "air", now)).await?;
    if !parked {
        return Err(Error::failed("the PDU is not a message the codec reads"));
    }
    // on the disk now: whatever the absorb does, the message is safe, so it is acknowledged either way
    absorb_logged(svc).await;
    Ok(())
}

/// Parks slots of `store`; the indexes the pool now holds (and that may leave the store).
async fn park(svc: &Inner, slots: Vec<Slot>, store: &str) -> Result<Vec<u32>> {
    let (store, now) = (store.to_owned(), unix_now());
    svc.pool
        .with(move |st| {
            let mut held = Vec::new();
            for s in &slots {
                match st.park(&s.pdu, s.unread, &format!("sim:{store}:{}", s.index), now) {
                    Ok(true) => held.push(s.index),
                    Ok(false) => {} // a stored SUBMIT, a report: left where it is
                    Err(e) => {
                        // a full or read-only disk: what is not parked stays on the SIM
                        tracing::warn!(error = %e, "an SMS could not be parked; it stays in the modem's store");
                        return Err(e);
                    }
                }
            }
            Ok(held)
        })
        .await
}

/// The whole store into the pool. Also what `f50.sms sync` starts.
pub(crate) async fn sync(svc: &Inner) -> Result<()> {
    let listed = async {
        let mut s = svc.at.session("sms", svc.cfg.rx_wait).await?;
        let (mut storage, slots) = modem::list(&mut s).await?;
        let held = park(svc, slots, &storage.store).await?;
        let mut moved = 0;
        for i in held {
            if modem::delete_slot(&mut s, i).await {
                moved += 1;
            }
        }
        storage.used = storage.used.saturating_sub(moved);
        Ok::<_, Error>(storage)
    }
    .await;
    // the waiting parts are looked at whatever the modem did
    let absorbed = absorb(svc).await;
    let storage = listed?;
    absorbed?;
    *svc.storage.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(storage);
    Ok(())
}

async fn sync_logged(svc: &Inner) {
    if let Err(e) = sync(svc).await {
        tracing::warn!(error = %e, "SMS sync failed; retried later");
    }
}

async fn absorb_logged(svc: &Inner) {
    if let Err(e) = absorb(svc).await {
        tracing::warn!(error = %e, "the SMS pool could not take the parked messages");
    }
}

async fn absorb(svc: &Inner) -> Result<()> {
    let now = unix_now();
    let Absorbed { new, waiting } = svc.pool.with(move |st| st.absorb(now)).await?;
    if !new.is_empty() {
        tracing::info!(new = new.len(), waiting, "SMS pooled");
        hook::offer(&svc.hooks, &new);
    }
    Ok(())
}

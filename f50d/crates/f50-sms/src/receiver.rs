//! Receiving: one task keeps the pool in step with the modem.
//!
//! - At start, and whenever the modem answers again after a restart (`At::ready`), the setup the shell daemon did
//!   (mu300-smsd): stored mode (`+CMTI`), a first sync of what the store holds, then direct delivery when the modem
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

use f50_at::Urc;
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::Inner;
use crate::hook;
use crate::modem::{self, Slot};
use crate::pdu::{Decoded, decode};
use crate::pool::Absorbed;

/// Failed `+CMT` rounds in a row before direct delivery is given up.
const MAX_FAILURES: u32 = 3;

#[derive(Debug, Default)]
struct Rx {
    configured: bool,
    direct: bool,
    failures: u32,
}

/// The receiver task: until `shutdown`.
pub(crate) async fn run(svc: Arc<Inner>, shutdown: CancellationToken) {
    let mut urcs = svc.at.urcs();
    let mut ready = svc.at.ready();
    let mut urcs_open = true;
    let mut ready_open = true;
    let mut tick = tokio::time::interval(svc.cfg.sync_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    tick.reset(); // the first sync is the setup's
    let mut rx = Rx::default();
    let mut up = *ready.borrow_and_update();
    if up {
        guarded(&shutdown, setup(&svc, &mut rx)).await;
    }
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            r = ready.changed(), if ready_open => {
                if r.is_err() {
                    ready_open = false;
                    continue;
                }
                let now_up = *ready.borrow_and_update();
                if now_up && !up {
                    // the modem restarted: its notification settings are its defaults again
                    guarded(&shutdown, setup(&svc, &mut rx)).await;
                } else if !now_up {
                    rx.configured = false;
                    rx.direct = false;
                }
                up = now_up;
            }
            u = urcs.recv(), if urcs_open => match u {
                Ok(urc) => guarded(&shutdown, on_urc(&svc, &mut rx, urc)).await,
                Err(RecvError::Lagged(n)) => {
                    // an announcement may be among the lost lines: the store is listed instead
                    tracing::warn!(lost = n, "unsolicited lines were lost; syncing the SMS store");
                    guarded(&shutdown, sync_logged(&svc)).await;
                }
                Err(RecvError::Closed) => urcs_open = false,
            },
            _ = tick.tick() => guarded(&shutdown, periodic(&svc, &mut rx, up)).await,
            () = svc.sync_now.notified() => guarded(&shutdown, sync_logged(&svc)).await,
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

async fn setup(svc: &Inner, rx: &mut Rx) {
    if let Err(e) = modem::notification_mode(&svc.at, false).await {
        tracing::warn!(error = %e, "SMS setup failed; retried at the next sync");
        rx.configured = false;
        return;
    }
    sync_logged(svc).await;
    rx.direct = modem::notification_mode(&svc.at, svc.cfg.direct).await.unwrap_or(false);
    rx.configured = true;
    rx.failures = 0;
    tracing::info!(direct = rx.direct, "SMS receiving is set up");
}

async fn periodic(svc: &Inner, rx: &mut Rx, up: bool) {
    if !rx.configured {
        if up {
            setup(svc, rx).await;
        } else {
            // the modem is away: the parts waiting past the partial wait are still pooled
            absorb_logged(svc).await;
        }
        return;
    }
    sync_logged(svc).await;
    if rx.direct && modem::direct_still_on(&svc.at).await == Some(false) {
        rx.direct = modem::notification_mode(&svc.at, true).await.unwrap_or(false);
        tracing::warn!(direct = rx.direct, "the modem had dropped direct SMS delivery");
    }
}

async fn on_urc(svc: &Inner, rx: &mut Rx, urc: Urc) {
    let line = urc.line.trim();
    if line.starts_with("+CMTI:") {
        if let Err(e) = on_cmti(svc, line).await {
            tracing::warn!(error = %e, "a stored message could not be read; syncing the store");
            sync_logged(svc).await;
        }
    } else if line.starts_with("+CMT:") {
        match on_cmt(svc, urc.body.as_deref()).await {
            Ok(()) => rx.failures = 0,
            Err(e) => {
                rx.failures += 1;
                tracing::warn!(error = %e, failures = rx.failures, "a directly delivered message was not pooled");
                if rx.failures >= MAX_FAILURES && rx.direct {
                    // never keep asking the network's acknowledgement of a modem that refuses it
                    modem::stored_mode(&svc.at).await;
                    rx.direct = false;
                    rx.failures = 0;
                    tracing::warn!("direct SMS delivery given up; new messages are stored and announced");
                }
            }
        }
    } else if line.starts_with("+CDS:") {
        // a status report routed here (none is asked for, ds=0): acknowledged so the routing stays
        if let Some(Decoded::StatusReport(r)) = urc.body.as_deref().and_then(decode) {
            tracing::info!(reference = r.reference, status = r.status, "an SMS status report");
        }
        if rx.direct {
            let _ = modem::acknowledge(&svc.at).await;
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

/// `+CMT: ,<length>` and its PDU: parked, absorbed, acknowledged.
async fn on_cmt(svc: &Inner, body: Option<&str>) -> Result<()> {
    let pdu = body.map(str::trim).filter(|p| crate::pdu::is_pdu_line(p)).ok_or_else(|| Error::failed("no PDU"))?;
    let (pdu, now) = (pdu.to_owned(), unix_now());
    let parked = svc.pool.with(move |st| st.park(&pdu, true, "air", now)).await?;
    if !parked {
        return Err(Error::failed("the PDU is not a message the codec reads"));
    }
    // on the disk now: whatever the absorb does, the message is safe, so it is acknowledged either way
    absorb_logged(svc).await;
    modem::acknowledge(&svc.at).await
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
                        break;
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
    absorb_logged(svc).await;
    let storage = listed?;
    *svc.storage.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(storage);
    Ok(())
}

async fn sync_logged(svc: &Inner) {
    if let Err(e) = sync(svc).await {
        tracing::warn!(error = %e, "SMS sync failed; retried later");
    }
}

async fn absorb_logged(svc: &Inner) {
    let now = unix_now();
    match svc.pool.with(move |st| st.absorb(now)).await {
        Ok(Absorbed { new, waiting }) => {
            if !new.is_empty() {
                tracing::info!(new = new.len(), waiting, "SMS pooled");
                hook::offer(&svc.hooks, &new);
            }
        }
        Err(e) => tracing::warn!(error = %e, "the SMS pool could not take the parked messages"),
    }
}

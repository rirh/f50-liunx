//! The lock reading: eight short reads, kept as the cache every `lock get` serves (a real read is about a second
//! per command on this firmware; on a busy or silent channel each waited out its budget, far past rpcd's 30 s, so
//! the panel never read the locks inside a request again). Refreshed in the background: at start, after each
//! apply or reset (retried, since the lock reads lag a restart by a few seconds), and when a `get` finds it old.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use f50_core::time::unix_now;
use serde::Serialize;

use super::codec::{
    LTE_TABLE, NR_FALLBACK, canonical, cells_of, lte_bands, lte_full, mode_label, nr_bands, nr_read_masks,
};
use crate::shared::{Inner, lock};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModeReading {
    pub label: &'static str,
    /// The work mode of slot 0, of slot 1, and the primary field (`+SPTESTMODE`).
    pub work: Option<u32>,
    pub gran: Option<u32>,
    pub m1: Option<u32>,
    pub prim: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Bands {
    /// Locked bands; empty is automatic (nothing locked, or the full table a cleared mask reads back as).
    pub locked: Vec<u32>,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Caps {
    /// The module's own NR list (`AT+SP5GCMDS`), sorted.
    pub nr: Vec<u32>,
    /// The board's LTE table (there is no LTE capability command).
    pub lte: Vec<u32>,
}

/// What the modem holds now.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LockReading {
    pub ts: u64,
    pub mode: ModeReading,
    /// EN-DC allowed (`+ENDC: 1`; off is 2).
    pub endc: Option<bool>,
    pub lte: Bands,
    pub nr: Bands,
    /// Every locked cell, NR first (`nr:627264,393`).
    pub cells: Vec<String>,
    pub caps: Caps,
}

impl LockReading {
    /// The NR bands this modem supports (the reading's, else the measured fallback).
    pub fn nr_supported(r: Option<&LockReading>) -> Vec<u32> {
        r.map(|r| r.caps.nr.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| NR_FALLBACK.to_vec())
    }
}

fn num(s: Option<&str>) -> Option<u32> {
    s?.trim().parse().ok()
}

/// One reading; `None` when the modem answered none of the lock reads (right after an SFUN restart it answers CFUN
/// and CEREG but not SPLBAND/SPTESTMODE yet: writing that as "nothing locked" clobbered the real state).
pub(crate) async fn read(inner: &Inner) -> Option<LockReading> {
    let tm = inner.read_field("AT+SPTESTMODE?", "+SPTESTMODE", 6).await;
    let gran = inner.read_field("AT+SP5GRAN?", "+SP5GRAN", 6).await;
    let endc = inner.read_field("AT+SPENDC?", "+ENDC", 6).await;
    let ltem = inner.read_field("AT+SPLBAND=0", "+SPLBAND", 6).await;
    let nrm = inner.read_field("AT+SPLBAND=3", "+SPLBAND", 6).await;
    let cell12 = inner.read_field("AT+SPFORCEFRQ=12,3", "+SPFORCEFRQ", 6).await;
    let cell16 = inner.read_field("AT+SPFORCEFRQ=16,3", "+SPFORCEFRQ", 6).await;
    // the module's own authoritative list (measured: "+SP5GCMDS: get nr support_band,6,41,78,1,8,28,5"); the old
    // SPLBAND=4 mask decode was wrong: it reported n79, which this RF chain cannot use, and missed n6
    let nrcap = match inner.poll("AT+SP5GCMDS=\"get nr support_band\"", 6).await {
        Ok(r) => r.value("+SP5GCMDS").and_then(|v| v.strip_prefix("get nr support_band,")).map(str::to_owned),
        Err(_) => None,
    };
    if tm.is_none() && ltem.is_none() && nrm.is_none() {
        return None;
    }
    let tmf: Vec<&str> = tm.as_deref().unwrap_or("").split(',').collect();
    let work = num(tmf.first().copied());
    let gran = num(gran.as_deref());
    let mut lte_locked = ltem.as_deref().and_then(lte_bands).map(|b| canonical(&b)).unwrap_or_default();
    // the read-back as it is: a lock the panel did not save shows as that lock; only one holding every band of the
    // table is automatic (it is what a cleared mask reads back as)
    if lte_full(&lte_locked) {
        lte_locked.clear();
    }
    let nr_locked = nrm.as_deref().and_then(nr_read_masks).map(|m| canonical(&nr_bands(m))).unwrap_or_default();
    let mut caps_nr: Vec<u32> = nrcap
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|b| b.trim_matches(|c: char| c == '"' || c.is_whitespace()).parse().ok())
        .collect();
    caps_nr = canonical(&caps_nr);
    if caps_nr.is_empty() {
        caps_nr = NR_FALLBACK.to_vec();
    }
    let mut cells: Vec<String> =
        cells_of(cell16.as_deref().unwrap_or(""), true).iter().map(ToString::to_string).collect();
    cells.extend(cells_of(cell12.as_deref().unwrap_or(""), false).iter().map(ToString::to_string));
    Some(LockReading {
        ts: unix_now(),
        mode: ModeReading {
            label: mode_label(work, gran),
            work,
            gran,
            m1: num(tmf.get(1).copied()).or(Some(134)),
            prim: num(tmf.get(2).copied()).or(Some(0)),
        },
        endc: num(endc.as_deref()).map(|v| v == 1),
        lte: Bands { locked: lte_locked, raw: ltem },
        nr: Bands { locked: nr_locked, raw: nrm },
        cells,
        caps: Caps { nr: caps_nr, lte: LTE_TABLE.to_vec() },
    })
}

/// The kept reading.
pub(crate) fn cached(inner: &Inner) -> Option<Arc<LockReading>> {
    lock(&inner.lock_cache).clone()
}

pub(crate) fn store(inner: &Inner, r: LockReading) {
    *lock(&inner.lock_cache) = Some(Arc::new(r));
}

/// Reads now and keeps it; false when the modem answered none of the lock reads.
async fn refresh(inner: &Inner) -> bool {
    match read(inner).await {
        Some(r) => {
            store(inner, r);
            true
        }
        None => false,
    }
}

/// A background refresh, one at a time (a second has nothing to add).
pub(crate) fn spawn_refresh(inner: &Arc<Inner>) {
    spawn_refresh_tries(inner, 1, Duration::ZERO);
}

fn spawn_refresh_tries(inner: &Arc<Inner>, tries: u32, gap: Duration) {
    if inner.lock_refreshing.swap(true, Ordering::SeqCst) {
        return;
    }
    let me = inner.clone();
    inner.tracker.spawn(async move {
        for n in 0..tries {
            if n > 0 {
                tokio::select! {
                    () = me.shutdown.cancelled() => break,
                    () = tokio::time::sleep(gap) => {}
                }
            }
            let done = tokio::select! {
                () = me.shutdown.cancelled() => break,
                ok = refresh(&me) => ok,
            };
            if done {
                break;
            }
        }
        me.lock_refreshing.store(false, Ordering::SeqCst);
    });
}

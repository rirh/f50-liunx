//! The boot replay of the saved locks (`lock replay`, `boot-replay`). Not every AT setting survives a modem power
//! cycle, so what was last applied is sent again once per boot.
//!
//! **Early** (K65): from the radio bring-up, after the RIL handshake and with the radio still off, the one safe
//! window. The settings are written, then read back; a failed or silent read-back leaves the marker absent so the
//! late replay can recover. No stack restart: radio-on itself is the activation.
//! **Late**: the boot worker, once the AT channel answers and the radio has come on without an early replay (or
//! not at all within `replay_timeout`: then nothing is replayed this boot, as the shell did). It writes the same
//! and restarts the stack once at the end. The marker lives in the runtime directory: a daemon restart in the same
//! boot does not replay twice. Writes get two seconds each: several vendor setters omit their final OK.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use f50_core::{Error, Result, fsx};

use super::codec::{
    Cell, Kind, LTE_TABLE, bands, canonical, cells_of, filter, lte_bands, lte_full, lte_write, mode_pair, nr_bands,
    nr_masks, nr_read_masks, nr_write,
};
use super::read::{LockReading, cached};
use super::state;
use crate::shared::Inner;
use crate::{Slot, radio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Early,
    Late,
}

fn marker(inner: &Inner) -> PathBuf {
    inner.paths().run().join("lock-replay-done")
}

/// The replay ran (or had nothing to do) at this boot.
pub(crate) fn done(inner: &Inner) -> bool {
    marker(inner).exists()
}

fn mark_done(inner: &Inner) {
    let r = fsx::private_dir(&inner.paths().run()).and_then(|()| fsx::write_atomic(&marker(inner), b"", 0o600));
    if let Err(e) = r {
        tracing::warn!(error = %e, "could not mark the lock replay done");
    }
}

async fn boot_write(inner: &Inner, cmd: &str) {
    let _ = inner.ctl(cmd, 2).await;
}

async fn boot_field(inner: &Inner, cmd: &str, prefix: &str) -> Option<String> {
    inner.read_field(cmd, prefix, 2).await
}

/// One saved setting written, without its own stack restart.
async fn write_quiet(inner: &Inner, kind: Kind, val: &str) {
    match kind {
        Kind::Mode => {
            let (m0, gran) = mode_pair(val);
            let tm = boot_field(inner, "AT+SPTESTMODE?", "+SPTESTMODE").await.unwrap_or_default();
            let f: Vec<&str> = tm.split(',').collect();
            let pick = |i: usize, d: &str| f.get(i).filter(|v| !v.is_empty()).map_or(d.to_owned(), |v| (*v).to_owned());
            boot_write(inner, &format!("AT+SP5GRAN={gran}")).await;
            boot_write(inner, &format!("AT+SPTESTMODE={m0},{},{}", pick(1, "134"), pick(2, "0"))).await;
        }
        Kind::Endc => boot_write(inner, if val == "on" { "AT+SPENDC=1" } else { "AT+SPENDC=2" }).await,
        Kind::Lte => boot_write(inner, &lte_write(&filter(&bands(val), LTE_TABLE))).await,
        Kind::Nr => {
            let sup = LockReading::nr_supported(cached(inner).as_deref());
            boot_write(inner, &nr_write(&filter(&bands(val), &sup))).await;
        }
        Kind::Cell | Kind::AutoApply => {}
    }
}

/// The saved settings read back as written (early phase).
async fn verify(inner: &Inner) -> bool {
    let p = inner.paths();
    if let Some(val) = state::load(p, Kind::Mode) {
        let (m0, gran) = mode_pair(&val);
        let raw = boot_field(inner, "AT+SPTESTMODE?", "+SPTESTMODE").await;
        if raw.as_deref().and_then(|r| r.split(',').next()) != Some(m0.to_string().as_str()) {
            return false;
        }
        if boot_field(inner, "AT+SP5GRAN?", "+SP5GRAN").await != Some(gran.to_string()) {
            return false;
        }
    }
    if let Some(val) = state::load(p, Kind::Endc) {
        let want = if val == "on" { "1" } else { "2" };
        if boot_field(inner, "AT+SPENDC?", "+ENDC").await.as_deref() != Some(want) {
            return false;
        }
    }
    if let Some(val) = state::load(p, Kind::Lte) {
        let want = canonical(&filter(&bands(&val), LTE_TABLE));
        let Some(got) = boot_field(inner, "AT+SPLBAND=0", "+SPLBAND").await.as_deref().and_then(lte_bands) else {
            return false;
        };
        let got = canonical(&got);
        if got != want && !(want.is_empty() && lte_full(&got)) {
            return false;
        }
    }
    if let Some(val) = state::load(p, Kind::Nr) {
        let sup = LockReading::nr_supported(cached(inner).as_deref());
        // SP5GCMDS reports RF capability, but SPLBAND's tables do not encode every capable band (n6): verify the
        // mask that can actually be sent
        let want = canonical(&nr_bands(nr_masks(&filter(&bands(&val), &sup))));
        if want != canonical(&bands(&val)) {
            tracing::info!(saved = %val, "the saved NR set has bands the SPLBAND codec cannot carry");
        }
        let Some(m) = boot_field(inner, "AT+SPLBAND=3", "+SPLBAND").await.as_deref().and_then(nr_read_masks) else {
            return false;
        };
        let got = canonical(&nr_bands(m));
        if got != want && !(want.is_empty() && got == canonical(&sup)) {
            return false;
        }
    }
    if let Some(saved) = state::cells(p) {
        let mut got = Vec::new();
        for (fam, nr) in [(12, false), (16, true)] {
            let Some(raw) = boot_field(inner, &format!("AT+SPFORCEFRQ={fam},3"), "+SPFORCEFRQ").await else {
                return false;
            };
            got.extend(cells_of(&raw, nr).iter().map(ToString::to_string));
        }
        let mut want: Vec<String> = saved.iter().map(ToString::to_string).collect();
        got.sort();
        want.sort();
        if got != want {
            return false;
        }
    }
    true
}

/// The replay itself; the caller holds the radio lock.
pub(crate) async fn run(inner: &Arc<Inner>, phase: Phase) -> Result<()> {
    // the internal card's cold start differs; its replay is not validated (as the shell)
    if inner.slot == Slot::Two || done(inner) {
        return Ok(());
    }
    let p = inner.paths();
    if !state::auto_apply(p) {
        tracing::info!("boot replay of the locks disabled");
        mark_done(inner);
        return Ok(());
    }
    // a reset that committed and was interrupted: its saved settings are not to be replayed
    state::finish_reset(p).map_err(|e| e.context("replay: an interrupted reset's saved settings"))?;
    if !state::any_saved(p) {
        mark_done(inner);
        return Ok(());
    }
    let mut need_restart = false;
    for kind in [Kind::Endc, Kind::Mode, Kind::Lte, Kind::Nr] {
        if let Some(val) = state::load(p, kind) {
            tracing::info!(kind = kind.key(), value = %val, "replay");
            write_quiet(inner, kind, &val).await;
            need_restart |= kind != Kind::Endc;
        }
    }
    if let Some(cells) = state::cells(p) {
        // the modem made to match the saved list exactly, the unlocked state included
        boot_write(inner, "AT+SPFORCEFRQ=12,4").await;
        boot_write(inner, "AT+SPFORCEFRQ=16,4").await;
        for c in &cells {
            boot_write(inner, &Cell::write(c)).await;
        }
        need_restart = true;
    }
    if phase == Phase::Early && !verify(inner).await {
        return Err(Error::failed("early replay read-back failed; the late fallback stays armed"));
    }
    if phase == Phase::Late && need_restart {
        let log = |l: &str| tracing::info!("{l}");
        radio::sfun_restart(inner, phase, &log).await;
    }
    mark_done(inner);
    tracing::info!(?phase, "lock replay done");
    Ok(())
}

/// The boot worker: no saved state or a disabled switch means no AT traffic at all. It waits while the radio
/// bring-up owns the first AT window (the early replay is its business), then replays late.
pub(crate) fn spawn_boot_worker(inner: &Arc<Inner>) {
    let p = inner.paths();
    if !state::auto_apply(p) || !state::any_saved(p) || done(inner) {
        return;
    }
    let me = inner.clone();
    inner.tracker.spawn(async move {
        let end = Instant::now() + me.cfg.replay_timeout;
        let ready = me.at.ready();
        while Instant::now() < end {
            if done(&me) {
                return;
            }
            let go = !me.early_pending.load(Ordering::SeqCst) && *ready.borrow();
            if go {
                let _mark = me.restart_mark();
                let guard = tokio::select! {
                    () = me.shutdown.cancelled() => return,
                    g = radio::acquire(&me, "lock replay", me.cfg.radio_lock_wait) => g,
                };
                match guard {
                    Ok(_g) => {
                        if let Err(e) = run(&me, Phase::Late).await {
                            tracing::warn!(error = %e, "late lock replay failed");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "late lock replay skipped"),
                }
                return;
            }
            tokio::select! {
                () = me.shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        }
        tracing::warn!(secs = me.cfg.replay_timeout.as_secs(), "saved lock replay skipped: the radio did not come on");
    });
}

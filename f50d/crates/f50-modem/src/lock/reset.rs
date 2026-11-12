//! Every lock back to the modem's defaults with ONE stack restart, verified (FINDINGS 52; kanoqwq's 23361ce): the
//! mode query must answer before anything is written, every write's answer is checked, the settings are read back
//! before the restart and again after it, and only then are the saved settings deleted, so the boot replay never
//! puts back a lock the modem still has. The deletion is committed with a tombstone first; an interrupted one is
//! finished by the next apply or replay. A failed reset keeps the saved settings and says which step failed (the
//! modem may hold part of the automatic values by then: a partial write cannot be rolled back).
//!
//! Read-backs verified on the device: `SPTESTMODE 134,134,0`, `SP5GRAN 1`, `SPLBAND=0 -> 0,482,0,149,0` (the full
//! LTE table), `SPLBAND=3 -> 553,0,272` or empty (both automatic), `SPFORCEFRQ=12,3` / `16,3` without a pair.

use std::sync::Arc;
use std::time::Duration;

use f50_core::jobs::JobCtx;
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use super::apply::{read_back, write};
use super::codec::{canonical, lte_bands, lte_full, nr_bands, nr_masks, nr_read_masks};
use super::read::{LockReading, cached};
use super::replay::Phase;
use super::state;
use crate::radio;
use crate::shared::{Inner, lock};

/// The first setting that does not read back as automatic, and what it read.
#[derive(Debug)]
struct Mismatch {
    step: &'static str,
    got: Option<String>,
}

/// Every setting reads back as automatic. "Automatic" bands: an empty mask, or the full supported set the modem
/// reads a cleared mask back as (the LTE table; the NR bands SP5GCMDS lists that the SPLBAND codec can carry: n6
/// is supported and not encodable). A silent read fails too.
async fn verify(inner: &Inner) -> Result<(), Mismatch> {
    let field = |cmd: &'static str, prefix: &'static str| inner.read_field(cmd, prefix, 4);
    let fail = |step, got: Option<String>| Err(Mismatch { step, got });
    let raw = field("AT+SPTESTMODE?", "+SPTESTMODE").await;
    if raw.as_deref().and_then(|r| r.split(',').next()) != Some("134") {
        return fail("mode", raw);
    }
    let raw = field("AT+SP5GRAN?", "+SP5GRAN").await;
    if raw.as_deref() != Some("1") {
        return fail("gran", raw);
    }
    let raw = field("AT+SPENDC?", "+ENDC").await;
    if raw.as_deref() != Some("1") {
        return fail("endc", raw);
    }
    let raw = field("AT+SPLBAND=0", "+SPLBAND").await;
    match raw.as_deref().and_then(lte_bands) {
        Some(b) if b.is_empty() || lte_full(&b) => {}
        _ => return fail("lte", raw),
    }
    let raw = field("AT+SPLBAND=3", "+SPLBAND").await;
    let sup = LockReading::nr_supported(cached(inner).as_deref());
    let want = canonical(&nr_bands(nr_masks(&sup)));
    match raw.as_deref().and_then(nr_read_masks).map(|m| canonical(&nr_bands(m))) {
        Some(got) if got.is_empty() || got == want => {}
        _ => return fail("nr", raw),
    }
    let raw = field("AT+SPFORCEFRQ=12,3", "+SPFORCEFRQ").await;
    if raw.as_deref() != Some("12,3") {
        return fail("cell-lte", raw);
    }
    let raw = field("AT+SPFORCEFRQ=16,3", "+SPFORCEFRQ").await;
    if raw.as_deref() != Some("16,3") {
        return fail("cell-nr", raw);
    }
    Ok(())
}

/// `verify`, again after `nap` while it fails (right after a restart the lock reads lag for a few seconds).
async fn verify_retry(inner: &Inner, tries: u32, nap: Duration) -> Result<(), Mismatch> {
    let mut n = 0;
    loop {
        match verify(inner).await {
            Ok(()) => return Ok(()),
            Err(m) => {
                n += 1;
                if n >= tries {
                    return Err(m);
                }
                tokio::time::sleep(nap).await;
            }
        }
    }
}

fn result(inner: &Inner, state: &str, t0: u64, extra: Value) -> Value {
    let mut v = json!({ "state": state, "ts": t0, "end": unix_now() });
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    *lock(&inner.reset_result) = Some(v.clone());
    v
}

fn failed(inner: &Inner, t0: u64, ctx: &JobCtx, error: &str, step: Option<&str>, got: Option<&str>) -> Error {
    ctx.log(format!("reset failed: {error} ({}: {}); saved settings kept", step.unwrap_or("-"), got.unwrap_or("-")));
    result(inner, "failed", t0, json!({ "error": error, "step": step }));
    Error::failed(error)
}

/// The job of `f50.lock reset`.
pub(crate) async fn reset(inner: Arc<Inner>, ctx: JobCtx) -> Result<Value> {
    let t0 = unix_now();
    result(&inner, "running", t0, json!({}));
    let out = run(&inner, t0, &ctx).await;
    read_back(&inner).await;
    out
}

async fn run(inner: &Arc<Inner>, t0: u64, ctx: &JobCtx) -> Result<Value> {
    if state::finish_reset(inner.paths()).is_err() {
        return Err(failed(inner, t0, ctx, "The saved locks could not be cleared", None, None));
    }
    let guard = radio::acquire(inner, "lock reset", inner.cfg.radio_lock_wait).await.ok();
    let _mark = inner.restart_mark();
    // the mode write keeps the modem's slot 1 and primary fields: they must have been read, and only digits go back
    let tm = inner.read_field("AT+SPTESTMODE?", "+SPTESTMODE", 4).await.unwrap_or_default();
    let f: Vec<&str> = tm.split(',').collect();
    let digits = |i: usize, max: usize| {
        f.get(i).filter(|v| !v.is_empty() && v.len() <= max && v.bytes().all(|b| b.is_ascii_digit())).copied()
    };
    let (Some(_), Some(m1), Some(prim)) = (digits(0, 3), digits(1, 3), digits(2, 1)) else {
        return Err(failed(inner, t0, ctx, "The modem did not answer the mode query; nothing was sent", None, None));
    };
    let mode = format!("AT+SPTESTMODE=134,{m1},{prim}");
    for cmd in [
        "AT+SP5GRAN=1",
        mode.as_str(),
        "AT+SPENDC=1",
        "AT+SPLBAND=1,0,0,0,0,0",
        "AT+SPLBAND=2,0,0,0,0",
        "AT+SPFORCEFRQ=12,4",
        "AT+SPFORCEFRQ=16,4",
    ] {
        if write(inner, cmd, ctx).await.is_err() {
            return Err(failed(inner, t0, ctx, "The modem refused a write", None, Some(cmd)));
        }
    }
    if let Err(m) = verify_retry(inner, 3, Duration::from_secs(1)).await {
        let e = "The read-back does not match the automatic values";
        return Err(failed(inner, t0, ctx, e, Some(m.step), m.got.as_deref()));
    }
    ctx.log("all locks -> automatic, read back; restarting the stack");
    let log = |l: &str| ctx.log(l);
    if guard.is_some() {
        radio::sfun_restart(inner, Phase::Late, &log).await;
    } else {
        ctx.log("the radio stayed busy (dial or watchdog): no stack restart, the values apply at the next one");
    }
    if let Err(m) = verify_retry(inner, 10, Duration::from_secs(3)).await {
        let e = "The read-back after the stack restart does not match the automatic values";
        return Err(failed(inner, t0, ctx, e, Some(m.step), m.got.as_deref()));
    }
    if state::commit_reset(inner.paths()).is_err() {
        return Err(failed(inner, t0, ctx, "The saved locks could not be cleared", None, None));
    }
    ctx.log(format!("reset verified ({}s); saved settings cleared", unix_now().saturating_sub(t0)));
    Ok(result(inner, "ok", t0, json!({})))
}

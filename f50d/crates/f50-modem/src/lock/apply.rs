//! One lock change: the value checked, written, its answer checked, saved for the boot replay, then one stack
//! restart under the radio lock so it takes effect.
//!
//! An explicit ERROR answer is a refusal: nothing saved, no restart (#356: a refused write used to be saved and
//! replayed as applied). No answer is not a refusal: several vendor setters apply the value and omit the OK.
//! A band list the modem supports none of is refused (a lock on n999 used to clear the NR lock and say done). The
//! radio lock is waited for up to `radio_lock_wait`; while a dial or the watchdog keeps it, the setting is written
//! and saved and applies at the radio's next restart.

use std::sync::Arc;
use std::time::Duration;

use f50_core::jobs::JobCtx;
use f50_core::{Code, Error, Result};
use serde_json::{Value, json};

use super::codec::{Cell, Kind, LTE_TABLE, bands, filter, join, lte_write, mode_pair, nr_write};
use super::read::{LockReading, cached, read, store};
use super::replay::Phase;
use super::state;
use crate::collector::engmd::field_f64;
use crate::radio;
use crate::shared::{Inner, refused};

/// One write; a refusal ends the change.
pub(crate) async fn write(inner: &Inner, cmd: &str, ctx: &JobCtx) -> Result<()> {
    let r = inner.ctl(cmd, 6).await;
    if let Some(why) = refused(&r) {
        ctx.log(format!("the modem refused {cmd}: {why}"));
        return Err(Error::failed(format!("The modem refused a write ({cmd})")));
    }
    Ok(())
}

/// The bands kept of a requested list; refused when none is supported, the left-out ones logged.
fn kept(kind: &str, want: &[u32], supported: &[u32], ctx: &JobCtx) -> Result<Vec<u32>> {
    let k = filter(want, supported);
    if !want.is_empty() && k.is_empty() {
        return Err(Error::field(Code::BadValue, "value", "None of these bands is supported by this modem"));
    }
    if k.len() != want.len() {
        ctx.log(format!("{kind}: bands the modem does not support left out of {} (locking {})", join(want), join(&k)));
    }
    Ok(k)
}

/// Refuses a band list the last reading says the modem supports none of (before any job starts).
pub(crate) fn precheck(inner: &Inner, kind: Kind, value: &str) -> Result<()> {
    let want = bands(value);
    if want.is_empty() {
        return Ok(());
    }
    let r = cached(inner);
    let sup = match kind {
        Kind::Lte => LTE_TABLE.to_vec(),
        Kind::Nr => LockReading::nr_supported(r.as_deref()),
        _ => return Ok(()),
    };
    if filter(&want, &sup).is_empty() {
        return Err(Error::field(Code::BadValue, "value", "None of these bands is supported by this modem"));
    }
    Ok(())
}

/// The serving cell of the latest reading, for `cell auto`.
fn serving_cell(inner: &Inner) -> Option<Cell> {
    let snap = inner.snap_tx.borrow().clone();
    let pick = |rat: &str, fr: &str, nr: bool| {
        let c = snap.detail.get(rat)?;
        let arfcn = field_f64(c, fr).filter(|f| *f > 0.0)?;
        let pci = field_f64(c, "pci")?;
        Cell::parse(&format!("{}:{arfcn},{pci}", if nr { "nr" } else { "lte" }))
    };
    pick("nr", "arfcn", true).or_else(|| pick("lte", "earfcn", false))
}

/// Writes one setting and saves it; true when a stack restart is needed for it to take effect.
async fn write_setting(inner: &Arc<Inner>, kind: Kind, value: &str, ctx: &JobCtx) -> Result<(String, bool)> {
    let p = inner.paths();
    let seed: Vec<Cell> =
        cached(inner).map(|r| r.cells.iter().filter_map(|c| Cell::parse(c)).collect()).unwrap_or_default();
    match kind {
        Kind::Mode => {
            let (m0, gran) = mode_pair(value);
            // the slot 1 and primary-SIM fields as the modem reported them
            let tm = inner.read_field("AT+SPTESTMODE?", "+SPTESTMODE", 6).await.unwrap_or_default();
            let f: Vec<&str> = tm.split(',').collect();
            let digits = |s: Option<&&str>| {
                s.filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())).map(|v| (*v).to_owned())
            };
            let m1 = digits(f.get(1)).unwrap_or_else(|| "134".into());
            let prim = digits(f.get(2)).unwrap_or_else(|| "0".into());
            write(inner, &format!("AT+SP5GRAN={gran}"), ctx).await?;
            write(inner, &format!("AT+SPTESTMODE={m0},{m1},{prim}"), ctx).await?;
            ctx.log(format!("network mode -> {value} (SPTESTMODE {m0}, SP5GRAN {gran})"));
            state::save(p, kind, value)?;
            Ok((value.to_owned(), true))
        }
        Kind::Endc => {
            // off is 2 here, not 0 (measured); takes effect without a stack restart
            let v = if value == "on" { 1 } else { 2 };
            write(inner, &format!("AT+SPENDC={v}"), ctx).await?;
            ctx.log(format!("EN-DC -> {value} (SPENDC={v}, no stack restart needed)"));
            state::save(p, kind, value)?;
            if let Some(r) = cached(inner) {
                let mut r = (*r).clone();
                r.endc = Some(value == "on");
                store(inner, r);
            }
            Ok((value.to_owned(), false))
        }
        Kind::Lte => {
            let k = kept("lte", &bands(value), LTE_TABLE, ctx)?;
            write(inner, &lte_write(&k), ctx).await?;
            ctx.log(format!("LTE bands -> {}", if k.is_empty() { "automatic".into() } else { join(&k) }));
            state::save(p, kind, &join(&k))?;
            Ok((join(&k), true))
        }
        Kind::Nr => {
            let sup = LockReading::nr_supported(cached(inner).as_deref());
            let k = kept("nr", &bands(value), &sup, ctx)?;
            write(inner, &nr_write(&k), ctx).await?;
            ctx.log(format!("NR bands -> {}", if k.is_empty() { "automatic".into() } else { join(&k) }));
            state::save(p, kind, &join(&k))?;
            Ok((join(&k), true))
        }
        Kind::Cell => {
            let applied = match value {
                "off" => {
                    write(inner, "AT+SPFORCEFRQ=12,4", ctx).await?;
                    write(inner, "AT+SPFORCEFRQ=16,4", ctx).await?;
                    state::cell_remove(p, &seed, None)?;
                    "off".to_owned()
                }
                "off-nr" => {
                    write(inner, "AT+SPFORCEFRQ=16,4", ctx).await?;
                    state::cell_remove(p, &seed, Some(true))?;
                    "off-nr".to_owned()
                }
                "off-lte" => {
                    write(inner, "AT+SPFORCEFRQ=12,4", ctx).await?;
                    state::cell_remove(p, &seed, Some(false))?;
                    "off-lte".to_owned()
                }
                other => {
                    let c = if other == "auto" {
                        serving_cell(inner).ok_or_else(|| Error::failed("No serving cell to lock to"))?
                    } else {
                        Cell::parse(other).ok_or_else(|| Error::bad_value("value"))?
                    };
                    write(inner, &c.write(), ctx).await?;
                    state::cell_add(p, &seed, c)?;
                    c.to_string()
                }
            };
            ctx.log(format!("cell lock -> {applied}"));
            Ok((applied, true))
        }
        Kind::AutoApply => {
            state::save(p, kind, value)?;
            Ok((value.to_owned(), false))
        }
    }
}

/// The job of `f50.lock set`.
pub(crate) async fn apply(inner: Arc<Inner>, kind: Kind, value: String, ctx: JobCtx) -> Result<Value> {
    // a reset that committed and was interrupted: finished first, or this setting would be deleted with them later
    state::finish_reset(inner.paths()).map_err(|_| Error::failed("The saved locks could not be cleared"))?;
    let guard = match radio::acquire(&inner, "lock apply", inner.cfg.radio_lock_wait).await {
        Ok(g) => Some(g),
        Err(_) => {
            ctx.log("the radio stayed busy (dial or watchdog): no stack restart, the setting applies at the next one");
            None
        }
    };
    let mark = inner.restart_mark();
    let (applied, needs_restart) = write_setting(&inner, kind, &value, &ctx).await?;
    let log = |l: &str| ctx.log(l);
    let restarted = match (&guard, needs_restart) {
        (Some(_), true) => radio::sfun_restart(&inner, Phase::Late, &log).await,
        _ => false,
    };
    drop(mark);
    drop(guard);
    ctx.log(format!("apply {}={applied} done", kind.key()));
    let reading = read_back(&inner).await;
    Ok(json!({ "kind": kind.key(), "value": applied, "restarted": restarted, "lock": reading.as_deref() }))
}

/// The reading after a change: the lock reads lag a restart by a few seconds, so up to ten tries.
pub(crate) async fn read_back(inner: &Arc<Inner>) -> Option<Arc<LockReading>> {
    for n in 0..10 {
        if n > 0 {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        if let Some(r) = read(inner).await {
            store(inner, r);
            return cached(inner);
        }
    }
    cached(inner)
}

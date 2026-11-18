//! `f50.modem`: the readings (served from the latest snapshot with its age), the identity (Write: IMEI, IMSI,
//! ICCID and the own number are private), the own number written to the SIM, the card's radio and the modem
//! restart (jobs).

use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use f50_core::fsx;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::time::{age_ms, unix_now};
use f50_core::{Error, Result};
use serde_json::{Value, json};

use super::radio_job;
use crate::Slot;
use crate::collector::ident::{self, Identity};
use crate::shared::{Inner, lock};
use crate::sim::{self, Cards};
use crate::{radio, sim::state};

const RADIO: &[Field] = &[Field::req("state", Rule::Enum(&["on", "off"]))];
/// An optional "+" and 3 to 15 digits: what `AT+CNUM` gives back, and safe inside the quoted AT argument.
const OWN_NUMBER: &[Field] = &[Field::req("number", Rule::Word { re: r"\+?[0-9]{3,15}", max: 16 })];
const SET_IMEI: &[Field] = &[
    Field::req("slot", Rule::Int { min: 1, max: 2 }),
    Field::secret("imei", Rule::Word { re: "[0-9]{15}", max: 15 }, true),
];

fn status(inner: &Inner) -> Value {
    let s = inner.watched_snapshot();
    json!({
        "registered": s.registered,
        "roaming": s.roaming,
        "rat": s.rat,
        "operator": s.operator,
        "signal": s.signal,
        "bars": s.signal.as_ref().map_or(0, |g| g.bars),
        "cfun": s.detail.get("cfun"),
        "error": s.detail.get("error"),
        "age_ms": s.taken.map(age_ms),
        "slot": inner.slot.number(),
        "sim_state": state::current(inner),
        "radio_allowed": sim::radio_allowed(inner, inner.slot),
        "restarting": inner.is_restarting(),
    })
}

fn cells(inner: &Inner) -> Value {
    let s = inner.watched_snapshot();
    json!({ "age_ms": s.taken.map(age_ms), "detail": s.detail })
}

fn identity(inner: &Inner) -> Value {
    let st = lock(&inner.ident);
    let own = st.own.as_ref();
    let imeis = [Slot::One, Slot::Two].map(|slot| {
        let current = st.shown.as_ref().filter(|_| slot == inner.slot).and_then(|id| id.imei.clone());
        current.or_else(|| ident::load_cache(inner.paths(), slot)?.imei)
    });
    let original_imeis = [Slot::One, Slot::Two].map(|slot| ident::load_original_imei(inner.paths(), slot).ok().flatten());
    json!({
        "slot": inner.slot.number(),
        "ident": st.shown,
        "imeis": imeis,
        "original_imeis": original_imeis,
        "msisdn": own.map(|o| o.0.clone()),
        "msisdn_src": own.map(|o| o.1),
    })
}

/// The card in use's radio, kept across reboots (FINDINGS 185): off is the card's stack off (`AT+SFUN=5`,
/// FINDINGS 202), on brings the radio and the data back.
pub(super) fn set_radio(inner: &Arc<Inner>, cards: Cards, on: bool) -> Result<Value> {
    sim::check_radio_change(inner)?;
    let now = sim::set_radio(inner, cards, on)?;
    if !now {
        return Ok(json!({ "saved": true, "applies": "when that card next starts" }));
    }
    let job = radio_job(inner, "f50.modem radio", |me, ctx| async move {
        let _g = radio::acquire(&me, "radio switch", me.cfg.radio_lock_wait).await?;
        let _mark = me.restart_mark();
        sim::rf_apply(&me, &|l: &str| ctx.log(l)).await
    })?;
    Ok(json!({ "saved": true, "job": job["job"] }))
}

/// The SIM's own number, written to the card (a job: it waits for the radio lock and the AT control channel).
fn own_number(inner: &Arc<Inner>, number: String) -> Result<Value> {
    radio_job(inner, "f50.modem own_number", move |me, ctx| async move {
        let _g = radio::acquire(&me, "own number", me.cfg.radio_lock_wait).await?;
        crate::own::write(&me, &number, &|l: &str| ctx.log(l)).await
    })
}

fn set_imei(inner: &Arc<Inner>, p: &Params) -> Result<Value> {
    if p.need_int("slot")? != i64::from(inner.slot.number()) {
        return Err(Error::forbidden("only the active slot can be edited; select the slot and reboot first"));
    }
    let imei = p.need_str("imei")?.to_owned();
    if !ident::valid_imei(&imei) {
        return Err(Error::bad_value("imei"));
    }
    radio_job(inner, "f50.modem set_imei", move |me, ctx| async move {
        let _g = radio::acquire(&me, "IMEI write", me.cfg.radio_lock_wait).await?;
        let _mark = me.restart_mark();
        let _update = me.ident_update.lock().await;
        let mut at = me.at.session("IMEI write", Duration::from_secs(10)).await?;
        if ident::load_original_imei(me.paths(), me.slot)?.is_none() {
            let before = at.send("AT+CGSN", Duration::from_secs(5)).await?;
            let original = ident::modem_imei(&before)
                .ok_or_else(|| Error::failed("could not read the original IMEI before writing"))?;
            ident::save_original_imei(me.paths(), me.slot, &original)?;
        }
        let mut id = lock(&me.ident)
            .shown
            .clone()
            .or_else(|| ident::load_cache(me.paths(), me.slot))
            .unwrap_or_else(|| Identity { slot: me.slot.number(), ts: unix_now(), ..Identity::default() });

        // Invalidate the old observation before writing, including if the modem reboots or the write times out.
        let path = ident::cache_path(me.paths(), me.slot);
        if let Err(e) = std::fs::remove_file(&path) && e.kind() != ErrorKind::NotFound {
            return Err(Error::io(path.display(), &e));
        }
        // Commit the original's directory entry (including a previous rename) and the cache removal before the write.
        let state_dir = me.paths().state();
        fsx::sync_dir(&state_dir)?;
        if let Some(parent) = state_dir.parent() {
            fsx::sync_dir(parent)?;
        }
        if let Some(shown) = lock(&me.ident).shown.as_mut() {
            shown.imei = None;
        }

        let command = format!("AT+SPIMEI={},\"{imei}\"", me.slot.index());
        let written = at.send(&command, Duration::from_secs(8)).await;
        let read = at.send("AT+CGSN", Duration::from_secs(5)).await;
        drop(at);
        id.imei = read.as_ref().ok().and_then(ident::modem_imei);
        let verified = id.imei.as_deref() == Some(imei.as_str());
        let mut state = lock(&me.ident);
        if id.imei.is_some() {
            ident::save_cache(me.paths(), me.slot, &id);
        }
        state.shown = (!id.empty()).then_some(id);
        state.read_at = None;

        if verified {
            ctx.log("IMEI read back from the active slot");
            return Ok(json!({ "slot": me.slot.number(), "verified": true }));
        }
        if written.as_ref().is_ok_and(|reply| !reply.is_ok()) {
            return Err(Error::failed("the modem rejected the IMEI write"));
        }
        Err(Error::failed("the new IMEI could not be confirmed by AT+CGSN; check the active slot before retrying"))
    })
}

fn restart(inner: &Arc<Inner>) -> Result<Value> {
    if !sim::radio_allowed(inner, inner.slot) {
        return Err(Error::forbidden("the radio of this SIM is switched off in its settings"));
    }
    radio_job(inner, "f50.modem restart", |me, ctx| async move {
        let _g = radio::acquire(&me, "modem restart", me.cfg.radio_lock_wait).await?;
        let _mark = me.restart_mark();
        radio::modem_reset(&me, &|l: &str| ctx.log(l)).await
    })
}

pub(super) fn object(inner: &Arc<Inner>) -> Object {
    let (a, b, c, d, e, f) = (inner.clone(), inner.clone(), inner.clone(), inner.clone(), inner.clone(), inner.clone());
    let imei = inner.clone();
    Object::builder("f50.modem")
        .method(
            "status",
            Access::Read,
            &[],
            "Registration, RAT, operator, signal and bars from the latest reading, with its age.",
            move |_p: Params| {
                let r = status(&a);
                async move { Ok(r) }
            },
        )
        .method(
            "cells",
            Access::Read,
            &[],
            "Serving cells, neighbours, carrier aggregation and QoS from the latest reading, with its age.",
            move |_p: Params| {
                let r = cells(&b);
                async move { Ok(r) }
            },
        )
        .method(
            "identity",
            Access::Write,
            &[],
            "Identity for the active slot and the original and last-read IMEIs for SIM1 and SIM2.",
            move |_p: Params| {
                let r = identity(&c);
                async move { Ok(r) }
            },
        )
        .method(
            "set_imei",
            Access::Write,
            SET_IMEI,
            "Writes the active slot's IMEI and reads it back (a job).",
            move |p: Params| {
                let r = set_imei(&imei, &p);
                async move { r }
            },
        )
        .method(
            "radio",
            Access::Write,
            RADIO,
            "Switches the radio of the card in use on or off, kept across reboots (a job when it changes now).",
            move |p: Params| {
                let r = set_radio(&d, Cards::One(d.slot), p.str("state") == Some("on"));
                async move { r }
            },
        )
        .method(
            "own_number",
            Access::Write,
            OWN_NUMBER,
            "Writes the SIM's own number to the card (its own-number phonebook, read by AT+CNUM; a job).",
            move |p: Params| {
                let r = own_number(&f, p.str("number").unwrap_or_default().to_owned());
                async move { r }
            },
        )
        .method("restart", Access::Write, &[], "Restarts the modem's protocol stack (a job).", move |_p: Params| {
            let r = restart(&e);
            async move { r }
        })
        .build()
}

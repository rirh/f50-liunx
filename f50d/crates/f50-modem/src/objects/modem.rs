//! `f50.modem`: the readings (served from the latest snapshot with its age), the identity (Write: IMEI, IMSI,
//! ICCID and the own number are private), the own number written to the SIM, the card's radio and the modem
//! restart (jobs).

use std::sync::Arc;

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::time::age_ms;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use super::radio_job;
use crate::shared::{Inner, lock};
use crate::sim::{self, Cards};
use crate::{radio, sim::state};

const RADIO: &[Field] = &[Field::req("state", Rule::Enum(&["on", "off"]))];
/// An optional "+" and 3 to 15 digits: what `AT+CNUM` gives back, and safe inside the quoted AT argument.
const OWN_NUMBER: &[Field] = &[Field::req("number", Rule::Word { re: r"\+?[0-9]{3,15}", max: 16 })];

fn status(inner: &Inner) -> Value {
    let s = inner.snap_tx.borrow().clone();
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
    let s = inner.snap_tx.borrow().clone();
    json!({ "age_ms": s.taken.map(age_ms), "detail": s.detail })
}

fn identity(inner: &Inner) -> Value {
    let st = lock(&inner.ident);
    let own = st.own.as_ref();
    json!({
        "slot": inner.slot.number(),
        "ident": st.shown,
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
    let (a, b, c, d, e, f) =
        (inner.clone(), inner.clone(), inner.clone(), inner.clone(), inner.clone(), inner.clone());
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
            "IMEI, IMSI, ICCID, model, firmware and the SIM's own number.",
            move |_p: Params| {
                let r = identity(&c);
                async move { Ok(r) }
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

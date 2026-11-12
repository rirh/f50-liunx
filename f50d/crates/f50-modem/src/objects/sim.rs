//! `f50.sim`: the slot in use and the next boot's, the card's state, each card's radio setting.
//!
//! On the F50 a selection is for the next boot (FINDINGS 74): `select` saves it and says a reboot is required.

use std::sync::Arc;

use f50_core::Result;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::{Value, json};

use super::modem::set_radio;
use super::radio_job;
use crate::Slot;
use crate::shared::Inner;
use crate::sim::{self, Cards};

const SELECT: &[Field] = &[Field::req("slot", Rule::Int { min: 1, max: 2 })];
const CARD: &[Field] =
    &[Field::req("slot", Rule::Enum(&["1", "2", "all"])), Field::req("radio", Rule::Enum(&["on", "off"]))];

fn select(inner: &Arc<Inner>, slot: Slot) -> Result<Value> {
    radio_job(inner, "f50.sim select", move |me, ctx| async move {
        sim::save_slot(me.paths(), slot)?;
        ctx.log(format!(
            "SIM{} saved for the next boot; SIM{} stays in use until then",
            slot.number(),
            me.slot.number()
        ));
        Ok(json!({
            "active": me.slot.number(),
            "default": slot.number(),
            "reboot_required": slot != me.slot,
        }))
    })
}

fn card(inner: &Arc<Inner>, p: &Params) -> Result<Value> {
    let cards = match p.str("slot") {
        Some("1") => Cards::One(Slot::One),
        Some("2") => Cards::One(Slot::Two),
        _ => Cards::All,
    };
    set_radio(inner, cards, p.str("radio") == Some("on"))
}

pub(super) fn object(inner: &Arc<Inner>) -> Object {
    let (a, b, c) = (inner.clone(), inner.clone(), inner.clone());
    Object::builder("f50.sim")
        .method(
            "get",
            Access::Read,
            &[],
            "The card in use, the next boot's, the card's state and both cards' radio settings.",
            move |_p: Params| {
                let r = sim::status(&a);
                async move { Ok(r) }
            },
        )
        .method("select", Access::Write, SELECT, "Chooses the card for the next boot (a job).", move |p: Params| {
            let r = Slot::from_number(p.int("slot").unwrap_or(1))
                .ok_or_else(|| f50_core::Error::bad_value("slot"))
                .and_then(|s| select(&b, s));
            async move { r }
        })
        .method(
            "card",
            Access::Write,
            CARD,
            "A card's radio on or off (or all: the persistent airplane mode), kept across reboots.",
            move |p: Params| {
                let r = card(&c, &p);
                async move { r }
            },
        )
        .build()
}

//! `f50.lock`: the cached lock reading, one change, everything back to automatic.

use std::sync::Arc;

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::{Error, Result};
use serde_json::{Value, json};

use super::radio_job;
use crate::lock::codec::{self, Kind};
use crate::lock::{apply, reset, state};
use crate::shared::Inner;
use crate::{Slot, lock};

const GET: &[Field] = &[Field::opt("fresh", Rule::Bool)];
const SET: &[Field] =
    &[Field::req("kind", Rule::Enum(Kind::WORDS)), Field::req("value", Rule::Word { re: "[a-z0-9:,_-]*", max: 127 })];

fn refuse_sim2(inner: &Inner) -> Result<()> {
    if inner.slot == Slot::Two {
        return Err(Error::unsupported("band, mode and cell locks for SIM2"));
    }
    Ok(())
}

fn set(inner: &Arc<Inner>, p: &Params) -> Result<Value> {
    let kind = p.str("kind").and_then(Kind::parse).ok_or_else(|| Error::bad_value("kind"))?;
    let value = p.str("value").unwrap_or("").to_owned();
    codec::validate(kind, &value)?;
    // the replay switch only: no AT, no restart, never queued behind a running apply
    if kind == Kind::AutoApply {
        state::save(inner.paths(), kind, &value)?;
        return Ok(json!({ "auto_apply": value == "on" }));
    }
    refuse_sim2(inner)?;
    apply::precheck(inner, kind, &value)?;
    let queued = inner.is_restarting();
    let mut r = radio_job(inner, "f50.lock set", move |me, ctx| apply::apply(me, kind, value, ctx))?;
    r["queued"] = json!(queued);
    Ok(r)
}

pub(super) fn object(inner: &Arc<Inner>) -> Object {
    let (a, b, c) = (inner.clone(), inner.clone(), inner.clone());
    Object::builder("f50.lock")
        .method(
            "get",
            Access::Read,
            GET,
            "The network mode, band, cell and EN-DC locks as last read, the replay switch and the last reset.",
            move |p: Params| {
                let r = lock::get(&a, p.flag("fresh"));
                async move { Ok(r) }
            },
        )
        .method(
            "set",
            Access::Write,
            SET,
            "One lock change (mode, endc, lte, nr, cell; auto_apply at once), then one stack restart (a job).",
            move |p: Params| {
                let r = set(&b, &p);
                async move { r }
            },
        )
        .method("reset", Access::Write, &[], "Every lock back to automatic, verified (a job).", move |_p: Params| {
            let r = refuse_sim2(&c).and_then(|()| radio_job(&c, "f50.lock reset", reset::reset));
            async move { r }
        })
        .build()
}

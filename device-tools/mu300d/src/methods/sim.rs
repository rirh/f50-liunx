//! sim.*, lock.*

use std::fs;

use serde_json::{Value, json};

use super::{argv, json_of, start_job};
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule, check_word};
use crate::exec::{Tool, executable, spawn_detached};
use crate::jobs::{JobSpec, alive};
use crate::m;
use crate::sys::Ctx;

const SLOT: &[Field] = &[Field::req("slot", Rule::Int { min: 1, max: 2 })];
const FRESH: &[Field] = &[Field::opt("fresh", Rule::Bool)];
const LOCK_SET: &[Field] = &[
    Field::req("kind", Rule::Enum(&["mode", "endc", "lte", "nr", "cell", "reset", "auto_apply"])),
    Field::opt("val", Rule::Scalar { max: 160 }),
];

pub static METHODS: &[Method] = &[
    m!("sim.get", Read, &[], get),
    m!("sim.select", Write, SLOT, select),
    m!("sim.state", Read, FRESH, state),
    m!("lock.get", Read, FRESH, lock_get),
    m!("lock.set", Write, LOCK_SET, lock_set),
];

async fn get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let o = ctx.need_tool("mu300-sim")?.args(["status", "--json"]).timeout_secs(15).ok().await?;
    Ok(json!({ "sim": json_of(&o, "mu300-sim")? }))
}

/// The card for data (mu300-sim default N): a SIM switch restarts the protocol stack, a job.
async fn select(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let slot = p.int("slot").unwrap_or(1).to_string();
    let t = ctx.need_tool("mu300-sim")?;
    let spec = JobSpec::new("sim.select", "modem", 90, argv(t.program(), &["default", &slot])).not_cancellable();
    start_job(&ctx, spec)
}

fn word_of(out: &str) -> String {
    out.lines().next().unwrap_or("").chars().filter(char::is_ascii_lowercase).collect()
}

/// What the card in use says of itself: ready, locked or absent (the answer kept for this boot, or AT+CPIN? when
/// there is none or `fresh`), and the slot in use.
async fn state(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let t = || ctx.need_tool("mu300-sim-state");
    t()?;
    let mut st = String::new();
    if !p.flag("fresh")
        && let Ok(o) = t()?.arg("get").timeout_secs(10).run().await
    {
        st = word_of(&o.stdout);
    }
    if st.is_empty()
        && let Ok(o) = t()?.arg("check").timeout_secs(15).run().await
    {
        st = word_of(&o.stdout);
    }
    let st = matches!(st.as_str(), "absent" | "ready" | "locked").then_some(st);
    let mut slot = None;
    if ctx.has_tool("mu300-sim") {
        let a = ctx.line_of(ctx.tool("mu300-sim").arg("active")).await;
        slot = match a.as_str() {
            "0" => Some(1),
            "1" => Some(2),
            _ => None,
        };
    }
    Ok(json!({ "state": st, "slot": slot }))
}

fn lock_bin(ctx: &Ctx) -> ApiResult<Tool> {
    if executable(&ctx.cfg.lock_bin) {
        Ok(Tool::new(&ctx.cfg, &ctx.cfg.lock_bin))
    } else {
        Err(ApiError::unsupported("locks"))
    }
}

/// A lock reading (eight AT commands) is running: the lock program's pid in its lock directory.
fn lock_reading(ctx: &Ctx) -> bool {
    fs::read_to_string(ctx.cfg.dash_dir.join("lock-read.lck/pid"))
        .ok()
        .and_then(|p| p.trim().parse::<i32>().ok())
        .is_some_and(alive)
}

/// The network, band and cell locks: the lock program's cache, never the modem inside the request. A missing cache
/// or `fresh` starts a reading in the background (`refreshing`); ask again for its result.
async fn lock_get(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let t = lock_bin(&ctx)?;
    let cache = ctx.cfg.dash_dir.join("lock.json");
    let have = fs::metadata(&cache).is_ok_and(|m| m.len() > 0);
    let mut refreshing = lock_reading(&ctx);
    if (p.flag("fresh") || !have) && !refreshing {
        let a = [ctx.cfg.lock_bin.clone().into_os_string(), "refresh".into()];
        refreshing = spawn_detached(&ctx.cfg, &a, &[]).is_ok();
    }
    if !have {
        return Ok(json!({ "lock": null, "refreshing": refreshing }));
    }
    let o = t.args(["get", "cached"]).timeout_secs(10).ok().await?;
    let v = serde_json::from_str::<Value>(o.stdout.trim())
        .map_err(|_| ApiError::failed("the lock program gave no answer"))?;
    Ok(json!({ "lock": v, "refreshing": refreshing }))
}

/// The value rule for each lock kind.
fn lock_val_rule(kind: &str) -> &'static str {
    match kind {
        "mode" => "auto|auto152|4g|sa|nsa",
        "endc" | "auto_apply" => "on|off",
        "lte" | "nr" => "([0-9]{1,3}(,[0-9]{1,3}){0,31})?",
        "cell" => "auto|off|off-nr|off-lte|(nr|lte):[0-9]{1,7},[0-9]{1,4}",
        _ => "auto",
    }
}

/// A lock setting, applied by the lock program: it restarts the protocol stack (half a minute without cellular).
async fn lock_set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let kind = p.str("kind").unwrap_or_default();
    let val = p.str("val").unwrap_or("");
    if val.is_empty() && kind != "lte" && kind != "nr" {
        return Err(ApiError::missing("val"));
    }
    check_word("val", val, lock_val_rule(kind), 160)?;
    let t = lock_bin(&ctx)?;
    let args: Vec<&str> = if kind == "reset" { vec!["reset"] } else { vec!["apply", kind, val] };
    let spec = JobSpec::new("lock.set", "modem", 240, argv(t.program(), &args)).not_cancellable();
    start_job(&ctx, spec)
}

#[cfg(test)]
mod tests {
    use crate::api::validate::matches;

    #[test]
    fn lock_rules() {
        let r = super::lock_val_rule;
        assert!(matches(r("lte"), ""));
        assert!(matches(r("lte"), "1,3,7,20"));
        assert!(!matches(r("lte"), "1,,3"));
        assert!(matches(r("cell"), "nr:627264,123"));
        assert!(!matches(r("cell"), "nr:627264"));
        assert!(matches(r("reset"), "auto"));
        assert!(!matches(r("mode"), "5g"));
    }
}

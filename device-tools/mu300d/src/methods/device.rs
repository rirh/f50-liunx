//! cpu.*, update.*

use serde_json::{Value, json};

use super::{argv, json_of, json_or_null, ok, start_job, text_of};
use crate::api::error::ApiResult;
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule};
use crate::jobs::JobSpec;
use crate::m;
use crate::sys::Ctx;

const PROFILE: &[Field] = &[Field::req("name", Rule::Enum(&["saving", "balanced", "performance"]))];

pub static METHODS: &[Method] = &[
    m!("cpu.get", Read, &[], cpu_get),
    m!("cpu.profile", Write, PROFILE, cpu_profile),
    m!("update.status", Read, &[], update_status),
    m!("update.check", Write, &[], update_check),
    m!("update.apply", Write, &[], update_apply),
];

async fn cpu_get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let o = ctx.need_tool("mu300-cpu")?.args(["status", "--json"]).timeout_secs(15).ok().await?;
    let cpu = json_of(&o, "mu300-cpu")?;
    let v = ctx.tool("mu300-cpu").args(["voltage", "status", "--json"]).timeout_secs(15).run().await;
    Ok(json!({ "cpu": cpu, "voltage": json_or_null(&v) }))
}

async fn cpu_profile(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let name = p.str("name").unwrap_or_default();
    ctx.need_tool("mu300-cpu")?.args(["profile", name]).timeout_secs(15).ok().await?;
    ok()
}

/// mu300-update status (the systems, their releases, the kernel, free space), its release source and version; no
/// network.
async fn update_status(ctx: Ctx, _: Params) -> ApiResult<Value> {
    ctx.need_tool("mu300-update")?;
    let u = || ctx.tool("mu300-update").timeout_secs(15);
    let status = json_or_null(&u().arg("status").run().await);
    let source = u().arg("source").output().await.map(|o| o.stdout.trim().to_owned()).unwrap_or_default();
    let version = u().arg("version").output().await.map(|o| text_of(&o).trim().to_owned()).unwrap_or_default();
    // a mirror's address is the user's own setting and carries no token: shown as it is, control characters out
    let source: String = source.chars().filter(|c| !c.is_control()).collect();
    Ok(json!({ "status": status, "source": source, "version": version }))
}

async fn update_check(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let t = ctx.need_tool("mu300-update")?;
    start_job(&ctx, JobSpec::new("update.check", "update", 300, argv(t.program(), &["check"])))
}

/// Never stopped half way: an update's writes.
async fn update_apply(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let t = ctx.need_tool("mu300-update")?;
    start_job(&ctx, JobSpec::new("update.apply", "update", 7200, argv(t.program(), &["apply"])).not_cancellable())
}

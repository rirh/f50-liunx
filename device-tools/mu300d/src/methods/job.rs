//! job.status, job.cancel, job.list

use serde_json::Value;

use crate::api::error::ApiResult;
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule};
use crate::jobs::{self, ID_RE};
use crate::m;
use crate::sys::Ctx;

const ID: &[Field] = &[Field::req("id", Rule::Word { re: ID_RE, max: 64 })];

pub static METHODS: &[Method] =
    &[m!("job.status", Read, ID, status), m!("job.cancel", Write, ID, cancel), m!("job.list", Read, &[], list)];

async fn status(ctx: Ctx, p: Params) -> ApiResult<Value> {
    jobs::status(&ctx.cfg, p.str("id").unwrap_or_default())
}

async fn cancel(ctx: Ctx, p: Params) -> ApiResult<Value> {
    jobs::cancel(&ctx.cfg, p.str("id").unwrap_or_default())
}

async fn list(ctx: Ctx, _: Params) -> ApiResult<Value> {
    Ok(jobs::list(&ctx.cfg))
}

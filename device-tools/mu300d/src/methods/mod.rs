//! The methods, one module per area. Each mirrors mu300-api's `m_<name>`: the same fields, the same tools with the
//! same arguments, the answer under `data`.

pub mod data;
pub mod debug;
pub mod device;
pub mod job;
pub mod lan;
pub mod sim;
pub mod sms;
pub mod status;
pub mod system;
pub mod wifi;
pub mod wifi_channels;

use std::ffi::OsString;
use std::path::Path;

use serde_json::{Value, json};

use crate::api::error::{ApiError, ApiResult};
use crate::exec::Output;
use crate::jobs::{self, JobSpec};
use crate::sanitize::sanitize_text;
use crate::sys::Ctx;

/// The tool's stdout as one JSON value, or `failed`.
pub(crate) fn json_of(o: &Output, what: &str) -> ApiResult<Value> {
    serde_json::from_str::<Value>(o.stdout.trim()).map_err(|_| ApiError::failed(format!("{what} gave no JSON")))
}

/// The tool's stdout as JSON, or null.
pub(crate) fn json_or_null(o: &std::io::Result<Output>) -> Value {
    o.as_ref().ok().and_then(|o| serde_json::from_str(o.stdout.trim()).ok()).unwrap_or(Value::Null)
}

/// The tool's stdout as text for a reply (control characters and URL tokens out).
pub(crate) fn text_of(o: &Output) -> String {
    sanitize_text(&o.stdout.replace('\r', ""))
}

pub(crate) fn argv(program: &Path, args: &[&str]) -> Vec<OsString> {
    std::iter::once(program.as_os_str().to_owned()).chain(args.iter().map(OsString::from)).collect()
}

/// Start a job and answer `{"job": id}`.
pub(crate) fn start_job(ctx: &Ctx, spec: JobSpec) -> ApiResult<Value> {
    let id = jobs::start(&ctx.cfg, spec)?;
    Ok(json!({ "job": id }))
}

pub(crate) fn ok() -> ApiResult<Value> {
    Ok(json!({}))
}

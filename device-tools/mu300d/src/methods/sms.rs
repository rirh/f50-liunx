//! sms.*: the SIM's messages through one of two tools.
//!
//! * `cli`: /opt/mu300/bin/sms (Ubuntu, plain OpenWrt): `sms json`, `sms read ID`, `sms send --stdin` (number and
//!   text on stdin), `sms delete ID|all`. This is mu300-api's path.
//! * `pool`: mu300-sms (openwrt-luci: the control panel's message pool): `list PAGE`, `show ID`,
//!   `send --stdin NUMBER` (text on stdin), `delete ID [--sim]`. This is mu300dash's path, which mu300-api used on
//!   openwrt-luci.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::{json_of, ok, start_job, text_of};
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule};
use crate::exec::{Tool, executable};
use crate::jobs::JobSpec;
use crate::m;
use crate::sys::{Ctx, first_line};

const SMS_ID: &str = "[0-9]{1,6}";
const LIST: &[Field] = &[Field::opt("page", Rule::Int { min: 1, max: 9999 })];
const READ: &[Field] = &[Field::req("id", Rule::Word { re: SMS_ID, max: 6 })];
const SEND: &[Field] = &[
    Field::req("number", Rule::Word { re: r"\+?[0-9]{1,20}", max: 21 }),
    Field::secret("text", Rule::Body { max_chars: 480 }, true),
];
const DELETE: &[Field] =
    &[Field::req("id", Rule::Word { re: "all|[0-9]{1,6}", max: 6 }), Field::opt("sim", Rule::Bool)];

pub static METHODS: &[Method] = &[
    m!("sms.list", Read, LIST, list),
    m!("sms.read", Read, READ, read),
    m!("sms.send", Write, SEND, send),
    m!("sms.delete", Write, DELETE, delete),
];

pub enum Backend {
    Cli(PathBuf),
    Pool(PathBuf),
}

impl Backend {
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Cli(_) => "cli",
            Backend::Pool(_) => "pool",
        }
    }
}

/// openwrt-luci's pool when it is there, else the sms CLI, else the pool wherever it is installed.
pub fn backend(ctx: &Ctx) -> Option<Backend> {
    let cli = ctx.cfg.tool("sms");
    let pool = [ctx.cfg.tool("mu300-sms"), ctx.cfg.sys("/usr/bin/mu300-sms")].into_iter().find(|p| executable(p));
    match (ctx.sys.is_luci(), pool, executable(&cli)) {
        (true, Some(p), _) => Some(Backend::Pool(p)),
        (_, _, true) => Some(Backend::Cli(cli)),
        (_, Some(p), false) => Some(Backend::Pool(p)),
        _ => None,
    }
}

fn need(ctx: &Ctx) -> ApiResult<Backend> {
    backend(ctx).ok_or_else(|| ApiError::unsupported("sms"))
}

fn trimmed(s: &str, from: usize, len: Option<usize>) -> String {
    let it = s.chars().skip(from);
    let t: String = match len {
        Some(n) => it.take(n).collect(),
        None => it.collect(),
    };
    t.trim().to_owned()
}

/// mu300-sms's list: "pool: N message(s), U unread - page P/T (10 per page)", then fixed-width rows.
pub(crate) fn parse_pool_list(out: &str) -> Value {
    let mut lines = out.lines();
    let head = lines.next().unwrap_or("");
    let num_after = |s: &str, pre: &str| -> Option<u64> {
        let r = &s[s.find(pre)? + pre.len()..];
        r.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
    };
    let total = num_after(head, "pool: ").unwrap_or(0);
    let unread = num_after(head, "), ").unwrap_or(0);
    let page = num_after(head, "page ").unwrap_or(1);
    let pages = head.split_once("page ").and_then(|(_, r)| num_after(r, "/")).unwrap_or(1);
    let msgs: Vec<Value> = lines
        .filter_map(|l| {
            let id = trimmed(l, 0, Some(7));
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some(json!({
                "id": id,
                "status": trimmed(l, 8, Some(7)),
                "dir": trimmed(l, 16, Some(3)),
                "peer": trimmed(l, 20, Some(18)),
                "time": trimmed(l, 39, Some(21)),
                "preview": crate::sanitize::sanitize_text(&trimmed(l, 61, None)),
            }))
        })
        .collect();
    json!({"total": total, "unread": unread, "page": page, "pages": pages, "msgs": msgs})
}

/// "STORE USED TOTAL EPOCH" as the last sync left it, else null.
fn storage_note(ctx: &Ctx) -> Value {
    let l = first_line(&ctx.cfg.sys("/run/mu300-sms-storage"));
    let f: Vec<&str> = l.split(' ').collect();
    if f.len() == 4 && (f[0] == "SM" || f[0] == "ME") {
        let n: Vec<u64> = f[1..].iter().filter_map(|x| x.parse().ok()).collect();
        if n.len() == 3 {
            return json!({"store": f[0], "used": n[0], "total": n[1], "at": n[2]});
        }
    }
    Value::Null
}

async fn list(ctx: Ctx, p: Params) -> ApiResult<Value> {
    match need(&ctx)? {
        Backend::Cli(b) => {
            let o = Tool::new(&ctx.cfg, b).arg("json").timeout_secs(30).ok().await?;
            Ok(json!({ "backend": "cli", "messages": json_of(&o, "sms")? }))
        }
        Backend::Pool(b) => {
            let page = p.int("page").unwrap_or(1).to_string();
            let o = Tool::new(&ctx.cfg, b).args(["list", &page]).timeout_secs(20).ok().await?;
            if o.stdout.trim().is_empty() {
                return Err(ApiError::failed("mu300-sms gave no list"));
            }
            let mut v = parse_pool_list(&o.stdout);
            v["backend"] = json!("pool");
            v["sim_storage"] = storage_note(&ctx);
            Ok(v)
        }
    }
}

async fn read(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let id = p.str("id").unwrap_or_default();
    let t = match need(&ctx)? {
        Backend::Cli(b) => Tool::new(&ctx.cfg, b).args(["read", id]),
        Backend::Pool(b) => Tool::new(&ctx.cfg, b).args(["show", id]),
    };
    let o = t.timeout_secs(20).ok().await?;
    Ok(json!({ "id": id, "text": text_of(&o) }))
}

/// A send waits up to 40 s for the modem's +CMGS: a job, one at a time, never submitted twice by itself. The number
/// and the text go on the tool's stdin (the pool takes the number as an argument: digits only).
async fn send(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let number = p.str("number").unwrap_or_default();
    let text = p.str("text").unwrap_or_default();
    let spec = match need(&ctx)? {
        Backend::Cli(b) => JobSpec::new("sms.send", "sms", 200, super::argv(&b, &["send", "--stdin"]))
            .stdin(format!("{number}\n{text}")),
        Backend::Pool(b) => JobSpec::new("sms.send", "sms", 200, super::argv(&b, &["send", "--stdin", number]))
            .stdin(text.to_owned())
            .env("MU300_AT_LOCK_WAIT", "3"),
    };
    start_job(&ctx, spec.not_cancellable())
}

async fn delete(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let id = p.str("id").unwrap_or_default();
    let t = match need(&ctx)? {
        Backend::Cli(b) => Tool::new(&ctx.cfg, b).args(["delete", id]),
        Backend::Pool(b) => {
            let t = Tool::new(&ctx.cfg, b).args(["delete", id]);
            if p.flag("sim") { t.arg("--sim") } else { t }
        }
    };
    t.timeout_secs(30).ok().await?;
    ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn pool_list() {
        let out = "pool: 12 message(s), 3 unread - page 2/2 (10 per page)\n\
                   12      unread  in  +905551112233      2026-11-12 10:00:01   Merhaba dünya\n\
                   garbage\n";
        let v = super::parse_pool_list(out);
        assert_eq!(v["total"], 12);
        assert_eq!(v["unread"], 3);
        assert_eq!(v["page"], 2);
        assert_eq!(v["pages"], 2);
        assert_eq!(v["msgs"].as_array().unwrap().len(), 1);
        assert_eq!(v["msgs"][0]["id"], "12");
        assert_eq!(v["msgs"][0]["peer"], "+905551112233");
        assert_eq!(v["msgs"][0]["preview"], "Merhaba dünya");
    }
}

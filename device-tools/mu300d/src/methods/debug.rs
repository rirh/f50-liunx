//! at.send, at.history: the AT terminal, behind mu300dash's guards.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

use serde_json::{Value, json};

use super::{argv, start_job, text_of};
use crate::api::error::{ApiError, ApiResult, Code};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule};
use crate::at_guard::{self, Class, Refusal};
use crate::cache::modem_restarting;
use crate::jobs::{self, JobSpec};
use crate::m;
use crate::sanitize::sanitize_text;
use crate::sys::Ctx;

/// The longest reply wait inside a request; a longer `timeout` (or a scan) runs as a job.
const SYNC_MAX: i64 = 8;
/// A scan's reply wait (mu300dash's AT_JOB_T).
const SCAN_T: i64 = 99;
const HISTORY_MAX: usize = 40;

const SEND: &[Field] = &[
    // a command may carry a PIN: it is a secret, on mu300-at's stdin, and kept redacted
    Field::secret("command", Rule::Text { max_bytes: 512, max_chars: 512 }, true),
    Field::opt("timeout", Rule::Int { min: 1, max: 60 }),
    Field::opt("confirm", Rule::Bool),
];

pub static METHODS: &[Method] = &[m!("at.send", Write, SEND, send), m!("at.history", Read, &[], history)];

fn refusal(r: Refusal) -> ApiError {
    let code = if r == Refusal::Malformed { Code::BadValue } else { Code::Forbidden };
    ApiError::field(code, "command", r.message())
}

/// The history file: the control panel's (openwrt-luci's terminal keeps the same one), root's only, in RAM.
fn history_file(ctx: &Ctx) -> std::path::PathBuf {
    ctx.cfg.dash_dir.join("at-history")
}

fn history_add(ctx: &Ctx, cmd: &str) {
    let dir = &ctx.cfg.dash_dir;
    let _ = fs::create_dir_all(dir);
    // SAFETY: geteuid(2) has no preconditions.
    let me = unsafe { libc::geteuid() };
    let own = fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && m.uid() == me);
    if !own {
        return;
    }
    let f = history_file(ctx);
    let old = fs::read_to_string(&f).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().map(str::to_owned).collect();
    lines.push(format!("{} {}", stamp(), at_guard::redact(cmd)));
    let keep = lines.split_off(lines.len().saturating_sub(HISTORY_MAX));
    let tmp = f.with_extension(format!("t{}", std::process::id()));
    let w = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp);
    if let Ok(mut w) = w
        && w.write_all((keep.join("\n") + "\n").as_bytes()).is_ok()
    {
        let _ = fs::rename(&tmp, &f);
    }
    let _ = fs::remove_file(&tmp);
}

/// "MM-DD HH:MM:SS" in local time.
#[allow(deprecated)] // libc warns that musl's time_t became 64-bit; it is 64-bit on aarch64 either way
fn stamp() -> String {
    let t = crate::sys::now_s() as libc::time_t;
    // SAFETY: localtime_r writes into the tm we own; a zeroed tm is a valid initial value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!("{:02}-{:02} {:02}:{:02}:{:02}", tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

async fn send(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let (cmd, class) = at_guard::check(p.str("command").unwrap_or_default()).map_err(refusal)?;
    let at = ctx.need_tool("mu300-at")?;
    if modem_restarting(&ctx.cfg) {
        return Err(ApiError::busy("the modem is restarting (network locks or a SIM switch), try again shortly"));
    }
    if let Some(id) = jobs::running(&ctx.cfg, "at") {
        return Err(ApiError::busy(format!("a long AT command is still running: job {id}")));
    }
    if let Some(c) = class
        && !p.flag("confirm")
    {
        return Err(ApiError::field(
            Code::Missing,
            "confirm",
            format!("class {}: this command changes the modem's state; send confirm: true", c.as_str()),
        ));
    }
    let t = p.int("timeout").unwrap_or(SYNC_MAX);
    history_add(&ctx, &cmd);
    if class == Some(Class::Scan) || t > SYNC_MAX {
        let wait = if class == Some(Class::Scan) { SCAN_T } else { t };
        let spec =
            JobSpec::new("at.send", "at", (wait + 40) as u64, argv(at.program(), &["-t", &wait.to_string(), "-"]))
                .stdin(format!("{cmd}\n"))
                .env("MU300_AT_LOCK_WAIT", "10");
        return start_job(&ctx, spec);
    }
    let o = at
        .args(["-t", &t.to_string(), "-"])
        .stdin(format!("{cmd}\n"))
        .env("MU300_AT_LOCK_WAIT", "3")
        .timeout_secs(t as u64 + 7)
        .output()
        .await?;
    let all = format!("{}\n{}", o.stdout, o.stderr);
    if all.lines().any(|l| l.trim() == "mu300-at: busy" || l.trim() == "unisoc-at: busy") {
        return Err(ApiError::busy("the AT channel is busy; the command was not sent, try again"));
    }
    if o.timed_out || all.contains("no answer from the daemon") {
        return Err(ApiError::new(
            Code::Timeout,
            "the modem did not answer in time; the command may still run, check before sending it again",
        ));
    }
    Ok(json!({ "rc": o.rc, "text": text_of(&o), "class": class.map(Class::as_str) }))
}

/// The last 40 commands of this boot, newest first, each without the arguments of a PIN or password command.
async fn history(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let text = fs::read_to_string(history_file(&ctx)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(HISTORY_MAX);
    let v: Vec<Value> = lines[start..]
        .iter()
        .rev()
        .map(|l| {
            let mut it = l.splitn(3, ' ');
            let (d, t, c) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
            json!({"time": format!("{d} {t}"), "command": sanitize_text(&at_guard::redact(c)).replace(['\n', '\t'], " ")})
        })
        .collect();
    Ok(json!({ "history": v }))
}

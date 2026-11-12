//! status.get, signal.get, cell.get, radio.*, modem.restart

use std::fs;

use serde_json::{Value, json};

use super::{argv, json_of, json_or_null, ok, start_job};
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule, check_word};
use crate::cache::{self, Tier};
use crate::exec::later;
use crate::jobs::JobSpec;
use crate::m;
use crate::sys::{Ctx, first_line, now_s};

const ON_OFF: Rule = Rule::Enum(&["on", "off"]);
const RADIO_SET: &[Field] = &[Field::req("slot", Rule::Scalar { max: 3 }), Field::req("state", ON_OFF)];

pub static METHODS: &[Method] = &[
    m!("status.get", Read, &[], status),
    m!("signal.get", Read, &[], signal),
    m!("cell.get", Read, &[], cell),
    m!("radio.get", Read, &[], radio_get),
    m!("radio.set", Write, RADIO_SET, radio_set),
    m!("modem.restart", Write, &[], modem_restart),
];

fn mem_kb(meminfo: &str, key: &str) -> Option<u64> {
    meminfo.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(':')?.trim().strip_suffix("kB")?.trim().parse().ok())
}

async fn df(ctx: &Ctx, path: &std::path::Path) -> Value {
    if !path.exists() {
        return Value::Null;
    }
    let Ok(o) = ctx.cmd("df").arg("-kP").arg(path).timeout_secs(5).run().await else { return Value::Null };
    let line = o.stdout.lines().nth(1).unwrap_or("");
    let f: Vec<u64> = line.split_whitespace().skip(1).take(3).filter_map(|x| x.parse().ok()).collect();
    if f.len() == 3 { json!({"size": f[0], "used": f[1], "avail": f[2]}) } else { Value::Null }
}

fn battery(ctx: &Ctx, device: &str) -> Value {
    if device != "u30air" {
        return Value::Null;
    }
    let dir = ctx.cfg.sys("/sys/class/power_supply");
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if first_line(&p.join("type")) != "Battery" {
            continue;
        }
        let n = |f: &str| first_line(&p.join(f)).parse::<i64>().ok();
        return json!({"capacity": n("capacity"), "status": first_line(&p.join("status")),
                      "voltage_uv": n("voltage_now"), "current_ua": n("current_now")});
    }
    Value::Null
}

fn temperatures(ctx: &Ctx) -> Value {
    let mut zones: Vec<_> = fs::read_dir(ctx.cfg.sys("/sys/class/thermal"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .map(|e| e.path())
        .collect();
    zones.sort();
    let v: Vec<Value> = zones
        .iter()
        .filter_map(|z| {
            let t = first_line(&z.join("temp")).parse::<i64>().ok()?;
            Some(json!({"name": first_line(&z.join("type")), "millicelsius": t}))
        })
        .collect();
    json!(v)
}

async fn status(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let device = ctx.device().await;
    let meminfo = fs::read_to_string(ctx.cfg.sys("/proc/meminfo")).unwrap_or_default();
    let uptime = first_line(&ctx.cfg.sys("/proc/uptime")).split('.').next().and_then(|s| s.parse::<u64>().ok());
    let traffic = if ctx.has_tool("mu300-traffic") {
        json_or_null(&ctx.tool("mu300-traffic").arg("json").timeout_secs(10).run().await)
    } else {
        Value::Null
    };
    let (iface, address) = super::data::data_addr(&ctx).await;
    let (ssid, enabled) = super::wifi::hotspot_brief(&ctx).await;
    Ok(json!({
        "system": ctx.sys.as_str(),
        "os": ctx.os_name(),
        "device": device,
        "release": ctx.release(),
        "kernel": super::system::kernel_release(&ctx),
        "hostname": super::system::hostname_now(&ctx),
        "uptime": uptime,
        "now": now_s(),
        "memory": {"total_kb": mem_kb(&meminfo, "MemTotal"), "avail_kb": mem_kb(&meminfo, "MemAvailable")},
        "storage": {"root": df(&ctx, &ctx.cfg.sys("/")).await, "disk": df(&ctx, &ctx.cfg.disk).await},
        "signal": cache::reading(&ctx.cfg, Tier::Fast),
        "data": {"state": super::data::data_service(&ctx).await, "iface": iface, "address": address},
        "traffic": traffic,
        "battery": battery(&ctx, &device),
        "temperatures": temperatures(&ctx),
        "hotspot": {"ssid": ssid, "enabled": enabled},
        "lan": ctx.lan_ip().await,
    }))
}

fn need_collector(ctx: &Ctx) -> ApiResult<()> {
    if cache::collector(&ctx.cfg).is_some() { Ok(()) } else { Err(ApiError::unsupported("radio readings")) }
}

async fn signal(ctx: Ctx, _: Params) -> ApiResult<Value> {
    need_collector(&ctx)?;
    Ok(cache::reading(&ctx.cfg, Tier::Fast))
}

async fn cell(ctx: Ctx, _: Params) -> ApiResult<Value> {
    need_collector(&ctx)?;
    Ok(cache::reading(&ctx.cfg, Tier::Full))
}

async fn radio_get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let o = ctx.need_tool("mu300-sim-radio")?.args(["status", "--json"]).timeout_secs(15).output().await?;
    let o = o.check()?;
    Ok(json!({ "radio": json_of(&o, "mu300-sim-radio")? }))
}

/// Each SIM's radio on or off (slot 1, 2 or "all": airplane mode). The SFUN sequence can take minutes: a job.
async fn radio_set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let slot = p.str("slot").unwrap_or_default();
    check_word("slot", slot, "1|2|all", 3)?;
    let sl = match slot {
        "1" => "0",
        "2" => "1",
        _ => "all",
    };
    let state = p.str("state").unwrap_or_default();
    let t = ctx.need_tool("mu300-sim-radio")?;
    let spec = JobSpec::new("radio.set", "modem", 150, argv(t.program(), &["set", sl, state])).not_cancellable();
    start_job(&ctx, spec)
}

/// Network recovery: the dial again, from the radio up.
async fn modem_restart(ctx: Ctx, _: Params) -> ApiResult<Value> {
    if ctx.sys.is_openwrt() {
        later(&ctx.cfg, &["sh", "-c", "ifdown wan; sleep 3; ifup wan"])?;
    } else {
        later(&ctx.cfg, &["systemctl", "restart", "mu300-mobile-data.service"])?;
    }
    ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn meminfo() {
        let m = "MemTotal:        1400000 kB\nMemAvailable:     700000 kB\n";
        assert_eq!(super::mem_kb(m, "MemTotal"), Some(1_400_000));
        assert_eq!(super::mem_kb(m, "MemFree"), None);
    }
}

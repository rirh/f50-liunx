//! hello, methods, system.*

use std::fs;

use serde_json::{Value, json};

use super::ok;
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::{self, Method};
use crate::api::validate::{Field, Params, Rule};
use crate::exec::{executable, later};
use crate::m;
use crate::sys::{Ctx, first_line, now_s};

/// The API version (mu300-api was 1; this is the unified contract).
pub const API_VERSION: u32 = 2;

const NAME: &[Field] = &[Field::req("name", Rule::Word { re: "[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?", max: 63 })];
const ZONE: &[Field] =
    &[Field::req("zone", Rule::Word { re: "UTC|[A-Z][A-Za-z_+-]{1,30}(/[A-Za-z0-9_+-]{1,30}){1,2}", max: 96 })];
const PASSWORD: &[Field] = &[Field::secret("password", Rule::Text { max_bytes: 128, max_chars: 128 }, true)];

pub static METHODS: &[Method] = &[
    m!("hello", Public, &[], hello),
    m!("methods", Public, &[], methods),
    m!("system.get", Read, &[], get),
    m!("system.reboot", Write, &[], reboot),
    m!("system.hostname", Write, NAME, hostname),
    m!("system.timezone", Write, ZONE, timezone),
    m!("system.password", Write, PASSWORD, password),
];

async fn hello(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let dev = ctx.device().await;
    let u30 = dev == "u30air";
    let features = json!({
        "wifi_client": ctx.has_tool("wifi-client"),
        "sms": super::sms::backend(&ctx).map(|b| b.name()),
        "locks": executable(&ctx.cfg.lock_bin),
        "radio_collector": crate::cache::collector(&ctx.cfg).map(|c| c.name()),
        "battery": u30,
        "usb_role": u30,
        "usb_net": super::lan::usbnet_ok(&ctx),
        "luci": ctx.sys.is_luci(),
    });
    Ok(json!({
        "api": API_VERSION,
        "server": "mu300d",
        "version": env!("CARGO_PKG_VERSION"),
        "system": ctx.sys.as_str(),
        "device": dev,
        "os": ctx.os_name(),
        "release": ctx.release(),
        "methods": registry::names(),
        "features": features,
    }))
}

async fn methods(_: Ctx, _: Params) -> ApiResult<Value> {
    Ok(json!({ "methods": registry::describe() }))
}

/// The host name, from the kernel (no subprocess).
pub fn hostname_now(ctx: &Ctx) -> String {
    let h = first_line(&ctx.cfg.sys("/proc/sys/kernel/hostname"));
    if !h.is_empty() {
        return h;
    }
    let mut buf = [0u8; 256];
    // SAFETY: gethostname(2) writes at most buf.len() bytes into our buffer.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        return String::from_utf8_lossy(&buf[..end]).into_owned();
    }
    String::new()
}

pub fn kernel_release(ctx: &Ctx) -> String {
    first_line(&ctx.cfg.sys("/proc/sys/kernel/osrelease"))
}

async fn current_timezone(ctx: &Ctx) -> String {
    if ctx.sys.is_openwrt() {
        ctx.uci_get("system.@system[0].zonename").await
    } else {
        fs::read_link(ctx.cfg.sys("/etc/localtime"))
            .ok()
            .and_then(|p| p.to_string_lossy().split_once("/zoneinfo/").map(|(_, z)| z.to_owned()))
            .unwrap_or_default()
    }
}

async fn ssh_on(ctx: &Ctx) -> bool {
    if ctx.sys.is_openwrt() {
        fs::read_dir(ctx.cfg.sys("/etc/rc.d")).into_iter().flatten().flatten().any(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with('S') && n.ends_with("dropbear")
        })
    } else {
        for unit in ["ssh.socket", "ssh.service"] {
            if ctx.cmd("systemctl").args(["is-enabled", unit]).timeout_secs(5).run().await.is_ok_and(|o| o.success()) {
                return true;
            }
        }
        false
    }
}

async fn get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    Ok(json!({
        "hostname": hostname_now(&ctx),
        "timezone": current_timezone(&ctx).await,
        "user": ctx.sys.login_user(),
        "ssh": ssh_on(&ctx).await,
        "time": now_s(),
        "release": ctx.release(),
        "kernel": kernel_release(&ctx),
        "system": ctx.sys.as_str(),
        "os": ctx.os_name(),
    }))
}

async fn reboot(ctx: Ctx, _: Params) -> ApiResult<Value> {
    later(&ctx.cfg, &["sh", "-c", "sync; reboot"])?;
    ok()
}

async fn hostname(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let name = p.str("name").unwrap_or_default().to_owned();
    if ctx.sys.is_openwrt() {
        ctx.uci_batch(format!("set system.@system[0].hostname={name}\ncommit system\n")).await?;
        later(&ctx.cfg, &["/etc/init.d/system", "reload"])?;
    } else {
        ctx.cmd("hostnamectl").args(["set-hostname", &name]).timeout_secs(15).ok().await?;
    }
    ok()
}

async fn timezone(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let zone = p.str("zone").unwrap_or_default().to_owned();
    if zone.contains("..") {
        return Err(ApiError::bad_value("zone"));
    }
    let zf = ctx.cfg.sys(&format!("/usr/share/zoneinfo/{zone}"));
    if ctx.sys.is_openwrt() {
        // OpenWrt keeps a POSIX TZ string: the last line of the zone's file
        let tz = if zone == "UTC" {
            "UTC0".to_owned()
        } else {
            let t = fs::read_to_string(&zf).map_err(|_| ApiError::unsupported(&format!("zoneinfo for {zone}")))?;
            t.lines().last().unwrap_or("").to_owned()
        };
        if !crate::api::validate::matches("[A-Za-z0-9<>+,./:-]{1,64}", &tz) {
            return Err(ApiError::unsupported(&format!("zoneinfo for {zone}")));
        }
        let q = crate::sys::uq;
        ctx.uci_batch(format!(
            "set system.@system[0].zonename={}\nset system.@system[0].timezone={}\ncommit system\n",
            q(&zone),
            q(&tz)
        ))
        .await?;
        later(&ctx.cfg, &["/etc/init.d/system", "reload"])?;
    } else {
        if !zf.is_file() {
            return Err(ApiError::bad_value("zone"));
        }
        ctx.cmd("timedatectl").args(["set-timezone", &zone]).timeout_secs(15).ok().await?;
    }
    ok()
}

async fn password(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let pw = p.str("password").unwrap_or_default();
    if pw.is_empty() {
        return Err(ApiError::missing("password"));
    }
    let user = ctx.sys.login_user();
    // Ubuntu's chpasswd (PAM's hash); OpenWrt's busybox has none, the static one next to us has it
    let bb = ctx.cfg.tool("busybox");
    let t = if ctx.sys.is_openwrt() && executable(&bb) {
        crate::exec::Tool::new(&ctx.cfg, bb).args(["chpasswd", "-c", "sha512"])
    } else {
        ctx.cmd("chpasswd")
    };
    t.stdin(format!("{user}:{pw}\n")).timeout_secs(15).ok().await?;
    Ok(json!({ "user": user }))
}

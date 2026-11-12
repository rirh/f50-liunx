//! lan.*, usb.*

use serde_json::{Value, json};

use super::ok;
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule};
use crate::exec::{Tool, executable, later};
use crate::m;
use crate::sys::{Ctx, conf_put, first_line};

/// A private /24 with the device at .1 (DHCP hands out .2-.200 of it).
const LAN_IP: &str = r"(10\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])|172\.(1[6-9]|2[0-9]|3[01])\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])|192\.168\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9]))\.1";
const LAN_SET: &[Field] = &[Field::req("ip", Rule::Word { re: LAN_IP, max: 15 })];
const ROLE: &[Field] = &[Field::req("role", Rule::Enum(&["host", "device"]))];
const NET: &[Field] = &[
    Field::opt("mode", Rule::Enum(&["ncm", "ecm", "rndis", "multi", "default"])),
    Field::opt("scope", Rule::Enum(&["once", "permanent"])),
    Field::opt("name", Rule::Enum(&["usb", "eth"])),
];

pub static METHODS: &[Method] = &[
    m!("lan.get", Read, &[], get),
    m!("lan.set", Write, LAN_SET, set),
    m!("usb.role", Write, ROLE, role),
    m!("usb.net", Write, NET, net),
];

fn usb_adapter(ctx: &Ctx) -> std::path::PathBuf {
    ctx.cfg.unisoc_dir.join("device-usb")
}

/// The USB network setting is available: the panel's device-usb adapter (openwrt-luci) or mu300-usb.
pub(crate) fn usbnet_ok(ctx: &Ctx) -> bool {
    if ctx.sys.is_luci() { executable(&usb_adapter(ctx)) } else { ctx.has_tool("mu300-usb") }
}

/// "role: host" / "usb role device" in mu300-usb's status.
fn parse_role(out: &str) -> String {
    for l in out.lines() {
        let l = l.trim_start();
        let l = l.strip_prefix("usb ").unwrap_or(l);
        if let Some(r) = l.strip_prefix("role") {
            return r.trim_start_matches([':', ' ']).chars().take_while(char::is_ascii_lowercase).collect();
        }
    }
    String::new()
}

async fn get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let u30 = ctx.device().await == "u30air";
    let mut role = String::new();
    if u30 && ctx.has_tool("mu300-usb") {
        if let Ok(o) = ctx.tool("mu300-usb").timeout_secs(10).run().await {
            role = parse_role(&o.stdout);
        }
    }
    Ok(json!({
        "ip": ctx.lan_ip().await,
        "default_ip": ctx.line_of(ctx.tool("mu300-lan-ip")).await,
        "usb_role": role,
        "usb_role_switch": u30,
    }))
}

async fn set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let ip = p.str("ip").unwrap_or_default();
    if ctx.sys.is_openwrt() {
        ctx.uci_batch(format!("set network.lan.ipaddr={ip}\ncommit network\n")).await?;
        later(&ctx.cfg, &["/etc/init.d/network", "reload"])?;
    } else {
        conf_put(&ctx.cfg.sys("/etc/mu300/lan.conf"), 0o644, &[("LAN_IP", ip)])
            .map_err(|_| ApiError::failed("could not write lan.conf"))?;
        later(&ctx.cfg, &["systemctl", "restart", "mu300-lan"])?;
    }
    Ok(json!({ "restart": true }))
}

async fn role(ctx: Ctx, p: Params) -> ApiResult<Value> {
    if ctx.device().await != "u30air" || !ctx.has_tool("mu300-usb") {
        return Err(ApiError::unsupported("usb role"));
    }
    let usb = ctx.cfg.tool("mu300-usb");
    later(&ctx.cfg, &[&usb.to_string_lossy(), p.str("role").unwrap_or_default()])?;
    ok()
}

fn field_of<'a>(out: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\":\"");
    out.find(&pat)
        .map(|i| &out[i + pat.len()..])
        .and_then(|r| r.split('"').next())
        .filter(|v| v.bytes().all(|b| b.is_ascii_lowercase()))
        .unwrap_or("")
}

/// The USB network to a computer (NCM, ECM, RNDIS, multi) from the next boot on, for good or once, and the name a
/// Linux computer gives the link. Without a field this only reads it.
async fn net(ctx: Ctx, p: Params) -> ApiResult<Value> {
    if !usbnet_ok(&ctx) {
        return Err(ApiError::unsupported("USB network mode"));
    }
    let mode = p.str("mode");
    let scope = p.str("scope").unwrap_or("permanent");
    let name = p.str("name");
    let applied = first_line(&ctx.cfg.sys("/run/mu300/usb-net-applied"));
    let now = ["ncm", "ecm", "rndis", "multi"].contains(&applied.as_str()).then_some(applied);
    if ctx.sys.is_luci() {
        let a = || Tool::new(&ctx.cfg, usb_adapter(&ctx)).timeout_secs(20);
        let check = |out: &str| {
            if out.contains("\"ok\":1") { Ok(()) } else { Err(ApiError::failed(field_of(out, "error"))) }
        };
        if let Some(m) = mode {
            let o = if m == "default" {
                a().args(["set-net", "ncm", "permanent", "0"]).output().await?
            } else {
                a().args(["set-net", m, scope, "1"]).output().await?
            };
            check(&o.stdout)?;
        }
        if let Some(n) = name {
            check(&a().args(["set-net-name", n]).output().await?.stdout)?;
        }
        let o = a().arg("get").output().await?;
        let mut m = field_of(&o.stdout, "net_mode");
        if !o.stdout.contains("\"net_auto\":1") || !["ncm", "ecm", "rndis", "multi"].contains(&m) {
            m = "default";
        }
        let once = m != "default" && field_of(&o.stdout, "net_scope") == "once";
        let nm = if field_of(&o.stdout, "net_name") == "eth" { "eth" } else { "usb" };
        return Ok(json!({"mode": m, "once": once, "name": nm, "now": now}));
    }
    let usb = || ctx.tool("mu300-usb").timeout_secs(20);
    if let Some(m) = mode {
        let t = match (m, scope) {
            ("default", _) => usb().args(["net", "default"]),
            (m, "once") => usb().args(["net", m, "once"]),
            (m, _) => usb().args(["net", m]),
        };
        t.ok().await?;
    }
    if let Some(n) = name {
        usb().args(["net-name", n]).ok().await?;
    }
    let o = usb().arg("net-name").ok().await?;
    let nm = o.stdout.lines().find_map(|l| l.strip_prefix("USB network name from the next boot: ")).unwrap_or("");
    let nm = if nm.trim() == "eth" { "eth" } else { "usb" };
    let o = usb().arg("net").ok().await?;
    let n = o.stdout.lines().find_map(|l| l.strip_prefix("USB network from the next boot: ")).unwrap_or("").trim();
    let (n, once) = match n.strip_suffix(" once") {
        Some(b) => (b, true),
        None => (n, false),
    };
    let n = if ["ncm", "ecm", "rndis", "multi", "default"].contains(&n) { n } else { "default" };
    Ok(json!({"mode": n, "once": once, "name": nm, "now": now}))
}

#[cfg(test)]
mod tests {
    use crate::api::validate::matches;

    #[test]
    fn lan_ip_rule() {
        for ok in ["192.168.77.1", "10.0.0.1", "172.16.5.1", "10.255.255.1"] {
            assert!(matches(super::LAN_IP, ok), "{ok}");
        }
        for bad in ["192.168.77.2", "172.32.0.1", "8.8.8.1", "10.256.0.1", "192.168.1.1 "] {
            assert!(!matches(super::LAN_IP, bad), "{bad}");
        }
    }

    #[test]
    fn role_parse() {
        assert_eq!(super::parse_role("mode: x\nusb role: host\n"), "host");
        assert_eq!(super::parse_role("  role device"), "device");
        assert_eq!(super::parse_role("nothing"), "");
    }
}

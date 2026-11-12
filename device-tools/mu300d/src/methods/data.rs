//! data.*, traffic.*

use serde_json::{Value, json};

use super::{json_of, json_or_null, ok};
use crate::api::error::{ApiError, ApiResult};
use crate::api::registry::Method;
use crate::api::validate::{Field, Params, Rule, check_word};
use crate::cache::{self, Tier};
use crate::exec::later;
use crate::m;
use crate::sys::{Ctx, conf_put, conf_unq, uq};

const ON_OFF: Rule = Rule::Enum(&["on", "off"]);
const DATA_SET: &[Field] = &[
    Field::opt("apn", Rule::Word { re: "|[A-Za-z0-9]([A-Za-z0-9.-]{0,98}[A-Za-z0-9])?", max: 100 }),
    Field::opt("pdp", Rule::Enum(&["IP", "IPV6", "IPV4V6"])),
    Field::opt("auth", Rule::Enum(&["none", "pap", "chap", "both"])),
    Field::opt("user", Rule::Word { re: "[ -~]{0,64}", max: 64 }),
    Field::secret("password", Rule::Word { re: "[ -~]{1,64}", max: 64 }, false),
    Field::opt("clear_credentials", Rule::Bool),
    Field::opt("roaming", Rule::Bool),
];
const STATE: &[Field] = &[Field::req("state", ON_OFF)];
const TRAFFIC_SET: &[Field] = &[
    Field::req(
        "key",
        Rule::Enum(&["reset_day", "cap", "warn", "cut", "used", "clear", "daily", "daily_cut", "count", "plan"]),
    ),
    Field::opt("value", Rule::Scalar { max: 96 }),
];

pub static METHODS: &[Method] = &[
    m!("data.get", Read, &[], get),
    m!("data.set", Write, DATA_SET, set),
    m!("data.power", Write, STATE, power),
    m!("traffic.get", Read, &[], traffic_get),
    m!("traffic.set", Write, TRAFFIC_SET, traffic_set),
];

/// The cellular interface with an address (SIM 1: sipa_eth0, SIM 2: sipa_eth8), and the address.
pub(crate) async fn data_addr(ctx: &Ctx) -> (String, String) {
    for i in ["sipa_eth0", "sipa_eth8"] {
        if let Ok(o) = ctx.cmd("ip").args(["-4", "-o", "addr", "show", "dev", i]).timeout_secs(5).run().await
            && let Some(a) = o.stdout.lines().next().and_then(|l| l.split_whitespace().nth(3))
        {
            return (i.into(), a.into());
        }
    }
    ("sipa_eth0".into(), String::new())
}

/// up, down, or off (OpenWrt's WAN disabled).
pub(crate) async fn data_service(ctx: &Ctx) -> &'static str {
    if ctx.sys.is_openwrt() {
        if ctx.uci_get("network.wan.disabled").await == "1" {
            return "off";
        }
        let o = ctx.cmd("ifstatus").arg("wan").timeout_secs(5).run().await;
        let up = o.is_ok_and(|o| {
            o.stdout.lines().any(|l| l.split_whitespace().collect::<String>().starts_with("\"up\":true"))
        });
        if up { "up" } else { "down" }
    } else if ctx.svc_active("mu300-mobile-data").await {
        "up"
    } else {
        "down"
    }
}

async fn get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let (iface, address) = data_addr(&ctx).await;
    let (apn, pdp, user, auth, pw_set) = if ctx.sys.is_openwrt() {
        (
            ctx.uci_get("network.wan.apn").await,
            ctx.uci_get("network.wan.pdptype").await,
            ctx.uci_get("network.wan.username").await,
            ctx.uci_get("network.wan.auth").await,
            !ctx.uci_get("network.wan.password").await.is_empty(),
        )
    } else {
        let f = ctx.cfg.sys("/etc/mu300/mobile-data.conf");
        let g = |k: &str| conf_unq(&f, k).unwrap_or_default();
        (
            g("MU300_APN"),
            g("MU300_PDP_TYPE"),
            g("MU300_APN_USER"),
            g("MU300_APN_AUTH"),
            !g("MU300_APN_PASSWORD").is_empty(),
        )
    };
    let sim = if ctx.has_tool("mu300-sim") {
        json_or_null(&ctx.tool("mu300-sim").args(["status", "--json"]).timeout_secs(10).run().await)
    } else {
        Value::Null
    };
    // data roaming (mobile-data roaming --json): null where the system's mobile-data has no such command
    let mut roaming = Value::Null;
    if ctx.has_tool("mobile-data")
        && let Ok(o) = ctx.tool("mobile-data").args(["roaming", "--json"]).timeout_secs(10).run().await
    {
        let first = o.stdout.lines().next().unwrap_or("");
        if first.starts_with("{\"allowed\":") {
            roaming = serde_json::from_str(first).unwrap_or(Value::Null);
        }
    }
    Ok(json!({
        "state": data_service(&ctx).await,
        "iface": iface,
        "address": address,
        "apn": apn,
        "pdp": if pdp.is_empty() { "IP".to_owned() } else { pdp },
        "auth": auth,
        "user": user,
        "password_set": pw_set,
        "signal": cache::reading(&ctx.cfg, Tier::Fast),
        "sim": sim,
        "roaming_switch": !roaming.is_null(),
        "roaming": roaming,
    }))
}

/// The dial's own rule for credentials: printable ASCII without a quote or a backslash, no space at either end.
fn cred_ok(s: &str) -> bool {
    !s.contains(['"', '\'', '\\']) && !s.starts_with(' ') && !s.ends_with(' ')
}

async fn set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let apn = p.str("apn").map(str::to_owned);
    let pdp = p.str("pdp").map(str::to_owned);
    let mut auth = p.str("auth").map(str::to_owned);
    let mut user = p.str("user").map(str::to_owned);
    let mut pw = p.str("password").map(str::to_owned);
    if user.as_deref().is_some_and(|u| !cred_ok(u)) {
        return Err(ApiError::bad_value("user"));
    }
    if pw.as_deref().is_some_and(|u| !cred_ok(u)) {
        return Err(ApiError::bad_value("password"));
    }
    if p.flag("clear_credentials") {
        user = Some(String::new());
        pw = Some(String::new());
        auth = Some(String::new());
    }
    let roam = p.bool("roaming");
    let profile = apn.is_some() || pdp.is_some() || auth.is_some() || user.is_some() || pw.is_some();
    if !profile && roam.is_none() {
        return Err(ApiError::missing("apn"));
    }
    if let Some(r) = roam {
        // the setting alone: the dial and its watchdog take it up within half a minute, nothing is restarted
        let t = ctx.need_tool("mobile-data")?;
        t.args(["roaming", if r { "on" } else { "off" }]).timeout_secs(15).ok().await?;
        if !profile {
            return Ok(json!({ "restart": false }));
        }
    }
    if ctx.sys.is_openwrt() {
        let mut s = String::new();
        let mut put = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                s.push_str(&format!("set network.wan.{k}={}\n", uq(v)));
            }
        };
        put("apn", &apn);
        put("pdptype", &pdp);
        put("auth", &auth);
        put("username", &user);
        put("password", &pw);
        s.push_str("commit network\n");
        ctx.uci_batch(s).await?;
        later(&ctx.cfg, &["ifup", "wan"])?;
    } else {
        let f = ctx.cfg.sys("/etc/mu300/mobile-data.conf");
        let pairs: Vec<(&str, &str)> = [
            ("MU300_APN", &apn),
            ("MU300_PDP_TYPE", &pdp),
            ("MU300_APN_AUTH", &auth),
            ("MU300_APN_USER", &user),
            ("MU300_APN_PASSWORD", &pw),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.as_deref().map(|v| (k, v)))
        .collect();
        conf_put(&f, 0o600, &pairs).map_err(|_| ApiError::failed("could not write mobile-data.conf"))?;
        later(&ctx.cfg, &["systemctl", "restart", "mu300-mobile-data"])?;
    }
    Ok(json!({ "restart": true }))
}

async fn power(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let on = p.str("state") == Some("on");
    if ctx.sys.is_openwrt() {
        later(&ctx.cfg, &[if on { "ifup" } else { "ifdown" }, "wan"])?;
    } else {
        later(&ctx.cfg, &["systemctl", if on { "start" } else { "stop" }, "mu300-mobile-data.service"])?;
    }
    ok()
}

async fn traffic_get(ctx: Ctx, _: Params) -> ApiResult<Value> {
    let o = ctx.need_tool("mu300-traffic")?.arg("json").timeout_secs(15).ok().await?;
    Ok(json!({ "traffic": json_of(&o, "mu300-traffic")? }))
}

/// traffic.set {key, value}: reset_day 1-31, cap/used/daily bytes (0: none), warn 1-100, cut/daily_cut on|off,
/// clear "yes", count total|rx|tx, plan text (at most 96 bytes; "" for none).
async fn traffic_set(ctx: Ctx, p: Params) -> ApiResult<Value> {
    let key = p.str("key").unwrap_or_default();
    let value = p.str("value");
    let v = match (key, value) {
        ("plan", Some(v)) => v,
        (_, None | Some("")) => return Err(ApiError::missing("value")),
        (_, Some(v)) => v,
    };
    let range = |lo: u32, hi: u32| -> ApiResult<()> {
        match v.parse::<u32>() {
            Ok(n) if (lo..=hi).contains(&n) && !v.starts_with('0') => Ok(()),
            _ => Err(ApiError::bad_value("value")),
        }
    };
    match key {
        "reset_day" => range(1, 31)?,
        "warn" => range(1, 100)?,
        "cut" | "daily_cut" => check_word("value", v, "on|off", 3)?,
        "clear" => check_word("value", v, "yes", 3)?,
        "count" => check_word("value", v, "total|rx|tx", 5)?,
        "cap" | "used" | "daily" => check_word("value", v, "0|[1-9][0-9]{0,15}", 16)?,
        _ => {
            if v.chars().any(char::is_control) {
                return Err(ApiError::bad_value("value"));
            }
        }
    }
    let args: Vec<&str> = match key {
        "reset_day" => vec!["set", "reset-day", v],
        "cap" | "daily" if v == "0" => vec!["set", key, "off"],
        "cap" | "daily" | "warn" | "cut" | "count" | "plan" => vec!["set", key, v],
        "daily_cut" => vec!["set", "daily-cut", v],
        "used" => vec!["used", v],
        _ => vec!["clear", "--yes"],
    };
    ctx.need_tool("mu300-traffic")?.args(args).timeout_secs(15).ok().await?;
    ok()
}

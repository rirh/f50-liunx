//! `f50.data` and `f50.traffic`.
//!
//! - `f50.data get` (read): the connection's state and why it is held, the interface and its addresses, the card
//!   in use's APN settings without the password (`password_set` says whether there is one), data roaming.
//! - `f50.data set` (write): the card in use's `apn`, `auth` (none|pap|chap), `username`, `password` (a secret: it
//!   goes to UCI on `uci batch`'s stdin and to the modem, never back), `pdp` (ipv4|ipv6|ipv4v6), `roaming`.
//!   A change to the context makes netifd dial again (`network reload`); roaming alone is read at the next decision.
//! - `f50.data power {state: on|off}` (write): mobile data on or off through netifd (`ifup`/`ifdown`: off lasts until
//!   it is turned on or the next boot).
//! - `f50.traffic get` (read), `f50.traffic set {key, value}` (write): `reset_day` 1-31, `cap` bytes (0: none),
//!   `warn` 1-100, `cut` on|off, `plan` (one line, at most 96 bytes), `clear` yes - the checks of the panel's adapter.

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::Data;
use crate::netifd::IfStatus;
use crate::settings::{Auth, Edit, Pdp, WanSettings, apn_ok, credential_ok};
use crate::traffic::sampler;

const SET: &[Field] = &[
    Field::opt("apn", Rule::Text { max_bytes: 100, max_chars: 100 }),
    Field::opt("auth", Rule::Enum(&["none", "pap", "chap"])),
    Field::opt("username", Rule::Text { max_bytes: 64, max_chars: 64 }),
    Field::secret("password", Rule::Text { max_bytes: 64, max_chars: 64 }, false),
    Field::opt("pdp", Rule::Enum(&["ipv4", "ipv6", "ipv4v6"])),
    Field::opt("roaming", Rule::Bool),
];
const POWER: &[Field] = &[Field::req("state", Rule::Enum(&["on", "off"]))];
const TRAFFIC_SET: &[Field] = &[
    Field::req("key", Rule::Enum(&["reset_day", "cap", "warn", "cut", "plan", "clear"])),
    Field::req("value", Rule::Text { max_bytes: 96, max_chars: 96 }),
];

pub(crate) fn data_object(d: &Data) -> Object {
    let (a, b, c) = (d.clone(), d.clone(), d.clone());
    Object::builder("f50.data")
        .method("get", Access::Read, &[], "The mobile data connection and its settings.", move |_p: Params| {
            let d = a.clone();
            async move { get(&d).await }
        })
        .method("set", Access::Write, SET, "The APN settings and data roaming of the SIM in use.", move |p: Params| {
            let d = b.clone();
            async move { set(&d, &p).await }
        })
        .method("power", Access::Write, POWER, "Mobile data on or off.", move |p: Params| {
            let d = c.clone();
            async move {
                let on = p.need_str("state")? == "on";
                let iface = d.config().interface.clone();
                let n = d.platform().netifd();
                if on {
                    n.up(&iface).await?
                } else {
                    n.down(&iface).await?
                }
                Ok(json!({ "state": if on { "on" } else { "off" } }))
            }
        })
        .build()
}

pub(crate) fn traffic_object(d: &Data) -> Object {
    let (a, b) = (d.clone(), d.clone());
    Object::builder("f50.traffic")
        .method("get", Access::Read, &[], "Data usage per day and billing cycle, and the cap.", move |_p: Params| {
            let d = a.clone();
            async move {
                let now = i64::try_from(unix_now()).unwrap_or(0);
                Ok(d.traffic().view(now, sampler::offset(&d, now)))
            }
        })
        .method("set", Access::Write, TRAFFIC_SET, "A data usage setting, or clear the count.", move |p: Params| {
            let d = b.clone();
            async move {
                let (key, value) = (p.need_str("key")?, p.need_str("value")?);
                if key == "clear" {
                    if value != "yes" {
                        return Err(Error::bad_value("value"));
                    }
                    sampler::clear(&d).await?;
                } else {
                    d.traffic().set_conf(key, value)?;
                }
                Ok(json!({}))
            }
        })
        .build()
}

async fn get(d: &Data) -> Result<Value> {
    let slot = d.modem().active_slot();
    let cfg = d.config();
    let settings = WanSettings::load(d.platform(), &cfg.interface, slot).await.unwrap_or_default();
    let mut addresses = Vec::new();
    for i in [&cfg.interface, &cfg.interface6] {
        if let Ok(Some(s)) = IfStatus::read(d.platform(), i).await {
            addresses.extend(s.addresses);
        }
    }
    let card = &settings.card;
    let auth =
        card.credentials().map(|c| format!("{:?}", c.auth).to_ascii_lowercase()).unwrap_or_else(|_| "invalid".into());
    let st = d.state();
    let device = st.bearer.as_ref().map_or_else(|| cfg.device_for(slot).to_owned(), |b| b.device.clone());
    // the device's byte counters since it came up: a page diffs two readings for the rate
    let counter =
        |name: &str| d.platform().sysfs().read_num::<u64>(format!("/sys/class/net/{device}/statistics/{name}"));
    let bytes = match (counter("rx_bytes"), counter("tx_bytes")) {
        (Some(rx), Some(tx)) => json!({ "rx": rx, "tx": tx }),
        _ => Value::Null,
    };
    Ok(json!({
        "state": st.phase,
        "hold": st.hold,
        "error": st.last_error,
        "since_s": st.since.map(|t| t.elapsed().as_secs()),
        "interface": cfg.interface,
        "device": device,
        "bytes": bytes,
        "slot": slot.number(),
        "addresses": addresses,
        "dns": st.bearer.as_ref().map(|b| b.dns.clone()),
        "relay": settings.relay,
        "apn": card.apn,
        "pdp": card.pdp_or_default(slot),
        "auth": auth,
        "username": card.username,
        "password_set": !card.password.is_empty(),
        "iccid_bound": card.iccid.is_some(),
        "roaming": card.roaming_allowed(),
        "roaming_held": st.hold == Some(crate::Hold::Roaming),
    }))
}

async fn set(d: &Data, p: &Params) -> Result<Value> {
    if p.is_empty() {
        return Err(Error::missing("apn"));
    }
    let edit = Edit {
        apn: p.str("apn").map(str::to_owned),
        pdp: p.str("pdp").and_then(Pdp::parse),
        auth: p.str("auth").map(|a| match a {
            "pap" => Auth::Pap,
            "chap" => Auth::Chap,
            _ => Auth::None,
        }),
        username: p.str("username").map(str::to_owned),
        password: p.str("password").map(str::to_owned),
        roaming: p.bool("roaming"),
    };
    if edit.apn.as_deref().is_some_and(|a| !apn_ok(a)) {
        return Err(Error::field(f50_core::Code::BadValue, "apn", "the APN may not hold a quote or a backslash"));
    }
    for (f, v) in [("username", &edit.username), ("password", &edit.password)] {
        if v.as_deref().is_some_and(|v| !credential_ok(v)) {
            return Err(Error::field(
                f50_core::Code::BadValue,
                f,
                format!("{f}: printable ASCII only, and no quote or backslash"),
            ));
        }
    }
    let slot = d.modem().active_slot();
    let iface = d.config().interface.clone();
    let now = WanSettings::load(d.platform(), &iface, slot).await?;
    let after = edit.applied_to(&now.card);
    let creds = after.credentials().map_err(|e| Error::field(f50_core::Code::BadValue, "auth", e.message()))?;
    if creds.auth != Auth::None && after.apn.is_empty() {
        return Err(Error::field(f50_core::Code::BadValue, "apn", "a user name or password needs an APN"));
    }
    d.platform().uci().apply("network", &edit.changes(&iface, slot)).await?;
    if edit.redials() {
        d.platform().netifd().reload().await?;
    }
    Ok(json!({ "slot": slot.number(), "redial": edit.redials() }))
}

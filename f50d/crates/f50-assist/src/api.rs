//! `f50.balance`, `f50.remote` and `f50.ddns`.
//!
//! - `f50.balance get` (Read: an amount, no number or message): `{amount, at, asked_at, pending, error, settings}`;
//!   `query` sends the carrier's query and answers at once (the reply comes within a minute or two: `pending`);
//!   `set {number, code}` (empty: from the SIM). Nothing queries on a schedule: each query is a billable SMS.
//! - `f50.remote get`/`set {enabled, numbers, secret}`: the allowed numbers and the history are private (Write);
//!   the secret is never returned.
//! - `f50.ddns get`/`set {enabled, token, zone, records, interval_minutes}`/`update`: `records` is a JSON array of
//!   `{name, type, target}` (`target`: `router` or an IPv6 suffix); `neighbors`: the LAN's devices with the IPv6
//!   addresses seen; the token is never returned.

use std::sync::Arc;

use f50_core::rpc::{Access, Field, Object, Rule};
use f50_core::time::unix_now;

use crate::{Inner, balance, ddns, remote};

const BALANCE_SET: &[Field] = &[
    Field::opt("number", Rule::Word { re: r"|[0-9]{3,15}", max: 15 }),
    Field::opt("code", Rule::Text { max_bytes: 40, max_chars: 20 }),
];
const REMOTE_SET: &[Field] = &[
    Field::opt("enabled", Rule::Bool),
    Field::opt("numbers", Rule::Text { max_bytes: 400, max_chars: 200 }),
    Field::secret("secret", Rule::Text { max_bytes: 64, max_chars: 32 }, false),
];
const DDNS_SET: &[Field] = &[
    Field::opt("enabled", Rule::Bool),
    Field::secret("token", Rule::Word { re: r"|[A-Za-z0-9_-]{20,200}", max: 200 }, false),
    Field::opt("zone", Rule::Word { re: r"|[A-Za-z0-9.-]{3,253}", max: 253 }),
    Field::opt("records", Rule::Body { max_chars: 8000 }),
    Field::opt("interval_minutes", Rule::Int { min: 1, max: 1440 }),
];

pub(crate) fn objects(inner: &Arc<Inner>) -> Vec<Object> {
    use Access::{Read, Write};
    let i = inner.clone();
    let balance = Object::builder("f50.balance").method(
        "get",
        Read,
        &[],
        "The SIM's balance as the carrier's last reply stated it.",
        move |_| {
            let st = Inner::lock(&i.balance).clone();
            async move { Ok(st.view(unix_now())) }
        },
    );
    let i = inner.clone();
    let balance = balance.method("query", Write, &[], "Sends the carrier's balance query by SMS.", move |_| {
        let i = i.clone();
        async move { balance::ask(&i).await.map(|st| st.view(unix_now())) }
    });
    let i = inner.clone();
    let balance = balance
        .method("set", Write, BALANCE_SET, "The query's number and code.", move |p| {
            let i = i.clone();
            async move {
                let next = {
                    let mut st = Inner::lock(&i.balance).clone();
                    if let Some(n) = p.str("number") {
                        st.settings.number = n.to_owned();
                    }
                    if let Some(c) = p.str("code") {
                        st.settings.code = c.trim().to_owned();
                    }
                    st
                };
                crate::store::save(&i.dir, "balance.json", &next).await?;
                let view = next.view(unix_now());
                *Inner::lock(&i.balance) = next;
                Ok(view)
            }
        })
        .build();

    let i = inner.clone();
    let remote = Object::builder("f50.remote").method(
        "get",
        Write,
        &[],
        "SMS remote control: the allowed numbers and the recent commands.",
        move |_| {
            let v = Inner::lock(&i.remote).view();
            async move { Ok(v) }
        },
    );
    let i = inner.clone();
    let remote = remote
        .method("set", Write, REMOTE_SET, "Turns SMS remote control on or off, its numbers and secret.", move |p| {
            let i = i.clone();
            async move { remote::set(&i, p.bool("enabled"), p.str("numbers"), p.str("secret")).await }
        })
        .build();

    let i = inner.clone();
    let ddns = Object::builder("f50.ddns").method(
        "get",
        Write,
        &[],
        "DDNS (Cloudflare): the records and how each stands.",
        move |_| {
            let v = Inner::lock(&i.ddns).view();
            async move { Ok(v) }
        },
    );
    let i = inner.clone();
    let ddns = ddns.method("set", Write, DDNS_SET, "Changes DDNS; absent fields are kept.", move |p| {
        let i = i.clone();
        async move {
            let c = ddns::Change {
                enabled: p.bool("enabled"),
                token: p.str("token"),
                zone: p.str("zone"),
                records: p.str("records"),
                interval_minutes: p.int("interval_minutes").and_then(|m| u32::try_from(m).ok()),
            };
            ddns::set(&i, c).await
        }
    });
    let i = inner.clone();
    let ddns = ddns.method(
        "neighbors",
        Write,
        &[],
        "The LAN's devices with the public IPv6 addresses the router has seen, to take a suffix from.",
        move |_| {
            let i = i.clone();
            async move { ddns::neighbors(&i).await }
        },
    );
    let i = inner.clone();
    let ddns = ddns
        .method("update", Write, &[], "Updates every record now.", move |_| {
            let i = i.clone();
            async move { ddns::update_now(&i).await }
        })
        .build();
    vec![balance, remote, ddns]
}

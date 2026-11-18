//! The `f50.sms` object.
//!
//! Message text and phone numbers are private: every method that returns them (`list`, `list_unread`, `read`) is
//! `Access::Write`, as is everything that changes the pool or the modem. `unread` returns counts alone and is
//! `Access::Read`.
//!
//! - `list {page, per_page, peer}`: without `peer`, the conversations (one per peer, newest first: message and
//!   unread counts, the newest message's id, direction, status, time and preview), with the pool's totals, the
//!   modem store's capacity and the network's SMS paths (`transport`: IMS, CS, 5G SMS over NAS, `ready`) as the last
//!   sync saw them; with `peer`, that conversation's messages whole, newest first.
//!   Nothing is marked read.
//! - `list_unread {page, per_page}`: unread incoming messages across all peers, newest first. Nothing is marked read.
//! - `read {id}`: one message whole; a received one is marked read.
//! - `send {number, text}`: starts a send job, `{"job": id}`; the job's result is `{"id", "parts", "references"}`.
//! - `delete {id | "all", sim}`: from the pool (remembered, so the SIM cannot bring it back); with `sim`, the matching
//!   messages in the modem's store too.
//! - `sync {}`: restores notifications and syncs the modem's store as a job, `{"job": id, "started": true}`.
//! - `unread {}`: `{"total", "unread"}`.
//! - `push {}`: the Telegram push settings without the token (`enabled`, `token_set`, `token_hint`, `chat_id`, `api`,
//!   `default_api`, `proxy`, `proxy_running`), with the messages queued and the last outcome (`last {at, ok, id, error}`).
//! - `push_set {enabled, token, chat_id, api, proxy}`: changes the settings; an absent field is kept, an empty `api` is
//!   the default (`https://api.telegram.org`); `proxy` (default on) sends through the local proxy while it runs.
//!   Turning push on needs a token and a chat id. Returns what `push` does.
//! - `push_test {token, chat_id, api, proxy}`: sends a test message with the saved settings, changed by the fields given
//!   (not saved), `{"sent": true}`; a refusal carries Telegram's reason.

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::json;

use crate::pool::Page;
use crate::push::PushChange;
use crate::{Sms, parse_id};

const ID_RE: &str = "[1-9][0-9]{0,8}";

const PAGE_NUMBER: Field = Field::opt("page", Rule::Int { min: 1, max: 100_000 });
const PAGE_SIZE: Field = Field::opt("per_page", Rule::Int { min: 1, max: 100 });
const PAGINATION: &[Field] = &[PAGE_NUMBER, PAGE_SIZE];
const LIST: &[Field] = &[
    PAGE_NUMBER,
    PAGE_SIZE,
    Field::opt("peer", Rule::Text { max_bytes: 64, max_chars: 32 }),
];
const ID: &[Field] = &[Field::req("id", Rule::WordNum { re: ID_RE, max: 9 })];
const SEND: &[Field] = &[
    Field::req("number", Rule::Word { re: r"\+?[0-9]{1,20}", max: 21 }),
    Field::secret("text", Rule::Body { max_chars: 1530 }, true),
];
const DELETE: &[Field] =
    &[Field::req("id", Rule::WordNum { re: "all|[1-9][0-9]{0,8}", max: 9 }), Field::opt("sim", Rule::Bool)];
/// A bot token (`<bot id>:<secret>`), a chat id (a number, negative for a group, or `@channel`), an http(s) Bot API
/// base without a query.
const TOKEN: Rule = Rule::Word { re: r"|[0-9]{1,20}:[A-Za-z0-9_-]{20,100}", max: 121 };
const CHAT_ID: Rule = Rule::Word { re: r"|-?[0-9]{1,20}|@[A-Za-z][A-Za-z0-9_]{3,31}", max: 33 };
const API: Rule =
    Rule::Word { re: r"|https?://[A-Za-z0-9.-]{1,253}(:[0-9]{1,5})?(/[A-Za-z0-9._~%/-]{0,200})?", max: 480 };
const PUSH_SET: &[Field] = &[
    Field::opt("enabled", Rule::Bool),
    Field::secret("token", TOKEN, false),
    Field::secret("chat_id", CHAT_ID, false),
    Field::opt("api", API),
    Field::opt("proxy", Rule::Bool),
];
const PUSH_TEST: &[Field] = &[
    Field::secret("token", TOKEN, false),
    Field::secret("chat_id", CHAT_ID, false),
    Field::opt("api", API),
    Field::opt("proxy", Rule::Bool),
];

fn push_change(p: &Params) -> PushChange {
    let s = |k: &str| p.str(k).map(str::to_owned);
    PushChange {
        enabled: p.bool("enabled"),
        token: s("token"),
        chat_id: s("chat_id"),
        api: s("api"),
        proxy: p.bool("proxy"),
    }
}

fn page(p: &Params) -> Page {
    Page {
        page: usize::try_from(p.int("page").unwrap_or(1)).unwrap_or(1),
        per_page: usize::try_from(p.int("per_page").unwrap_or(20)).unwrap_or(20),
    }
}

pub(crate) fn object(sms: Sms) -> Object {
    let (a, b, c, d, e, f) = (sms.clone(), sms.clone(), sms.clone(), sms.clone(), sms.clone(), sms.clone());
    let (g, h, i, unread) = (sms.clone(), sms.clone(), sms.clone(), sms);
    Object::builder("f50.sms")
        .method(
            "list",
            Access::Write,
            LIST,
            "Conversations newest first with unread counts, or one conversation's messages (peer).",
            move |p: Params| {
                let s = a.clone();
                async move { s.list(page(&p), p.str("peer").map(str::to_owned)).await }
            },
        )
        .method(
            "list_unread",
            Access::Write,
            PAGINATION,
            "Unread incoming messages across all conversations, newest first, without marking them read.",
            move |p: Params| {
                let s = unread.clone();
                async move { s.list_unread(page(&p)).await }
            },
        )
        .method("read", Access::Write, ID, "One message whole; a received one is marked read.", move |p: Params| {
            let s = b.clone();
            async move { s.read(parse_id(p.need_str("id")?)?).await }
        })
        .method("send", Access::Write, SEND, "Sends a message (up to 10 parts) as a job.", move |p: Params| {
            let r = p.need_str("number").and_then(|n| c.send(n, p.need_str("text")?));
            async move { r.map(|job| json!({ "job": job })) }
        })
        .method(
            "delete",
            Access::Write,
            DELETE,
            "Deletes a message or all; with sim, the matching ones in the modem's store too.",
            move |p: Params| {
                let s = d.clone();
                async move {
                    let id = match p.need_str("id")? {
                        "all" => None,
                        v => Some(parse_id(v)?),
                    };
                    s.delete(id, p.flag("sim")).await
                }
            },
        )
        .method(
            "sync",
            Access::Write,
            &[],
            "Restores SMS notifications and moves stored messages into the pool as a job.",
            move |_p: Params| {
                let r = e.sync();
                async { r.map(|job| json!({ "job": job, "started": true })) }
            },
        )
        .method(
            "unread",
            Access::Read,
            &[],
            "How many messages the pool holds, and how many are unread.",
            move |_p: Params| {
                let s = f.clone();
                async move { s.counts().await.map(|(total, unread)| json!({ "total": total, "unread": unread })) }
            },
        )
        .method(
            "push",
            Access::Write,
            &[],
            "The Telegram push settings (never the token), the queue and the last outcome.",
            move |_p: Params| {
                let v = g.push().view();
                async move { Ok(v) }
            },
        )
        .method(
            "push_set",
            Access::Write,
            PUSH_SET,
            "Changes the Telegram push settings; absent fields are kept.",
            move |p: Params| {
                let s = h.clone();
                let change = push_change(&p);
                async move { s.push().set(change).await }
            },
        )
        .method(
            "push_test",
            Access::Write,
            PUSH_TEST,
            "Sends a Telegram test message with the saved settings changed by the given fields (not saved).",
            move |p: Params| {
                let s = i.clone();
                let change = push_change(&p);
                async move { s.push().test(change).await }
            },
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::rpc::validate;

    #[test]
    fn fields_refuse_what_they_must() {
        assert!(validate(SEND, &json!({"number": "+905551112233", "text": "-hi\nthere"})).is_ok());
        for bad in ["+", "12a", "1;AT", "+123456789012345678901", ""] {
            assert!(validate(SEND, &json!({"number": bad, "text": "x"})).is_err(), "{bad}");
        }
        assert!(validate(SEND, &json!({"number": "1", "text": "a".repeat(1531)})).is_err());
        assert!(validate(SEND, &json!({"number": "1", "text": ""})).is_err());
        assert!(validate(DELETE, &json!({"id": "all", "sim": true})).is_ok());
        assert!(validate(DELETE, &json!({"id": 12})).is_ok());
        for bad in [json!("0"), json!("1,4"), json!("-1"), json!("1 2")] {
            assert!(validate(DELETE, &json!({"id": bad})).is_err());
        }
        assert!(validate(ID, &json!({"id": "all"})).is_err());
        assert!(validate(LIST, &json!({"page": 0})).is_err());
        assert!(validate(LIST, &json!({"per_page": 5, "peer": "ADANA BLD"})).is_ok());
        let ok = json!({"enabled": true, "token": "123456789:AAH-abcdefghijklmnopqrstuvwxyz_0", "chat_id": "-1001234567890",
            "api": "https://tg.example.com/proxy"});
        assert!(validate(PUSH_SET, &ok).is_ok());
        assert!(validate(PUSH_SET, &json!({"chat_id": "@my_channel", "api": ""})).is_ok());
        for bad in [
            json!({"token": "abc"}),
            json!({"chat_id": "12a"}),
            json!({"api": "ftp://x"}),
            json!({"api": "https://x/?a=1"}),
            json!({"api": "https://x\"bad"}),
        ] {
            assert!(validate(PUSH_SET, &bad).is_err(), "{bad}");
        }
    }
}

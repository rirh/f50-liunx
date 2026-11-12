//! The `f50.sms` object.
//!
//! Message text and phone numbers are private: every method that returns them (`list`, `read`) is `Access::Write`,
//! as is everything that changes the pool or the modem. `unread` returns counts alone and is `Access::Read`.
//!
//! - `list {page, per_page, peer}`: without `peer`, the conversations (one per peer, newest first: message and
//!   unread counts, the newest message's id, direction, status, time and preview), with the pool's totals and the
//!   modem store's capacity as the last sync saw it; with `peer`, that conversation's messages whole, newest first.
//!   Nothing is marked read.
//! - `read {id}`: one message whole; a received one is marked read.
//! - `send {number, text}`: starts a send job, `{"job": id}`; the job's result is `{"id", "parts", "references"}`.
//! - `delete {id | "all", sim}`: from the pool (remembered, so the SIM cannot bring it back); with `sim`, the matching
//!   messages in the modem's store too.
//! - `sync {}`: a sync of the modem's store, in the background.
//! - `unread {}`: `{"total", "unread"}`.

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::json;

use crate::pool::Page;
use crate::{Sms, parse_id};

const ID_RE: &str = "[1-9][0-9]{0,8}";

const LIST: &[Field] = &[
    Field::opt("page", Rule::Int { min: 1, max: 100_000 }),
    Field::opt("per_page", Rule::Int { min: 1, max: 100 }),
    Field::opt("peer", Rule::Text { max_bytes: 64, max_chars: 32 }),
];
const ID: &[Field] = &[Field::req("id", Rule::WordNum { re: ID_RE, max: 9 })];
const SEND: &[Field] = &[
    Field::req("number", Rule::Word { re: r"\+?[0-9]{1,20}", max: 21 }),
    Field::secret("text", Rule::Body { max_chars: 1530 }, true),
];
const DELETE: &[Field] =
    &[Field::req("id", Rule::WordNum { re: "all|[1-9][0-9]{0,8}", max: 9 }), Field::opt("sim", Rule::Bool)];

pub(crate) fn object(sms: Sms) -> Object {
    let (a, b, c, d, e, f) = (sms.clone(), sms.clone(), sms.clone(), sms.clone(), sms.clone(), sms);
    Object::builder("f50.sms")
        .method(
            "list",
            Access::Write,
            LIST,
            "Conversations newest first with unread counts, or one conversation's messages (peer).",
            move |p: Params| {
                let s = a.clone();
                async move {
                    let page = Page {
                        page: usize::try_from(p.int("page").unwrap_or(1)).unwrap_or(1),
                        per_page: usize::try_from(p.int("per_page").unwrap_or(20)).unwrap_or(20),
                    };
                    s.list(page, p.str("peer").map(str::to_owned)).await
                }
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
            "Moves what the modem's store holds into the pool, in the background.",
            move |_p: Params| {
                e.sync();
                async { Ok(json!({ "started": true })) }
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
    }
}

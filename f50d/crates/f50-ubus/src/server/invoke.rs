//! One invocation of one of our objects: checking what ubusd delivered, running the callback with a deadline, and
//! the reply messages.
//!
//! ubusd forwards a call as INVOKE with OBJID, METHOD, USER/GROUP (when known) and DATA, the header's peer set to
//! the caller's client id (`ubusd_forward_invoke`). The object answers on the same seq and peer with an optional
//! DATA (OBJID + DATA) and then STATUS (STATUS + OBJID), which ubusd routes back to the caller with the peer
//! rewritten to the object id (`ubus_send_reply`, `ubus_complete_deferred_request`, `ubusd_handle_response`).

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::proto::{Body, Message, attr, msg, status, status_msg};
use crate::{Invoke, ObjectSpec, UbusError};

/// Invocations answered at the same time; further INVOKEs wait unread in the socket until one finishes.
pub const MAX_CONCURRENT_INVOCATIONS: usize = 16;
/// How long one invocation may run before the caller gets `UBUS_STATUS_TIMEOUT`. Long work is a job and returns
/// at once, so this only catches handlers that hang.
pub const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a reply goes: the INVOKE's seq and peer, and the object it addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplyTo {
    pub seq: u16,
    pub peer: u32,
    pub objid: u32,
    pub no_reply: bool,
}

/// An INVOKE that names one of our objects and methods.
#[derive(Debug, PartialEq)]
pub(crate) struct Call {
    pub to: ReplyTo,
    pub object: String,
    pub method: String,
    pub args: Value,
}

/// What a finished invocation task returns.
#[derive(Debug)]
pub(crate) struct Done {
    pub to: ReplyTo,
    pub result: Result<Value, u32>,
}

/// Checks an INVOKE against our objects (`ubus_process_invoke`). `Err` carries where to send the failure status,
/// or `None` when the message is too broken to answer.
pub(crate) fn parse(
    m: &Message,
    objects: &[ObjectSpec],
    ids: &HashMap<u32, usize>,
) -> Result<Call, Option<(ReplyTo, u32)>> {
    let attrs = m.attrs().map_err(|_| None)?;
    let objid = attrs.u32(attr::OBJID).ok_or(None)?;
    let no_reply = attrs.get(attr::NO_REPLY).and_then(|a| a.payload.first().copied()).is_some_and(|b| b != 0);
    let to = ReplyTo { seq: m.header.seq, peer: m.header.peer, objid, no_reply };
    let fail = |code| Err(Some((to, code)));
    let Some(spec) = ids.get(&objid).and_then(|&i| objects.get(i)) else {
        return fail(status::NOT_FOUND);
    };
    let Some(method) = attrs.str(attr::METHOD) else {
        return fail(status::INVALID_ARGUMENT);
    };
    if !spec.methods.iter().any(|s| s.name == method) {
        return fail(status::METHOD_NOT_FOUND);
    }
    let args = match attrs.data() {
        Ok(Some(map)) => Value::Object(map),
        Ok(None) | Err(_) => return fail(status::INVALID_ARGUMENT),
    };
    if let (Some(user), Some(group)) = (attrs.str(attr::USER), attrs.str(attr::GROUP)) {
        tracing::trace!(object = %spec.name, method, user, group, "ubus invoke");
    }
    Ok(Call { to, object: spec.name.clone(), method: method.to_owned(), args })
}

/// Runs the callback under [`INVOKE_TIMEOUT`]; dropping the returned future cancels the handler.
pub(crate) async fn run(call: Call, invoke: Invoke) -> Done {
    let to = call.to;
    let fut = invoke(call.object, call.method, call.args);
    let result = tokio::time::timeout(INVOKE_TIMEOUT, fut).await.map_err(|_| status::TIMEOUT);
    Done { to, result }
}

/// The messages that answer `done`: DATA (unless the reply is `null`) and STATUS; nothing when NO_REPLY was set.
pub(crate) fn replies(done: Done) -> Vec<Message> {
    let to = done.to;
    if to.no_reply {
        return Vec::new();
    }
    let data = match done.result {
        Ok(Value::Null) => Ok(None),
        Ok(Value::Object(map)) => data_msg(to, &map).map(Some),
        // ubus replies are tables; anything else is wrapped so the caller still sees it
        Ok(other) => data_msg(to, &Map::from_iter([("value".to_owned(), other)])).map(Some),
        Err(code) => Err(code),
    };
    let (data, code) = match data {
        Ok(d) => (d, status::OK),
        Err(code) => (None, code),
    };
    let mut out: Vec<Message> = data.into_iter().collect();
    match status_msg(to.seq, to.peer, code, Some(to.objid)) {
        Ok(s) => out.push(s),
        Err(e) => tracing::error!(error = %e, "ubus status reply could not be encoded"),
    }
    out
}

/// The failure status for an INVOKE that never reached the callback.
pub(crate) fn refusal(to: ReplyTo, code: u32) -> Option<Message> {
    if to.no_reply {
        return None;
    }
    status_msg(to.seq, to.peer, code, Some(to.objid)).ok()
}

fn data_msg(to: ReplyTo, map: &Map<String, Value>) -> Result<Message, u32> {
    let built: Result<Message, UbusError> = Body::new()
        .u32(attr::OBJID, to.objid)
        .and_then(|b| b.data(map))
        .and_then(|b| b.finish(msg::DATA, to.seq, to.peer));
    built.map_err(|e| {
        // too large for UBUS_MAX_MSGLEN, or a NUL inside a string: libubus reports a failed reply as NO_DATA
        tracing::warn!(error = %e, "ubus reply could not be encoded");
        status::NO_DATA
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ArgType, MethodSpec};

    fn objects() -> Vec<ObjectSpec> {
        vec![ObjectSpec {
            name: "f50.sms".to_owned(),
            methods: vec![MethodSpec { name: "list".to_owned(), args: vec![("box".to_owned(), ArgType::String)] }],
        }]
    }

    fn invoke_msg(method: &str, args: Option<&Value>) -> Result<Message, UbusError> {
        let mut b = Body::new().u32(attr::OBJID, 0x77)?.string(attr::METHOD, method)?;
        b = b.string(attr::USER, "root")?.string(attr::GROUP, "root")?;
        if let Some(Value::Object(map)) = args {
            b = b.data(map)?;
        }
        b.finish(msg::INVOKE, 0x0102, 0x0a)
    }

    #[test]
    fn parses_a_forwarded_invoke() -> Result<(), UbusError> {
        let ids = HashMap::from([(0x77, 0)]);
        let args = json!({"box": "inbox"});
        let call = parse(&invoke_msg("list", Some(&args))?, &objects(), &ids);
        let expected = Call {
            to: ReplyTo { seq: 0x0102, peer: 0x0a, objid: 0x77, no_reply: false },
            object: "f50.sms".to_owned(),
            method: "list".to_owned(),
            args,
        };
        assert_eq!(call, Ok(expected));
        Ok(())
    }

    #[test]
    fn refuses_unknown_method_missing_data_and_unknown_object() -> Result<(), UbusError> {
        let ids = HashMap::from([(0x77, 0)]);
        let to = ReplyTo { seq: 0x0102, peer: 0x0a, objid: 0x77, no_reply: false };
        let args = json!({});
        assert_eq!(
            parse(&invoke_msg("nope", Some(&args))?, &objects(), &ids),
            Err(Some((to, status::METHOD_NOT_FOUND)))
        );
        assert_eq!(parse(&invoke_msg("list", None)?, &objects(), &ids), Err(Some((to, status::INVALID_ARGUMENT))));
        assert_eq!(
            parse(&invoke_msg("list", Some(&args))?, &objects(), &HashMap::new()),
            Err(Some((to, status::NOT_FOUND)))
        );
        Ok(())
    }

    #[test]
    fn no_reply_flag_is_read() -> Result<(), UbusError> {
        let ids = HashMap::from([(0x77, 0)]);
        let Value::Object(args) = json!({}) else { unreachable!("literal is an object") };
        let m = Body::new()
            .u32(attr::OBJID, 0x77)?
            .string(attr::METHOD, "list")?
            .data(&args)?
            .u8(attr::NO_REPLY, 1)?
            .finish(msg::INVOKE, 1, 0x0a)?;
        let call = parse(&m, &objects(), &ids).map_err(|_| UbusError::Protocol("refused".to_owned()))?;
        assert!(call.to.no_reply);
        assert_eq!(refusal(call.to, status::NOT_FOUND), None);
        Ok(())
    }

    #[test]
    fn reply_is_data_then_status() -> Result<(), UbusError> {
        let to = ReplyTo { seq: 3, peer: 0x0a, objid: 0x77, no_reply: false };
        let out = replies(Done { to, result: Ok(json!({"ok": true})) });
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].to_bytes(),
            vec![
                0, 2, 0, 3, 0, 0, 0, 0x0a, // DATA, seq 3, caller's peer
                0, 0, 0, 0x20, // root: 4 + 8 + 20
                0x03, 0, 0, 0x08, 0, 0, 0, 0x77, // OBJID
                0x07, 0, 0, 0x14, // DATA: 4 + 16
                0x87, 0, 0, 0x0d, 0, 2, b'o', b'k', 0, 0, 0, 0, 1, 0, 0, 0, // "ok": BOOL true, raw 4 + 8 + 1
            ]
        );
        assert_eq!(out[1], status_msg(3, 0x0a, status::OK, Some(0x77))?);
        Ok(())
    }

    #[test]
    fn null_reply_is_status_only_and_timeout_is_status() -> Result<(), UbusError> {
        let to = ReplyTo { seq: 3, peer: 0x0a, objid: 0x77, no_reply: false };
        assert_eq!(replies(Done { to, result: Ok(Value::Null) }), vec![status_msg(3, 0x0a, 0, Some(0x77))?]);
        assert_eq!(
            replies(Done { to, result: Err(status::TIMEOUT) }),
            vec![status_msg(3, 0x0a, status::TIMEOUT, Some(0x77))?]
        );
        let quiet = ReplyTo { no_reply: true, ..to };
        assert!(replies(Done { to: quiet, result: Ok(json!({})) }).is_empty());
        Ok(())
    }
}

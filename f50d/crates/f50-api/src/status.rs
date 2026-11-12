//! `f50.status`: the dashboard in one call.
//!
//! The figures come from the other objects' own read methods, called concurrently through the same validation and
//! dispatch a client uses: each part is that method's envelope, so one failing part (the modem restarting) never
//! hides the others, and the aggregate cannot drift from what the parts report. Only read methods are used, so the
//! aggregate is itself a read: nothing private (SMS text, identities, client lists) is in it.

use std::sync::Arc;

use f50_core::rpc::{Access, Object, Params, dispatch, envelope};
use serde_json::{Map, Value, json};

/// The parts: the key in the reply, and the read method it comes from.
pub const PARTS: &[(&str, &str, &str)] = &[
    ("modem", "f50.modem", "status"),
    ("sim", "f50.sim", "get"),
    ("data", "f50.data", "get"),
    ("traffic", "f50.traffic", "get"),
    ("wifi", "f50.wifi", "get"),
    ("wificlient", "f50.wificlient", "status"),
    ("usb", "f50.usb", "get"),
    ("system", "f50.system", "status"),
    ("sms", "f50.sms", "unread"),
    ("update", "f50.update", "status"),
];

pub(crate) fn object(objects: Arc<Vec<Object>>) -> Object {
    Object::builder("f50.status")
        .method("get", Access::Read, &[], "The dashboard: every part's read method in one call.", move |_p: Params| {
            let objs = objects.clone();
            async move { Ok(collect(&objs).await) }
        })
        .build()
}

async fn collect(objects: &[Object]) -> Value {
    let empty = json!({});
    let calls = PARTS.iter().filter(|(_, o, m)| is_read(objects, o, m)).map(|(key, o, m)| {
        let req = &empty;
        async move { (*key, envelope(&dispatch(objects, o, m, req).await)) }
    });
    let parts = join_all(calls).await;
    let mut out = Map::new();
    for (k, v) in parts {
        out.insert(k.to_owned(), v);
    }
    Value::Object(out)
}

fn is_read(objects: &[Object], object: &str, method: &str) -> bool {
    objects
        .iter()
        .find(|o| o.name == object)
        .and_then(|o| o.method(method))
        .is_some_and(|m| m.access == Access::Read && m.fields.iter().all(|f| !f.required))
}

/// The futures run concurrently on this task (no spawn: they borrow the object table).
async fn join_all<F: std::future::Future>(futs: impl Iterator<Item = F>) -> Vec<F::Output> {
    let mut pending: Vec<_> = futs.map(Box::pin).collect();
    let mut out = Vec::with_capacity(pending.len());
    // poll them together; each part has its own deadline inside its method
    let mut results: Vec<Option<F::Output>> = (0..pending.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut all = true;
        for (i, f) in pending.iter_mut().enumerate() {
            if results[i].is_none() {
                match f.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => results[i] = Some(v),
                    std::task::Poll::Pending => all = false,
                }
            }
        }
        if all { std::task::Poll::Ready(()) } else { std::task::Poll::Pending }
    })
    .await;
    out.extend(results.into_iter().flatten());
    out
}

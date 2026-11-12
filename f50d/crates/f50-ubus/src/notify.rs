//! Notifications from f50d's objects to their subscribers (`ubus_notify`), and whether anyone is subscribed.
//!
//! A subscriber is anything that called `ubus_subscribe` on one of our objects: uhttpd's `/ubus/subscribe/<object>`
//! (a browser's event stream), or `ubus subscribe` in a shell. ubusd tells the owner when an object gets its first
//! subscriber and when it loses its last (a NOTIFY carrying `UBUS_ATTR_ACTIVE`), so a publisher does its work only
//! while somebody listens: [`Notifier::subscribed`] is a `watch` that follows it.
//!
//! The queue is bounded and never waits: a notification that does not fit is dropped, since the next one carries
//! newer figures anyway. While the connection to ubusd is down nothing is subscribed.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value};
use tokio::sync::{mpsc, watch};

/// Notifications waiting for the session to send them.
const QUEUE: usize = 64;

/// One notification: `kind` is the event name a subscriber sees (`event: <kind>` on an event stream).
#[derive(Debug)]
pub(crate) struct Note {
    pub object: String,
    pub kind: String,
    pub data: Map<String, Value>,
}

type Active = Arc<HashMap<String, watch::Sender<bool>>>;

/// Sends notifications from the daemon's objects. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Notifier {
    tx: mpsc::Sender<Note>,
    active: Active,
}

/// The receiving end, handed to [`crate::serve_notifying`].
#[derive(Debug)]
pub struct NotifyQueue {
    pub(crate) rx: mpsc::Receiver<Note>,
    pub(crate) active: Active,
}

/// A notifier for `objects` (the names that may notify) and the queue the ubus server reads.
pub fn notifier(objects: &[&str]) -> (Notifier, NotifyQueue) {
    let (tx, rx) = mpsc::channel(QUEUE);
    let active: Active = Arc::new(objects.iter().map(|o| ((*o).to_owned(), watch::channel(false).0)).collect());
    (Notifier { tx, active: active.clone() }, NotifyQueue { rx, active })
}

impl Notifier {
    /// Queues `kind` with `data` (a JSON object) for `object`'s subscribers. `false` when nobody is subscribed, the
    /// object cannot notify, `data` is not an object, or the queue is full (the notification is dropped).
    pub fn notify(&self, object: &str, kind: &str, data: Value) -> bool {
        if !self.is_subscribed(object) {
            return false;
        }
        let Value::Object(data) = data else { return false };
        self.tx.try_send(Note { object: object.to_owned(), kind: kind.to_owned(), data }).is_ok()
    }

    /// Whether `object` has a subscriber now.
    pub fn is_subscribed(&self, object: &str) -> bool {
        self.active.get(object).is_some_and(|w| *w.borrow())
    }

    /// Follows whether `object` has a subscriber; `None` for an object this notifier was not made for.
    pub fn subscribed(&self, object: &str) -> Option<watch::Receiver<bool>> {
        self.active.get(object).map(watch::Sender::subscribe)
    }
}

impl NotifyQueue {
    pub(crate) fn set(active: &Active, object: &str, on: bool) {
        if let Some(w) = active.get(object) {
            w.send_if_modified(|v| std::mem::replace(v, on) != on);
        }
    }

    /// The connection is gone: so is every subscription.
    pub(crate) fn clear(&self) {
        for w in self.active.values() {
            w.send_if_modified(|v| std::mem::replace(v, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn only_while_subscribed() {
        let (n, mut q) = notifier(&["o"]);
        assert!(!n.notify("o", "k", json!({"a": 1})), "nobody listens yet");
        let mut sub = n.subscribed("o").expect("known");
        NotifyQueue::set(&q.active, "o", true);
        sub.changed().await.expect("changed");
        assert!(*sub.borrow());
        assert!(n.notify("o", "k", json!({"a": 1})));
        assert!(!n.notify("o", "k", json!([1])), "data must be an object");
        assert!(!n.notify("other", "k", json!({})), "unknown object");
        let got = q.rx.recv().await.expect("queued");
        assert_eq!((got.object.as_str(), got.kind.as_str()), ("o", "k"));
        q.clear();
        assert!(!n.is_subscribed("o"));
        assert!(n.subscribed("other").is_none());
    }

    #[test]
    fn a_full_queue_drops() {
        let (n, q) = notifier(&["o"]);
        NotifyQueue::set(&q.active, "o", true);
        let sent = (0..QUEUE + 5).filter(|_| n.notify("o", "k", json!({}))).count();
        assert_eq!(sent, QUEUE);
    }
}

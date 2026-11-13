//! `f50.live`: the figures a page shows as they change, pushed to subscribers instead of polled.
//!
//! A browser subscribes through uhttpd (`GET /ubus/subscribe/f50.live`, an event stream; the ACL grants
//! `:subscribe` to `f50-read`) and receives one event per [`TOPICS`] entry: the event name is the topic, the data is
//! the envelope of the read method behind it (`{"ok":true,"data":...}`), exactly what calling that method returns.
//! So a page puts an event straight into the cache it would otherwise fill by polling.
//!
//! Nothing is collected while nobody is subscribed. While somebody is, each topic calls its method on its own
//! period and sends the result when it changed, or at least every [`HEARTBEAT`] (a subscriber that joined later than
//! the first one then hears the unchanging topics too). `f50.live get` returns every topic at once.
//!
//! Transport is the binary's business: [`run`] gets a function that sends one event and a `watch` that says whether
//! anyone listens, and never sees a ubus message.

use std::sync::Arc;
use std::time::Duration;

use f50_core::rpc::{Access, Object, Params, dispatch, envelope};
use serde_json::{Map, Value, json};
use tokio::sync::watch;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// The object's name.
pub const OBJECT: &str = "f50.live";
/// The longest an unchanged topic goes unsent while somebody is subscribed.
pub const HEARTBEAT: Duration = Duration::from_secs(15);

/// One live topic: the event name, the read method behind it, and how often it is read.
#[derive(Debug, Clone, Copy)]
pub struct Topic {
    pub kind: &'static str,
    pub object: &'static str,
    pub method: &'static str,
    pub every: Duration,
}

const fn topic(kind: &'static str, object: &'static str, method: &'static str, ms: u64) -> Topic {
    Topic { kind, object, method, every: Duration::from_millis(ms) }
}

/// What is pushed. Only read methods without required fields: nothing private, nothing that changes the device.
pub const TOPICS: &[Topic] = &[
    // the dashboard
    topic("status", "f50.status", "get", 3000),
    // the system page's meters and chart
    topic("system", "f50.system", "status", 2000),
    // the signal page
    topic("modem", "f50.modem", "status", 2000),
    topic("cells", "f50.modem", "cells", 5000),
    // the unread count in the navigation
    topic("sms", "f50.sms", "unread", 5000),
    // the log page: a fingerprint only (the logs are private), so it reads them again when they change
    topic("logs", "f50.system", "logs_mark", 2000),
];

/// The topics whose methods exist and are plain reads in `objects` (a topic that is not is never pushed).
fn usable(objects: &[Object]) -> Vec<Topic> {
    TOPICS
        .iter()
        .copied()
        .filter(|t| {
            objects
                .iter()
                .find(|o| o.name == t.object)
                .and_then(|o| o.method(t.method))
                .is_some_and(|m| m.access == Access::Read && m.fields.iter().all(|f| !f.required))
        })
        .collect()
}

async fn read(objects: &[Object], t: &Topic) -> Value {
    envelope(&dispatch(objects, t.object, t.method, &json!({})).await)
}

/// The `f50.live` object: `get` answers every topic now. `objects` are the ones the topics read.
pub(crate) fn object(objects: Arc<Vec<Object>>) -> Object {
    Object::builder(OBJECT)
        .method(
            "get",
            Access::Read,
            &[],
            "Every live topic now (the events a subscription to f50.live sends, by topic).",
            move |_p: Params| {
                let objs = objects.clone();
                async move {
                    let mut out = Map::new();
                    for t in usable(&objs) {
                        out.insert(t.kind.to_owned(), read(&objs, &t).await);
                    }
                    Ok(Value::Object(out))
                }
            },
        )
        .build()
}

/// Sends one event: `(topic, envelope)`; `false` when it was not sent (nobody listens, the queue is full).
pub type Publish = Arc<dyn Fn(&str, Value) -> bool + Send + Sync>;

/// Publishes every topic while `subscribed` is true, until `shutdown`. One task per topic, on `tracker`.
pub fn run(
    objects: Arc<Vec<Object>>,
    publish: Publish,
    subscribed: watch::Receiver<bool>,
    tracker: &TaskTracker,
    shutdown: CancellationToken,
) {
    for t in usable(&objects) {
        tracker.spawn(topic_loop(t, objects.clone(), publish.clone(), subscribed.clone(), shutdown.clone()));
    }
}

async fn topic_loop(
    t: Topic,
    objects: Arc<Vec<Object>>,
    publish: Publish,
    mut subscribed: watch::Receiver<bool>,
    shutdown: CancellationToken,
) {
    loop {
        // idle until somebody subscribes
        while !*subscribed.borrow_and_update() {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                r = subscribed.changed() => if r.is_err() { return },
            }
        }
        tracing::debug!(topic = t.kind, "live topic started");
        let mut tick = tokio::time::interval(t.every);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last: Option<(Value, Instant)> = None;
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                r = subscribed.changed() => {
                    if r.is_err() {
                        return;
                    }
                    if !*subscribed.borrow_and_update() {
                        break;
                    }
                }
                _ = tick.tick() => {
                    let v = read(&objects, &t).await;
                    if due(last.as_ref(), &v, Instant::now()) && publish(t.kind, v.clone()) {
                        last = Some((v, Instant::now()));
                    }
                }
            }
        }
        tracing::debug!(topic = t.kind, "live topic stopped: nobody subscribed");
    }
}

/// Whether `v` should be sent: it changed since the last one sent, or that one is [`HEARTBEAT`] old.
fn due(last: Option<&(Value, Instant)>, v: &Value, now: Instant) -> bool {
    last.is_none_or(|(prev, at)| prev != v || now.duration_since(*at) >= HEARTBEAT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn counter() -> Object {
        let n = Arc::new(std::sync::atomic::AtomicU64::new(0));
        Object::builder("f50.system")
            .method("status", Access::Read, &[], "t", move |_p: Params| {
                let v = n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async move { Ok(json!({ "n": v })) }
            })
            .build()
    }

    #[test]
    fn only_plain_reads_are_topics() {
        let objs = vec![counter()];
        let kinds: Vec<_> = usable(&objs).iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec!["system"]);
    }

    #[test]
    fn sent_when_changed_or_stale() {
        let t0 = Instant::now();
        let a = json!({"x": 1});
        assert!(due(None, &a, t0));
        let last = (a.clone(), t0);
        assert!(!due(Some(&last), &a, t0 + Duration::from_secs(1)));
        assert!(due(Some(&last), &json!({"x": 2}), t0 + Duration::from_secs(1)));
        assert!(due(Some(&last), &a, t0 + HEARTBEAT));
    }

    #[tokio::test(start_paused = true)]
    async fn publishes_only_while_subscribed() {
        let objs = Arc::new(vec![counter()]);
        let sent: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let s = sent.clone();
        let publish: Publish = Arc::new(move |k, v| {
            s.lock().expect("lock").push((k.to_owned(), v));
            true
        });
        let (sub_tx, sub_rx) = watch::channel(false);
        let tracker = TaskTracker::new();
        let shutdown = CancellationToken::new();
        run(objs, publish, sub_rx, &tracker, shutdown.clone());

        tokio::time::sleep(Duration::from_secs(10)).await;
        assert!(sent.lock().expect("lock").is_empty(), "nothing while nobody listens");

        sub_tx.send(true).expect("send");
        tokio::time::sleep(Duration::from_millis(4100)).await;
        let n = sent.lock().expect("lock").len();
        assert_eq!(n, 3, "at 0, 2 and 4 s");
        let (kind, v) = sent.lock().expect("lock")[0].clone();
        assert_eq!(kind, "system");
        assert_eq!(v, json!({"ok": true, "data": {"n": 0}}));

        sub_tx.send(false).expect("send");
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(sent.lock().expect("lock").len(), n, "stops when the last subscriber leaves");

        shutdown.cancel();
        tracker.close();
        tracker.wait().await;
    }
}

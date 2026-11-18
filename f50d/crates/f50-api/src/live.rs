//! Live figures pushed to subscribers instead of polled.
//!
//! A browser subscribes through uhttpd (`GET /ubus/subscribe/f50.live`, an event stream; the ACL grants
//! `:subscribe` to `f50-read`) and receives events named after [`TOPICS`] entries: the data is the envelope of the
//! read method behind it (`{"ok":true,"data":...}`), exactly what calling that method returns. So a page puts an
//! event straight into the cache it would otherwise fill by polling.
//!
//! `f50.live` subscribers receive every topic. Pages can subscribe to `f50.live.topics` instead and use its `want`
//! method to lease topics for [`LEASE_TTL`] (renew by calling again; empty `topics` releases the lease). Subscribers
//! on that stream receive the union of topics currently leased by pages.
//!
//! Each running topic is read once per period whoever wants it (one producer per topic, its events go to every
//! subscriber), and through the shared [`Samples`]: the `f50.status` aggregate and `get` reuse the same readings.
//! A topic's event is sent when it changed, or at least every [`HEARTBEAT`]; `want` answers with the latest reading
//! of each topic it grants, so a page that joins a running topic need not wait for its next change.
//!
//! Transport is the binary's business: [`Live::run`] gets a function that sends one event and a `watch` per stream
//! that says whether anyone listens, and never sees a ubus message.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::{Map, Value, json};
use tokio::sync::{Notify, watch};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::samples::Samples;

/// The original stream, which publishes every topic.
pub const OBJECT: &str = "f50.live";
/// The stream that publishes leased topics.
pub const TOPICS_OBJECT: &str = "f50.live.topics";
pub const STREAMS: [&str; 2] = [OBJECT, TOPICS_OBJECT];
/// The longest an unchanged topic goes unsent while somebody is subscribed.
pub const HEARTBEAT: Duration = Duration::from_secs(15);
/// How long a `want` holds its topics; a page renews well within it (a background tab's timers may run only once
/// a minute).
pub const LEASE_TTL: Duration = Duration::from_secs(120);
/// Leases kept at once (one per open tab); the one closest to expiry goes first beyond it.
const MAX_LEASES: usize = 64;

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
    // the proxy page: state, node, traffic counters (the page derives the speed), groups
    topic("proxy", "f50.proxy", "status", 2000),
];

/// The topic named `kind`, if it is one.
pub fn topic_of(kind: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.kind == kind)
}

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

const WANT_FIELDS: &[Field] = &[
    // one per tab, made by the page (no session is visible here: uhttpd checked it)
    Field::req("client", Rule::Word { re: "[A-Za-z0-9_-]{8,64}", max: 64 }),
    // comma-separated topic names; empty or absent releases the lease's topics
    Field::opt("topics", Rule::Word { re: "(?:[a-z]+(?:,[a-z]+)*)?", max: 256 }),
];

/// What one client wants, until when.
#[derive(Debug)]
struct Lease {
    topics: HashSet<&'static str>,
    until: Instant,
}

#[derive(Debug)]
struct Inner {
    objects: Arc<Vec<Object>>,
    topics: Vec<Topic>,
    samples: Samples,
    leases: Mutex<HashMap<String, Lease>>,
    /// A lease changed: the topics that run are worked out again.
    changed: Notify,
}

/// The live topics: their leases, their producers, the `f50.live` object. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Live {
    inner: Arc<Inner>,
}

impl Live {
    /// `objects` are the ones the topics read; `samples` the readings shared with `f50.status`.
    pub fn new(objects: Arc<Vec<Object>>, samples: Samples) -> Live {
        let topics = usable(&objects);
        Live {
            inner: Arc::new(Inner {
                objects,
                topics,
                samples,
                leases: Mutex::new(HashMap::new()),
                changed: Notify::new(),
            }),
        }
    }

    async fn read(&self, t: &Topic, max_age: Duration) -> Value {
        self.inner.samples.read(&self.inner.objects, t.object, t.method, max_age).await
    }

    fn leases(&self) -> std::sync::MutexGuard<'_, HashMap<String, Lease>> {
        self.inner.leases.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records what `client` wants for [`LEASE_TTL`]; the reply names the topics granted (the ones this daemon has)
    /// with their latest readings.
    fn want(&self, client: &str, topics: &str) -> Value {
        let granted: HashSet<&'static str> =
            topics.split(',').filter_map(|k| self.inner.topics.iter().find(|t| t.kind == k).map(|t| t.kind)).collect();
        {
            let mut leases = self.leases();
            let now = Instant::now();
            leases.retain(|_, l| l.until > now);
            if granted.is_empty() {
                leases.remove(client);
            } else {
                if !leases.contains_key(client) && leases.len() >= MAX_LEASES {
                    let oldest = leases.iter().min_by_key(|(_, l)| l.until).map(|(k, _)| k.clone());
                    if let Some(k) = oldest {
                        leases.remove(&k);
                    }
                }
                leases.insert(client.to_owned(), Lease { topics: granted.clone(), until: now + LEASE_TTL });
            }
        }
        self.inner.changed.notify_one();
        let mut data = Map::new();
        for t in self.inner.topics.iter().filter(|t| granted.contains(t.kind)) {
            if let Some((v, age)) = self.inner.samples.latest(t.object, t.method)
                && age <= HEARTBEAT
            {
                data.insert(t.kind.to_owned(), v);
            }
        }
        // in TOPICS order, for a stable reply
        let names: Vec<&str> = self.inner.topics.iter().map(|t| t.kind).filter(|k| granted.contains(k)).collect();
        json!({ "topics": names, "ttl": LEASE_TTL.as_secs(), "data": data })
    }

    /// The topics some unexpired lease names, and when the next lease expires.
    fn wanted(&self, now: Instant) -> (HashSet<&'static str>, Option<Instant>) {
        let mut leases = self.leases();
        leases.retain(|_, l| l.until > now);
        let next = leases.values().map(|l| l.until).min();
        (leases.values().flat_map(|l| l.topics.iter().copied()).collect(), next)
    }

    /// The original `f50.live` object: `get` answers every topic now.
    pub(crate) fn object(&self) -> Object {
        let get = self.clone();
        Object::builder(OBJECT)
            .method(
                "get",
                Access::Read,
                &[],
                "Every live topic now (the events a subscription to f50.live sends, by topic).",
                move |_p: Params| {
                    let live = get.clone();
                    async move {
                        let mut out = Map::new();
                        for t in &live.inner.topics {
                            out.insert(t.kind.to_owned(), live.read(t, t.every).await);
                        }
                        Ok(Value::Object(out))
                    }
                },
            )
            .build()
    }

    /// The `f50.live.topics` object: `want` takes a page's topics.
    pub(crate) fn topics_object(&self) -> Object {
        let want = self.clone();
        Object::builder(TOPICS_OBJECT)
            .method(
                "want",
                Access::Read,
                WANT_FIELDS,
                "The topics a page shows (comma-separated; empty releases them), held for `ttl` seconds: renew \
                 within it. Replies with the topics granted and their latest readings.",
                move |p: Params| {
                    let r = p.need_str("client").map(|c| want.want(c, p.str("topics").unwrap_or("")));
                    async move { r }
                },
            )
            .build()
    }

    /// Publishes to the subscribed streams, in [`STREAMS`] order, with one producer task per topic.
    pub fn run(
        &self,
        publish: Publish,
        subscribed: [watch::Receiver<bool>; STREAMS.len()],
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) {
        let mut switches = Vec::new();
        for t in self.inner.topics.iter().copied() {
            let (tx, rx) = watch::channel([false; STREAMS.len()]);
            switches.push((t.kind, tx));
            tracker.spawn(topic_loop(t, self.clone(), publish.clone(), rx, shutdown.clone()));
        }
        tracker.spawn(coordinate(self.clone(), switches, subscribed, shutdown));
    }
}

/// Chooses which streams need each topic.
async fn coordinate(
    live: Live,
    switches: Vec<(&'static str, watch::Sender<[bool; STREAMS.len()]>)>,
    [mut legacy, mut leased]: [watch::Receiver<bool>; STREAMS.len()],
    shutdown: CancellationToken,
) {
    loop {
        let all = *legacy.borrow_and_update();
        let filtered = *leased.borrow_and_update();
        let (wanted, next_expiry) = live.wanted(Instant::now());
        for (kind, tx) in &switches {
            let run = [all, filtered && wanted.contains(kind)];
            tx.send_if_modified(|cur| std::mem::replace(cur, run) != run);
        }
        let expiry = async {
            match next_expiry {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            r = legacy.changed() => if r.is_err() { return },
            r = leased.changed() => if r.is_err() { return },
            () = live.inner.changed.notified() => {}
            () = expiry => {}
        }
    }
}

/// Sends one event: `(object, topic, envelope)`; `false` when it was not sent (nobody listens, the queue is full).
pub type Publish = Arc<dyn Fn(&str, &str, Value) -> bool + Send + Sync>;

async fn topic_loop(
    t: Topic,
    live: Live,
    publish: Publish,
    mut running: watch::Receiver<[bool; STREAMS.len()]>,
    shutdown: CancellationToken,
) {
    // a reading another reader took within half a period is as good as a new one
    let max_age = t.every / 2;
    loop {
        // idle until somebody wants it
        while !running.borrow_and_update().iter().any(|on| *on) {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                r = running.changed() => if r.is_err() { return },
            }
        }
        tracing::debug!(topic = t.kind, "live topic started");
        let mut tick = tokio::time::interval(t.every);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last: [Option<(Value, Instant)>; STREAMS.len()] = [None, None];
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                r = running.changed() => {
                    if r.is_err() {
                        return;
                    }
                    let active = *running.borrow_and_update();
                    for (on, previous) in active.into_iter().zip(&mut last) {
                        if !on {
                            *previous = None;
                        }
                    }
                    if !active.into_iter().any(|on| on) {
                        break;
                    }
                }
                _ = tick.tick() => {
                    let v = live.read(&t, max_age).await;
                    let active = *running.borrow();
                    for ((object, on), previous) in STREAMS.into_iter().zip(active).zip(&mut last) {
                        if on && due(previous.as_ref(), &v, Instant::now()) && publish(object, t.kind, v.clone()) {
                            *previous = Some((v.clone(), Instant::now()));
                        }
                    }
                }
            }
        }
        tracing::debug!(topic = t.kind, "live topic stopped: nobody wants it");
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

    type Sent = Arc<Mutex<Vec<(String, Value)>>>;

    fn unread() -> Object {
        Object::builder("f50.sms")
            .method("unread", Access::Read, &[], "t", |_p: Params| async { Ok(json!({})) })
            .build()
    }

    fn started(objs: Vec<Object>) -> (Live, Sent, [watch::Sender<bool>; 2], TaskTracker, CancellationToken) {
        let live = Live::new(Arc::new(objs), Samples::default());
        let sent: Sent = Arc::default();
        let s = sent.clone();
        let publish: Publish = Arc::new(move |_, k, v| {
            s.lock().expect("lock").push((k.to_owned(), v));
            true
        });
        let (legacy_tx, legacy_rx) = watch::channel(false);
        let (leased_tx, leased_rx) = watch::channel(false);
        let tracker = TaskTracker::new();
        let shutdown = CancellationToken::new();
        live.run(publish, [legacy_rx, leased_rx], &tracker, shutdown.clone());
        (live, sent, [legacy_tx, leased_tx], tracker, shutdown)
    }

    fn kinds(sent: &Sent) -> HashSet<String> {
        sent.lock().expect("lock").iter().map(|(k, _)| k.clone()).collect()
    }

    async fn stop(tracker: TaskTracker, shutdown: CancellationToken) {
        shutdown.cancel();
        tracker.close();
        tracker.wait().await;
    }

    #[tokio::test(start_paused = true)]
    async fn publishes_only_while_subscribed() {
        let (_live, sent, sub_tx, tracker, shutdown) = started(vec![counter()]);

        tokio::time::sleep(Duration::from_secs(10)).await;
        assert!(sent.lock().expect("lock").is_empty(), "nothing while nobody listens");

        sub_tx[0].send(true).expect("send");
        tokio::time::sleep(Duration::from_millis(4100)).await;
        let n = sent.lock().expect("lock").len();
        assert_eq!(n, 3, "at 0, 2 and 4 s");
        let (kind, v) = sent.lock().expect("lock")[0].clone();
        assert_eq!(kind, "system");
        assert_eq!(v, json!({"ok": true, "data": {"n": 0}}));

        sub_tx[0].send(false).expect("send");
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(sent.lock().expect("lock").len(), n, "stops when the last subscriber leaves");
        stop(tracker, shutdown).await;
    }

    #[tokio::test(start_paused = true)]
    async fn only_the_topics_a_lease_names_run() {
        let (live, sent, sub_tx, tracker, shutdown) = started(vec![counter(), unread()]);
        let r = live.want("tab-aaaaaaaa", "sms,nosuch");
        assert_eq!(r["topics"], json!(["sms"]), "only topics this daemon has are granted");
        sub_tx[1].send(true).expect("send");
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(kinds(&sent), HashSet::from(["sms".to_owned()]), "system is not wanted: not read");

        // a second tab wants the system meters: both run, each read once per period for both tabs
        let r = live.want("tab-bbbbbbbb", "system");
        assert_eq!(r["data"], json!({}), "system has no reading yet");
        tokio::time::sleep(Duration::from_millis(4100)).await;
        let system = sent.lock().expect("lock").iter().filter(|(k, _)| k == "system").count();
        assert_eq!(system, 3, "at 0, 2 and 4 s");
        let r = live.want("tab-aaaaaaaa", "sms,system");
        assert_eq!(r["data"]["system"]["ok"], true, "a running topic's latest reading comes with the lease");

        // both release: nothing runs, though both are still subscribed
        live.want("tab-aaaaaaaa", "");
        live.want("tab-bbbbbbbb", "");
        tokio::time::sleep(Duration::from_millis(10)).await;
        let n = sent.lock().expect("lock").len();
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(sent.lock().expect("lock").len(), n, "released topics stop");
        stop(tracker, shutdown).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_lease_not_renewed_expires() {
        let (live, sent, sub_tx, tracker, shutdown) = started(vec![counter(), unread()]);
        live.want("tab-aaaaaaaa", "system");
        sub_tx[1].send(true).expect("send");
        tokio::time::sleep(LEASE_TTL - Duration::from_secs(1)).await;
        assert_eq!(kinds(&sent), HashSet::from(["system".to_owned()]));
        // The filtered stream is idle after its last lease expires.
        sent.lock().expect("lock").clear();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(kinds(&sent).is_empty());
        assert!(live.wanted(Instant::now()).0.is_empty());
        stop(tracker, shutdown).await;
    }

    #[test]
    fn want_validates() {
        let live = Live::new(Arc::new(vec![counter()]), Samples::default());
        let obj = live.topics_object();
        let m = obj.method("want").expect("want");
        assert!(
            f50_core::rpc::validate(m.fields, &json!({"client": "tab-aaaaaaaa", "topics": "system,modem"})).is_ok()
        );
        assert!(f50_core::rpc::validate(m.fields, &json!({"client": "tab-aaaaaaaa", "topics": ""})).is_ok());
        assert!(f50_core::rpc::validate(m.fields, &json!({"client": "x"})).is_err());
        assert!(f50_core::rpc::validate(m.fields, &json!({"client": "tab-aaaaaaaa", "topics": "a,,b"})).is_err());
    }
}

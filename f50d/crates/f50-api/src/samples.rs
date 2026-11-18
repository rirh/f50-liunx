//! Readings of the read methods behind the live topics, shared by everyone who wants one.
//!
//! A live topic, `f50.live get` and the `f50.status` aggregate all read the same few methods (`f50.system status`,
//! `f50.modem status`, `f50.sms unread`, ...). Each asks here with the age it can accept: a reading at most that old
//! is answered from memory, an older one is read again, once, however many ask at the same moment (the others wait
//! for that one reading). So two topics, or a topic and a dashboard poll, never read the device twice per period.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_core::rpc::{Object, dispatch, envelope};
use serde_json::{Value, json};
use tokio::time::Instant;

type Key = (&'static str, &'static str);

#[derive(Debug, Default)]
struct Slot {
    last: Mutex<Option<(Value, Instant)>>,
    /// Held while the method is read: a second caller waits for that reading instead of making another.
    reading: tokio::sync::Mutex<()>,
}

impl Slot {
    fn fresh(&self, max_age: Duration) -> Option<Value> {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.as_ref().filter(|(_, at)| at.elapsed() <= max_age).map(|(v, _)| v.clone())
    }
}

/// The shared readings, by object and method. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Samples {
    slots: Arc<Mutex<HashMap<Key, Arc<Slot>>>>,
}

impl Samples {
    fn slot(&self, object: &'static str, method: &'static str) -> Arc<Slot> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner).entry((object, method)).or_default().clone()
    }

    /// The envelope of `object.method` (no fields) in `objects`: the last reading when it is at most `max_age`
    /// old, else a new one.
    pub async fn read(
        &self,
        objects: &[Object],
        object: &'static str,
        method: &'static str,
        max_age: Duration,
    ) -> Value {
        let slot = self.slot(object, method);
        if let Some(v) = slot.fresh(max_age) {
            return v;
        }
        let _reading = slot.reading.lock().await;
        // somebody else may have read it while this one waited
        if let Some(v) = slot.fresh(max_age) {
            return v;
        }
        let v = envelope(&dispatch(objects, object, method, &json!({})).await);
        *slot.last.lock().unwrap_or_else(PoisonError::into_inner) = Some((v.clone(), Instant::now()));
        v
    }

    /// The last reading of `object.method` and its age, if there was one.
    pub fn latest(&self, object: &'static str, method: &'static str) -> Option<(Value, Duration)> {
        let slot = self.slots.lock().unwrap_or_else(PoisonError::into_inner).get(&(object, method)).cloned()?;
        let last = slot.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.as_ref().map(|(v, at)| (v.clone(), at.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::rpc::{Access, Params};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn counting() -> (Object, Arc<AtomicU64>) {
        let n = Arc::new(AtomicU64::new(0));
        let c = n.clone();
        let o = Object::builder("f50.system")
            .method("status", Access::Read, &[], "t", move |_p: Params| {
                let v = c.fetch_add(1, Ordering::SeqCst);
                async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok(json!({ "n": v }))
                }
            })
            .build();
        (o, n)
    }

    #[tokio::test(start_paused = true)]
    async fn one_reading_per_period_however_many_ask() {
        let (o, n) = counting();
        let objs = Arc::new(vec![o]);
        let s = Samples::default();
        let max = Duration::from_secs(2);
        let (a, b) =
            tokio::join!(s.read(&objs, "f50.system", "status", max), s.read(&objs, "f50.system", "status", max),);
        assert_eq!(a, b);
        assert_eq!(n.load(Ordering::SeqCst), 1, "two at once are one reading");
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(s.read(&objs, "f50.system", "status", max).await, a, "still fresh");
        assert_eq!(n.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(s.read(&objs, "f50.system", "status", max).await["data"]["n"], 1, "stale: read again");
        assert_eq!(s.latest("f50.system", "status").map(|(v, _)| v["data"]["n"].clone()), Some(json!(1)));
        assert!(s.latest("f50.sms", "unread").is_none());
    }
}

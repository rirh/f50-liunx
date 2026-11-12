//! Key presses from raw key events: a press, a release with how long it was held, and each hold threshold reached
//! while the key is still down (mu300-keys reported a long press while it was held, not on release).

use std::collections::BTreeMap;
use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyEvent {
    Pressed(u16),
    Released {
        code: u16,
        held: Duration,
    },
    /// The key is still down and has been for `threshold`.
    Held {
        code: u16,
        threshold: Duration,
    },
}

/// The keys that are down, and the thresholds each has reached.
#[derive(Debug)]
pub(crate) struct Presses {
    thresholds: Vec<Duration>,
    /// code -> (pressed at, how many thresholds were reported)
    down: BTreeMap<u16, (Instant, usize)>,
}

impl Presses {
    pub(crate) fn new(mut thresholds: Vec<Duration>) -> Self {
        thresholds.sort_unstable();
        thresholds.dedup();
        Presses { thresholds, down: BTreeMap::new() }
    }

    /// One `EV_KEY` event: value 1 a press, 0 a release, 2 the kernel's autorepeat (ignored).
    pub(crate) fn on_key(&mut self, code: u16, value: i32, now: Instant) -> Option<KeyEvent> {
        match value {
            1 => {
                self.down.insert(code, (now, 0));
                Some(KeyEvent::Pressed(code))
            }
            0 => {
                let (at, _) = self.down.remove(&code)?;
                Some(KeyEvent::Released { code, held: now.saturating_duration_since(at) })
            }
            _ => None,
        }
    }

    /// The thresholds reached by `now`.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<KeyEvent> {
        let mut out = Vec::new();
        for (code, (at, n)) in &mut self.down {
            while let Some(t) = self.thresholds.get(*n) {
                if now.saturating_duration_since(*at) < *t {
                    break;
                }
                out.push(KeyEvent::Held { code: *code, threshold: *t });
                *n += 1;
            }
        }
        out
    }

    /// When the next threshold is reached, if a key is down.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.down.values().filter_map(|(at, n)| self.thresholds.get(*n).map(|t| *at + *t)).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_long_and_longer() {
        let s = Duration::from_secs;
        let mut p = Presses::new(vec![s(10), s(3)]);
        let t0 = Instant::now();
        assert_eq!(p.on_key(116, 1, t0), Some(KeyEvent::Pressed(116)));
        assert_eq!(p.next_deadline(), Some(t0 + s(3)));
        assert!(p.due(t0 + s(2)).is_empty());
        assert_eq!(p.due(t0 + s(3)), vec![KeyEvent::Held { code: 116, threshold: s(3) }]);
        assert!(p.due(t0 + s(4)).is_empty(), "reported once");
        assert_eq!(p.next_deadline(), Some(t0 + s(10)));
        assert_eq!(p.on_key(116, 2, t0 + s(5)), None, "autorepeat");
        assert_eq!(p.on_key(116, 0, t0 + s(5)), Some(KeyEvent::Released { code: 116, held: s(5) }));
        assert_eq!(p.next_deadline(), None);
        assert_eq!(p.on_key(116, 0, t0 + s(6)), None, "a release without its press");
    }
}

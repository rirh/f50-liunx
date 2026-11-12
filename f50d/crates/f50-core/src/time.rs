//! Wall-clock and monotonic time in the forms the API uses.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch (0 when the clock is before it: a device without NTP yet).
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Milliseconds elapsed since `t`, saturating.
pub fn age_ms(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
}

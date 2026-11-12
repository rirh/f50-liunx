//! What the card in use says of itself (ready, locked by a PIN or PUK, absent), kept for this boot so that a slot
//! with no card is not dialled over and over (FINDINGS 186; `mu300-sim-state`).
//!
//! Only the card in use is asked: another position may have its SIM power off and answer `+CME ERROR: 10` whatever
//! is in it (FINDINGS 74). `+ECIND: 3,...` is only a hint (SIM errors and PIN changes send it too): it starts a
//! check two seconds later, never a verdict by itself.

use std::sync::Arc;
use std::time::Duration;

use f50_at::{Final, Reply};
use serde::Serialize;

use crate::shared::{Inner, lock};
use crate::sim;

/// The card's own account (`AT+CPIN?`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SimState {
    Absent,
    Ready,
    /// A PIN or PUK is asked.
    Locked,
}

fn classify_line(l: &str) -> Option<SimState> {
    let l = l.trim();
    if let Some(v) = l.strip_prefix("+CPIN:") {
        let v = v.trim();
        return match v {
            "READY" => Some(SimState::Ready),
            "NOT INSERTED" | "SIM NOT INSERTED" => Some(SimState::Absent),
            _ if ["SIM PIN", "SIM PUK", "SIM PIN2", "SIM PUK2", "PH-"].iter().any(|p| v.starts_with(p)) => {
                Some(SimState::Locked)
            }
            _ => None,
        };
    }
    match l.strip_prefix("+CME ERROR:").map(str::trim) {
        Some("10" | "SIM not inserted") => Some(SimState::Absent),
        _ => None,
    }
}

/// The answer to `AT+CPIN?` -> a state, or `None` for an answer that says nothing about the card (busy, another
/// error): the kept state stays then. The last line that says something wins.
pub fn classify(r: &Reply) -> Option<SimState> {
    let mut s = r.lines.iter().filter_map(|l| classify_line(l)).next_back();
    match &r.status {
        Final::CmeError(10) => s = Some(SimState::Absent),
        Final::Other(o) => s = classify_line(o).or(s),
        _ => {}
    }
    s
}

/// The filter of the stock RIL's onSimStatusChanged (as kanoqwq read it, b8d4f222): `+ECIND: 3,<value>[,<cause>
/// [,<number>...]]`, every field a decimal number of at most 10 digits and 32 bits. A card coming up reports 3,100
/// (CPIN still busy) and then 3,0,0,1 (FINDINGS 210).
pub fn sim_event(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("+ECIND:") else { return false };
    let body: String = rest.chars().filter(|c| !matches!(c, ' ' | '\r' | '\t')).collect();
    if !body.starts_with("3,") || body.ends_with(',') || body.contains(",,") {
        return false;
    }
    if !body.bytes().all(|b| b.is_ascii_digit() || b == b',') {
        return false;
    }
    let fields: Vec<&str> = body.split(',').collect();
    if fields.iter().any(|f| f.len() > 10 || f.parse::<u64>().map_or(true, |n| n > u64::from(u32::MAX))) {
        return false;
    }
    let value = fields.get(1).copied().unwrap_or("");
    let cause = fields.get(2).copied().unwrap_or("");
    match value {
        "0" | "2" | "4" | "100" => true,
        "1" => matches!(cause, "1" | "2" | "7" | "25" | "34"),
        _ => false,
    }
}

/// The kept state.
pub(crate) fn current(inner: &Inner) -> Option<SimState> {
    *lock(&inner.sim_state)
}

/// Asks the card in use (one command) and keeps the answer; a card that answers after `absent` makes the data
/// interface available again (FINDINGS 186).
pub(crate) async fn check(inner: &Inner) -> Option<SimState> {
    let new = inner.ctl("AT+CPIN?", 4).await.ok().as_ref().and_then(classify);
    let old = current(inner);
    if let Some(n) = new
        && Some(n) != old
    {
        *lock(&inner.sim_state) = Some(n);
        tracing::info!(slot = inner.slot.number(), from = ?old, to = ?n, "SIM state");
        if n != SimState::Absent && old == Some(SimState::Absent) {
            sim::wan_available(inner).await;
        }
    }
    new.or(old)
}

/// The URC listener: a SIM event starts a check two seconds later; a radio that came on asks once too. A burst of
/// events is one check.
pub(crate) fn spawn_event_listener(inner: &Arc<Inner>) {
    let me = inner.clone();
    let mut urcs = inner.at.urcs();
    inner.tracker.spawn(async move {
        loop {
            tokio::select! {
                () = me.shutdown.cancelled() => break,
                u = urcs.recv() => match u {
                    Ok(u) if sim_event(&u.line) => me.sim_check.notify_one(),
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    });
    let me = inner.clone();
    inner.tracker.spawn(async move {
        loop {
            tokio::select! {
                () = me.shutdown.cancelled() => break,
                () = me.sim_check.notified() => {}
            }
            // the card needs a moment after its event
            tokio::select! {
                () = me.shutdown.cancelled() => break,
                () = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
            if !me.is_restarting() {
                check(&me).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(lines: &[&str], status: Final) -> Reply {
        Reply { lines: lines.iter().map(|s| (*s).to_owned()).collect(), status }
    }

    #[test]
    fn cpin_answers() {
        assert_eq!(classify(&reply(&["+CPIN: READY"], Final::Ok)), Some(SimState::Ready));
        assert_eq!(classify(&reply(&["+CPIN: SIM PIN"], Final::Ok)), Some(SimState::Locked));
        assert_eq!(classify(&reply(&["+CPIN: PH-NET PIN"], Final::Ok)), Some(SimState::Locked));
        assert_eq!(classify(&reply(&["+CPIN: NOT INSERTED"], Final::Ok)), Some(SimState::Absent));
        assert_eq!(classify(&reply(&[], Final::CmeError(10))), Some(SimState::Absent));
        assert_eq!(classify(&reply(&[], Final::CmeError(14))), None);
        assert_eq!(classify(&reply(&[], Final::Error)), None);
    }

    #[test]
    fn ecind_filter() {
        for ok in ["+ECIND: 3,0", "+ECIND: 3,100", "+ECIND: 3,0,0,1", "+ECIND:3,1,25", "+ECIND: 3,4,7"] {
            assert!(sim_event(ok), "{ok}");
        }
        for bad in [
            "+ECIND: 2,0",
            "+ECIND: 3,1,3",
            "+ECIND: 3,",
            "+ECIND: 3,,1",
            "+ECIND: 3,-1",
            "+ECIND: 3,5",
            "+ECIND: 3,0,99999999999",
            "+ECIND: 3,0,4294967296",
            "+CREG: 1",
        ] {
            assert!(!sim_event(bad), "{bad}");
        }
    }
}

//! Whether the network currently gives this SIM any way to carry SMS.
//!
//! The pool, the codec and the AT layer can all be right and still nothing arrives: an SMS needs a path through the
//! network, and there are three.
//!
//! - IMS (SMS over IP): `+CIREG: <n>,<reg_info>`, `reg_info` 1 when registered.
//! - The CS domain, or SGs from LTE: `+CREG: <n>,<stat>`, stat 1/5 (registered) or 6/7 ("SMS only"). On 5G SA the
//!   F50 shows stat 8 (emergency only).
//! - 5G SA SMS over NAS: `+C5GUSMS: <sms_available>,<sms_allowed>`, `sms_allowed` 2 when the network allows it
//!   (27.007 10.1.59; 1 is "not allowed", 0 unknown).
//!
//! Seen on the device (China Mobile, 5G SA, 2026-11): `+CIREG:0,0,0`, `+CREG: 2,8,...`, `+C5GUSMS: 2,1`. No path at
//! all: every send fails and nothing is ever delivered, while every SMS setting of the modem looks right. This
//! reading makes that visible (`f50.sms list`, the send job's log) instead of leaving an empty inbox.
//!
//! Read on the poll lane: it never waits behind a send, and a busy lane just leaves a field unknown.

use std::time::Duration;

use f50_at::{At, SendOpts};
use f50_core::time::unix_now;
use serde::Serialize;

const T_READ: Duration = Duration::from_secs(5);

/// The SMS paths as the modem last reported them; `None`: the modem did not say.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(crate) struct Transport {
    /// Registered on IMS (SMS over IP).
    pub ims: Option<bool>,
    /// Registered in the CS domain (or for SMS only): SMS over CS or SGs.
    pub cs: Option<bool>,
    /// The `+CREG` stat behind `cs` (8: emergency only).
    pub cs_stat: Option<u8>,
    /// 5G SMS over NAS allowed by the network.
    pub nas: Option<bool>,
    /// Some path is up (`true`), every path is known down (`false`), or not known.
    pub ready: Option<bool>,
    /// When it was read (Unix seconds).
    pub at: u64,
}

impl Transport {
    fn new(ims: Option<bool>, cs_stat: Option<u8>, nas: Option<bool>, at: u64) -> Self {
        let cs = cs_stat.map(cs_carries_sms);
        let paths = [ims, cs, nas];
        let ready = if paths.contains(&Some(true)) {
            Some(true)
        } else if paths.iter().all(|p| *p == Some(false)) {
            Some(false)
        } else {
            None
        };
        Transport { ims, cs, cs_stat, nas, ready, at }
    }

    /// In words, for a log or an error (no identifiers in it).
    pub(crate) fn describe(&self) -> String {
        let state = |v: Option<bool>, up: &str, down: &str| match v {
            Some(true) => up.to_owned(),
            Some(false) => down.to_owned(),
            None => "unknown".to_owned(),
        };
        let cs_down = match self.cs_stat {
            Some(s) => format!("not registered (CREG stat {s})"),
            None => "not registered".to_owned(),
        };
        format!(
            "SMS paths: IMS {}, CS {}, 5G SMS over NAS {}",
            state(self.ims, "registered", "not registered"),
            state(self.cs, "registered", &cs_down),
            state(self.nas, "allowed", "not allowed"),
        )
    }
}

/// Reads the three paths (each independently: one the modem refuses stays unknown).
pub(crate) async fn read(at: &At) -> Transport {
    let field = |cmd: &'static str, prefix: &'static str| async move {
        match at.send(cmd, SendOpts::poll(T_READ)).await {
            Ok(r) if r.is_ok() => r.value(prefix).map(str::to_owned),
            _ => None,
        }
    };
    let ims = field("AT+CIREG?", "+CIREG").await.as_deref().and_then(parse_cireg);
    let cs = field("AT+CREG?", "+CREG").await.as_deref().and_then(parse_creg_stat);
    let nas = field("AT+C5GUSMS?", "+C5GUSMS").await.as_deref().and_then(parse_c5gusms);
    Transport::new(ims, cs, nas, unix_now())
}

/// CS stats that carry SMS: registered home or roaming, or registered for SMS only.
fn cs_carries_sms(stat: u8) -> bool {
    matches!(stat, 1 | 5 | 6 | 7)
}

/// `reg_info` of a `+CIREG` read answer (`0,0,0`, `2,1,0`): registered on IMS.
pub(crate) fn parse_cireg(v: &str) -> Option<bool> {
    let reg: u8 = v.split(',').nth(1)?.trim().parse().ok()?;
    Some(reg == 1)
}

/// `stat` of a `+CREG` read answer (`2,8,"0000","0B726003",11`).
pub(crate) fn parse_creg_stat(v: &str) -> Option<u8> {
    v.split(',').nth(1)?.trim().parse().ok()
}

/// `sms_allowed` of `+C5GUSMS: <sms_available>,<sms_allowed>`: allowed (2), else not.
pub(crate) fn parse_c5gusms(v: &str) -> Option<bool> {
    let allowed: u8 = v.split(',').nth(1)?.trim().parse().ok()?;
    Some(allowed == 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        assert_eq!(parse_cireg("0,0,0"), Some(false));
        assert_eq!(parse_cireg("2,1,0"), Some(true));
        assert_eq!(parse_cireg("x"), None);
        assert_eq!(parse_creg_stat("2,8,\"0000\",\"0B726003\",11"), Some(8));
        assert_eq!(parse_creg_stat("0,1"), Some(1));
        assert_eq!(parse_creg_stat("1"), None);
        assert_eq!(parse_c5gusms("2,1"), Some(false));
        assert_eq!(parse_c5gusms("2,2"), Some(true));
        assert_eq!(parse_c5gusms("0,0"), Some(false));
        assert_eq!(parse_c5gusms("2"), None);
    }

    #[test]
    fn readiness() {
        // the device on 5G SA: no path at all
        let sa = Transport::new(Some(false), Some(8), Some(false), 0);
        assert_eq!((sa.cs, sa.ready), (Some(false), Some(false)));
        assert_eq!(
            sa.describe(),
            "SMS paths: IMS not registered, CS not registered (CREG stat 8), 5G SMS over NAS not allowed"
        );
        // LTE with a combined attach: SGs
        assert_eq!(Transport::new(Some(false), Some(1), Some(false), 0).ready, Some(true));
        assert_eq!(Transport::new(Some(false), Some(6), None, 0).ready, Some(true));
        // IMS alone is enough
        assert_eq!(Transport::new(Some(true), None, None, 0).ready, Some(true));
        // a path not read: not claimed down
        assert_eq!(Transport::new(Some(false), Some(8), None, 0).ready, None);
    }
}

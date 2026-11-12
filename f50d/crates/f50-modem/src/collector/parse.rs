//! The standard readings: registration (`+CEREG`, `+C5GREG`), `+CESQ`, the bearer's QoS (`+CGEQOSRDP`), and what
//! the snapshot derives from them: registered, roaming, the RAT, the signal and its bars.

use serde_json::{Value, json};

use super::engmd::{field_f64, one_decimal};
use crate::{Rat, Signal};

/// `+CEREG: 2,1,"01F0","0ABCD123",13` (the value after the prefix) -> `{stat, tac, cid, act}`.
pub fn reg(value: Option<&str>) -> Value {
    let Some(v) = value else { return Value::Null };
    let v = v.replace('"', "");
    let f: Vec<&str> = v.split(',').collect();
    let n = |i: usize| -> Value {
        let s: String = f.get(i).map(|s| s.replace(' ', "")).unwrap_or_default();
        if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse::<u64>().map_or(Value::Null, |x| json!(x))
        } else {
            Value::Null
        }
    };
    let s = |i: usize| -> Value { f.get(i).filter(|s| !s.is_empty()).map_or(Value::Null, |s| json!(s)) };
    json!({ "stat": n(1), "tac": s(2), "cid": s(3), "act": n(4) })
}

/// The registration status of a `reg` fragment.
pub fn stat(reg: &Value) -> Option<u64> {
    reg.get("stat").and_then(Value::as_u64)
}

/// Registered: home (1), roaming (5), or 10, as the shell counted them.
pub fn registered(stat: Option<u64>) -> bool {
    matches!(stat, Some(1 | 5 | 10))
}

/// `+CESQ: 24,99,55,255,36,98` -> RSRP and RSRQ from their indexes (the fallback reading).
pub fn cesq(value: Option<&str>) -> Value {
    let Some(v) = value else { return Value::Null };
    let f: Vec<&str> = v.split(',').map(str::trim).collect();
    let idx = |i: usize| f.get(i).filter(|s| !s.is_empty() && **s != "255").and_then(|s| s.parse::<f64>().ok());
    let rsrp = idx(5).map_or(Value::Null, |x| json!(x - 140.0));
    let rsrq = idx(4).map_or(Value::Null, |x| json!(one_decimal(-19.5 + x * 0.5)));
    json!({ "rsrp": rsrp, "rsrq": rsrq })
}

/// `+CGEQOSRDP: 1,9,0,0,0,0,100000,50000` (this modem glues OK onto the line) -> QCI and the AMBR in Mbit/s.
pub fn qos(value: Option<&str>) -> Value {
    let Some(v) = value else { return Value::Null };
    let v = v.strip_suffix("OK").unwrap_or(v);
    let f: Vec<&str> = v.split(',').collect();
    let Some(qci) = f.get(1).map(|s| s.trim()).filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    else {
        return Value::Null;
    };
    let rate = |i: usize| {
        f.get(i)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|x| *x > 0.0)
            .map_or(Value::Null, |x| json!(one_decimal(x / 1000.0)))
    };
    json!({ "qci": qci.parse::<u64>().unwrap_or(0), "dl": rate(6), "ul": rate(7) })
}

/// The CID behind a SIPA data device of a card: each card has eight, sipa_eth0-7 are slot 1's CID 1-8,
/// sipa_eth8-15 slot 2's (`lib.sh data_cid`).
pub fn data_cid(device: &str, slot_index: u8) -> Option<u8> {
    let n: u8 = device.strip_prefix("sipa_eth")?.parse().ok()?;
    match (slot_index, n) {
        (0, 0..=7) => Some(n + 1),
        (1, 8..=15) => Some(n - 7),
        _ => None,
    }
}

/// The preferred signal source: NR (NSA or SA), then LTE, then the CESQ fallback.
pub fn pick_signal<'a>(nr: &'a Value, lte: &'a Value, cesq: &'a Value) -> (&'static str, &'a Value) {
    if !nr.is_null() {
        ("nr", nr)
    } else if !lte.is_null() {
        ("lte", lte)
    } else {
        ("cesq", cesq)
    }
}

/// The dashboard's 0-10 score: RSRP 40 %, RSRQ 25 %, SINR 35 %, interpolated between anchors, a missing value's
/// weight shared out (`common.js qScore`).
fn score(rsrp: Option<f64>, rsrq: Option<f64>, sinr: Option<f64>) -> Option<f64> {
    fn interp(v: Option<f64>, pts: &[(f64, f64)]) -> Option<f64> {
        let v = v?;
        for w in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            if v <= x0 {
                return Some(y0 + (v - x0) * (y1 - y0) / (x1 - x0));
            }
        }
        pts.last().map(|p| p.1)
    }
    let parts = [
        (interp(rsrp, &[(-120.0, 0.0), (-110.0, 3.5), (-100.0, 6.5), (-90.0, 8.5), (-80.0, 10.0)]), 0.40),
        (interp(rsrq, &[(-20.0, 0.0), (-14.0, 3.5), (-11.0, 6.5), (-8.0, 8.5), (-3.0, 10.0)]), 0.25),
        (interp(sinr, &[(-5.0, 0.0), (0.0, 3.5), (13.0, 6.5), (20.0, 8.5), (30.0, 10.0)]), 0.35),
    ];
    let (mut sum, mut w) = (0.0, 0.0);
    for (p, weight) in parts {
        if let Some(p) = p {
            sum += p * weight;
            w += weight;
        }
    }
    (w > 0.0).then(|| (sum / w).clamp(0.0, 10.0))
}

/// 0-5 bars as the dashboard draws them: none without a reading, else at least one (`home.js`).
pub fn bars(rsrp: Option<f64>, rsrq: Option<f64>, sinr: Option<f64>) -> u8 {
    match score(rsrp, rsrq, sinr) {
        None => 0,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(s) => ((s / 2.0 + 0.5).floor() as u8).clamp(1, 5),
    }
}

/// The snapshot's signal from the preferred source.
pub fn signal(sig: &Value) -> Option<Signal> {
    if sig.is_null() {
        return None;
    }
    let (rsrp, rsrq, sinr) = (field_f64(sig, "rsrp"), field_f64(sig, "rsrq"), field_f64(sig, "sinr"));
    if rsrp.is_none() && rsrq.is_none() && sinr.is_none() {
        return None;
    }
    Some(Signal { rsrp, rsrq, sinr, rssi: None, bars: bars(rsrp, rsrq, sinr) })
}

/// The access technology of a 27.007 `<AcT>`.
pub fn rat_of_act(act: u64) -> Option<Rat> {
    match act {
        0 | 1 | 3 => Some(Rat::Gsm),
        2 | 4..=6 => Some(Rat::Umts),
        7..=10 => Some(Rat::Lte),
        11 | 12 => Some(Rat::Sa),
        13 => Some(Rat::Nsa),
        _ => None,
    }
}

/// The RAT in use: SA when NR is registered on its own (C5GREG), NSA when LTE carries an NR leg, else the LTE or
/// operator AcT. CEREG alone still reports LTE-style AcT on n78, hence C5GREG and the NR serving cell.
pub fn rat(reg4: &Value, reg5: &Value, nr: &Value, cops_act: Option<u64>) -> Option<Rat> {
    let act = |r: &Value| r.get("act").and_then(Value::as_u64);
    if registered(stat(reg5)) && matches!(act(reg5).and_then(rat_of_act), Some(Rat::Sa) | None) {
        return Some(Rat::Sa);
    }
    if registered(stat(reg4)) {
        if !nr.is_null() || act(reg4) == Some(13) {
            return Some(Rat::Nsa);
        }
        return act(reg4).and_then(rat_of_act).or(Some(Rat::Lte));
    }
    cops_act.and_then(rat_of_act).filter(|_| !nr.is_null())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_lines() {
        let r = reg(Some("2,1,\"1A2B\",\"01234567\",7"));
        assert_eq!(r, json!({"stat":1,"tac":"1A2B","cid":"01234567","act":7}));
        assert_eq!(reg(Some("2,0")), json!({"stat":0,"tac":null,"cid":null,"act":null}));
        assert_eq!(reg(None), Value::Null);
        assert!(registered(stat(&r)));
        assert!(!registered(Some(2)));
    }

    #[test]
    fn cesq_indexes() {
        assert_eq!(cesq(Some("99,99,255,255,20,50")), json!({"rsrp":-90.0,"rsrq":-9.5}));
        assert_eq!(cesq(Some("99,99,255,255,255,255")), json!({"rsrp":null,"rsrq":null}));
    }

    #[test]
    fn qos_lines() {
        assert_eq!(qos(Some("3,9,0,0,0,0,100000,50000")), json!({"qci":9,"dl":100.0,"ul":50.0}));
        assert_eq!(qos(Some("1,9,0,0,0,0,1000,0OK")), json!({"qci":9,"dl":1.0,"ul":null}));
        assert_eq!(qos(Some("1,,0")), Value::Null);
    }

    #[test]
    fn cids_of_devices() {
        assert_eq!(data_cid("sipa_eth0", 0), Some(1));
        assert_eq!(data_cid("sipa_eth2", 0), Some(3));
        assert_eq!(data_cid("sipa_eth8", 1), Some(1));
        assert_eq!(data_cid("sipa_eth8", 0), None);
        assert_eq!(data_cid("wwan0", 0), None);
    }

    #[test]
    fn bars_follow_the_dashboard() {
        assert_eq!(bars(None, None, None), 0);
        assert_eq!(bars(Some(-80.0), Some(-3.0), Some(30.0)), 5);
        assert_eq!(bars(Some(-130.0), None, None), 1);
        assert_eq!(bars(Some(-100.0), None, None), 3); // 6.5 / 2 = 3.25
    }

    #[test]
    fn rat_choice() {
        let lte = json!({"stat":1,"act":7});
        let none = json!({"stat":0,"act":null});
        let nr5 = json!({"stat":1,"act":11});
        assert_eq!(rat(&lte, &none, &Value::Null, None), Some(Rat::Lte));
        assert_eq!(rat(&lte, &none, &json!({"band":78}), None), Some(Rat::Nsa));
        assert_eq!(rat(&none, &nr5, &json!({"band":78}), None), Some(Rat::Sa));
        assert_eq!(rat(&none, &none, &Value::Null, Some(7)), None);
    }
}

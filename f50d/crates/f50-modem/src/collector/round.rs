//! One collector round: the reads of its tier, then the snapshot built from them.
//!
//! The engineering reads, the neighbours and the bearer QoS have no useful answer while the modem is searching or
//! has no service: they are skipped then, which avoids a chain of AT timeouts.

use std::time::Instant;

use f50_core::time::unix_now;
use serde_json::{Value, json};

use super::engmd::{lte_neighbours, lte_serving, neighbours, nr_neighbours, nr_serving};
use super::parse::{cesq, pick_signal, rat, reg, registered, signal, stat};
use super::{Collector, Round, Slow, operator};
use crate::RadioSnapshot;

/// The raw answers of a round.
#[derive(Debug, Default)]
struct Reads {
    cfun: Option<String>,
    cereg: Option<String>,
    c5greg: Option<String>,
    cesq: Option<String>,
    sp_lte: Option<String>,
    sp_phy: Option<String>,
    sp_nr: Option<String>,
    cops: Option<String>,
}

impl Collector {
    pub(super) async fn round(&mut self, full: bool, deadline: Instant) {
        let inner = self.inner.clone();
        let r = Round { inner: &inner, deadline };
        let mut x = Reads {
            cfun: r.value("AT+CFUN?", "+CFUN", 2).await,
            cereg: r.value("AT+CEREG?", "+CEREG", 2).await,
            c5greg: r.value("AT+C5GREG?", "+C5GREG", 2).await,
            cesq: r.value("AT+CESQ", "+CESQ", 2).await,
            ..Reads::default()
        };
        if x.cfun.as_deref().map(str::trim) == Some("1") {
            self.saw_radio_on();
        }
        let (reg4, reg5) = (reg(x.cereg.as_deref()), reg(x.c5greg.as_deref()));
        let is_registered = registered(stat(&reg4)) || registered(stat(&reg5));
        if is_registered {
            x.sp_lte = r.engmd("AT+SPENGMD=0,6,0").await;
            x.sp_phy = r.engmd("AT+SPENGMD=0,0,6").await;
            x.sp_nr = r.engmd("AT+SPENGMD=0,14,1").await;
        }
        if full {
            x.cops = r.value("AT+COPS?", "+COPS", 2).await.map(|v| format!("+COPS: {v}"));
            let (named, cscs) = match x.cops.as_deref() {
                Some(c) => self.operator_name(&r, c).await,
                None => (None, None),
            };
            let mut slow = Slow {
                operator: operator::fragment(x.cops.as_deref(), named.as_deref(), cscs.as_deref()),
                cops_act: None,
                qos: Value::Null,
                neigh: Value::Null,
                ts: unix_now(),
            };
            slow.cops_act = slow.operator.get("act").and_then(Value::as_u64);
            if is_registered {
                let ln = r.engmd("AT+SPENGMD=0,6,6").await.map(|l| lte_neighbours(&l)).unwrap_or_default();
                let nn = r.value("AT+SPQ5GNCELLEX", "+SPQ5GNCELLEX", 2).await.map(|l| nr_neighbours(&l));
                slow.neigh = neighbours(nn.unwrap_or_default(), ln);
                slow.qos = self.bearer_qos(&r).await;
            }
            self.identity(&r).await;
            self.own_number(&r).await;
            self.slow = slow;
        }
        self.publish_round(&x, reg4, reg5);
    }

    fn publish_round(&mut self, x: &Reads, reg4: Value, reg5: Value) {
        let ts = unix_now();
        let nothing = [&x.cfun, &x.cereg, &x.cesq, &x.sp_lte, &x.sp_nr, &x.cops].iter().all(|v| v.is_none());
        if nothing {
            self.publish(RadioSnapshot {
                taken: Some(Instant::now()),
                detail: json!({
                    "ts": ts,
                    "error": "the AT channel did not answer (radio off, modem restarting, or channel unavailable)",
                }),
                ..RadioSnapshot::default()
            });
            return;
        }
        self.down = false;
        let lte = x.sp_lte.as_deref().map_or(Value::Null, |l| lte_serving(l, x.sp_phy.as_deref()));
        let nr = x.sp_nr.as_deref().map_or(Value::Null, nr_serving);
        let cesqj = cesq(x.cesq.as_deref());
        let (src, sig) = pick_signal(&nr, &lte, &cesqj);
        let cfun = x.cfun.as_deref().and_then(|c| c.trim().parse::<u8>().ok());
        let (s4, s5) = (stat(&reg4), stat(&reg5));
        let op = &self.slow.operator;
        let operator = op
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| op.get("plmn").and_then(Value::as_str))
            .map(str::to_owned);
        let snap = RadioSnapshot {
            taken: Some(Instant::now()),
            registered: registered(s4) || registered(s5),
            roaming: s4 == Some(5) || s5 == Some(5),
            rat: rat(&reg4, &reg5, &nr, self.slow.cops_act),
            operator,
            signal: signal(sig),
            detail: json!({
                "ts": ts,
                "cfun": cfun,
                "reg": reg4,
                "reg5g": reg5,
                "operator": self.slow.operator,
                "sig_src": src,
                "sig": sig,
                "lte": lte,
                "nr": nr,
                "qos": self.slow.qos,
                "neigh": self.slow.neigh,
                "full_ts": self.slow.ts,
            }),
        };
        self.publish(snap);
    }
}

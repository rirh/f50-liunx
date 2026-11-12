//! Sending: a job (kind `sms`), because a submission can take longer than a request may (rpcd ends a call at 30 s,
//! the modem may take 40 s for its `+CMGS`; the panel lost slow sends that way, FINDINGS 46).
//!
//! One transaction on the control channel, with no other client in between (FINDINGS 46): the SMSC written back
//! unchanged, PDU mode, then each part with `AT+CMGS=<length>`, the modem's "> " prompt, the PDU and Ctrl-Z. The
//! sent message goes into the pool as outgoing once every part was taken. The job is not cancellable: stopping
//! half way would leave a long message half sent, and nothing ever submits a part twice by itself.
//! The number and the text never go into the job's log or result.

use std::sync::Arc;
use std::time::Duration;

use f50_core::jobs::JobCtx;
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::Inner;
use crate::modem::{self, final_text, parse_cmgs};
use crate::pdu::{Coding, Submit};
use crate::pool::{Dir, NewMessage, Status, fingerprint};

/// The longest wait for the control channel before the send gives up.
const SESSION_WAIT: Duration = Duration::from_secs(30);
/// The modem's time to answer one submission.
const CMGS_TIMEOUT: Duration = Duration::from_secs(60);

/// What the job sends: the parts, already encoded (a text that cannot be sent is refused before the job starts).
#[derive(Debug)]
pub(crate) struct Outgoing {
    pub number: String,
    pub text: String,
    pub coding: Coding,
    pub parts: Vec<Submit>,
}

pub(crate) async fn run(svc: Arc<Inner>, ctx: JobCtx, out: Outgoing) -> Result<Value> {
    let n = out.parts.len();
    let mut s = svc.at.session("sms", SESSION_WAIT).await?;
    ctx.log(modem::smsc_refresh(&mut s).await);
    modem::pdu_mode(&mut s).await?;
    let mut refs = Vec::with_capacity(n);
    for (i, p) in out.parts.iter().enumerate() {
        let cmd = format!("AT+CMGS={}", p.tpdu_len);
        let r = s
            .send_with_body(&cmd, &p.pdu, CMGS_TIMEOUT)
            .await
            .map_err(|e| e.context(format!("part {}/{n} ({i} sent before it)", i + 1)))?;
        let mr = r.value("+CMGS").and_then(parse_cmgs);
        match (r.is_ok(), mr) {
            (true, Some(mr)) => refs.push(mr),
            // only the modem's error is quoted, never a bare OK: an answer without +CMGS is no confirmation
            _ => {
                let why = if r.is_ok() { "no +CMGS confirmation".to_owned() } else { final_text(&r.status) };
                return Err(Error::failed(format!(
                    "the modem did not take part {}/{n} ({i} sent before it): {why}",
                    i + 1
                )));
            }
        }
        ctx.log(format!("part {}/{n} taken by the modem", i + 1));
    }
    drop(s);
    let now = unix_now();
    let time = utc_time(now);
    let msg = NewMessage {
        dir: Dir::Out,
        status: Status::Sent,
        fingerprint: fingerprint(&out.number, &time, &out.text),
        peer: out.number,
        time,
        utc_offset: Some(0),
        coding: out.coding,
        parts: u8::try_from(n).unwrap_or(u8::MAX),
        incomplete: None,
        partial: None,
        part_ids: Vec::new(),
        text: out.text,
    };
    let stored = svc.pool.with(move |st| st.store(vec![msg], now)).await;
    let id = match stored {
        Ok(m) => m.first().map(|m| m.id),
        Err(e) => {
            // sent all the same: the result says so, the pool just has no copy
            ctx.log(format!("sent, but not kept in the pool: {}", e.message()));
            None
        }
    };
    Ok(json!({ "id": id, "parts": n, "references": refs }))
}

/// `YYYY-MM-DD HH:MM:SS` in UTC for Unix seconds (days to civil date: H. Hinnant's algorithm).
pub(crate) fn utc_time(secs: u64) -> String {
    let days = i64::try_from(secs / 86400).unwrap_or(0);
    let rem = secs % 86400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::utc_time;

    #[test]
    fn utc_times() {
        assert_eq!(utc_time(0), "1970-01-01 00:00:00");
        assert_eq!(utc_time(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(utc_time(1_790_000_000), "2026-09-21 14:13:20");
    }
}

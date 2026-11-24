//! The SIM's balance (话费余额), asked of the carrier by SMS and read from its reply.
//!
//! - The carrier comes from the IMSI's PLMN (`f50.modem identity`): China Mobile `CXYE` to 10086, China Unicom
//!   `CXHF` to 10010, China Telecom `102` to 10001; `number` and `code` override it.
//! - A query sends the code (`f50.sms send`); the reply is read when it is pooled (`offer`), as is any message from
//!   the carrier that states a balance. A reply to our own query is marked read.
//! - A query is sent only on demand (`f50.balance query`, the SMS remote): each one is an SMS the carrier may
//!   charge for, so nothing asks on a schedule.

use std::sync::LazyLock;
use std::time::Duration;

use f50_core::time::unix_now;
use f50_core::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::inbox::{Incoming, normalize};
use crate::{Inner, store};

const FILE: &str = "balance.json";
/// A query unanswered for this long is over.
pub(crate) const REPLY_WAIT: u64 = 300;

/// "账户余额为9.60元", "余额：12.34元", "可用余额 -3.5 元"; not "余额已不足30元".
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::unwrap_used)]
    Regex::new(r"余额\s*(?:为|是|：|:)?\s*(-?[0-9]+(?:\.[0-9]+)?)\s*元").unwrap()
});

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// The carrier's number and code; empty: from the SIM.
    pub number: String,
    pub code: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct State {
    pub settings: Settings,
    /// Yuan.
    pub amount: Option<f64>,
    /// When it was read (Unix seconds), and the message it came from.
    pub at: Option<u64>,
    pub text: Option<String>,
    /// The last query sent, and where to.
    pub asked_at: Option<u64>,
    pub asked_to: Option<String>,
    pub error: Option<String>,
}

impl State {
    pub fn load(dir: &std::path::Path) -> State {
        store::load(dir, FILE)
    }

    /// A query is waiting for its reply.
    pub fn pending(&self, now: u64) -> bool {
        self.asked_at.is_some_and(|t| now.saturating_sub(t) < REPLY_WAIT && self.at.is_none_or(|at| at < t))
    }

    pub fn view(&self, now: u64) -> serde_json::Value {
        json!({
            "amount": self.amount,
            "at": self.at,
            "text": self.text,
            "asked_at": self.asked_at,
            "pending": self.pending(now),
            "error": self.error,
            "settings": self.settings,
        })
    }
}

/// The balance a message states, in yuan.
pub(crate) fn parse_amount(text: &str) -> Option<f64> {
    AMOUNT.captures(text)?.get(1)?.as_str().parse().ok()
}

/// The carrier's balance query `(number, code)` for a PLMN (the IMSI's first five digits).
pub(crate) fn carrier_query(plmn: &str) -> Option<(&'static str, &'static str)> {
    match plmn {
        "46000" | "46002" | "46004" | "46007" | "46008" | "46013" => Some(("10086", "CXYE")),
        "46001" | "46006" | "46009" | "46010" => Some(("10010", "CXHF")),
        "46003" | "46005" | "46011" | "46012" => Some(("10001", "102")),
        _ => None,
    }
}

async fn query_target(inner: &Inner) -> Result<(String, String)> {
    let s = Inner::lock(&inner.balance).settings.clone();
    if !s.number.is_empty() && !s.code.is_empty() {
        return Ok((s.number, s.code));
    }
    let id = inner.call("f50.modem", "identity", json!({})).await?;
    let imsi = id["ident"]["imsi"].as_str().unwrap_or_default();
    let (n, c) = carrier_query(imsi.get(..5).unwrap_or_default())
        .ok_or_else(|| Error::failed("the carrier is not known: set the number and the query code"))?;
    Ok((if s.number.is_empty() { n.into() } else { s.number }, if s.code.is_empty() { c.into() } else { s.code }))
}

async fn save(inner: &Inner) {
    let st = Inner::lock(&inner.balance).clone();
    if let Err(e) = store::save(&inner.dir, FILE, &st).await {
        tracing::warn!(error = %e, "the balance was not saved");
    }
}

/// Sends a query (unless one is waiting); the state after it.
pub(crate) async fn ask(inner: &Inner) -> Result<State> {
    let _one = inner.asking.lock().await;
    if Inner::lock(&inner.balance).pending(unix_now()) {
        return Ok(Inner::lock(&inner.balance).clone());
    }
    let r = async {
        let (number, code) = query_target(inner).await?;
        inner.call("f50.sms", "send", json!({ "number": number, "text": code })).await?;
        Ok::<_, Error>(number)
    }
    .await;
    {
        let mut st = Inner::lock(&inner.balance);
        match &r {
            Ok(number) => {
                st.asked_at = Some(unix_now());
                st.asked_to = Some(normalize(number));
                st.error = None;
            }
            Err(e) => st.error = Some(e.message().to_owned()),
        }
    }
    save(inner).await;
    r.map(|_| Inner::lock(&inner.balance).clone())
}

/// Sends a query and waits for its reply, up to `wait`.
pub(crate) async fn ask_and_wait(inner: &Inner, wait: Duration) -> Result<State> {
    let asked = ask(inner).await?.asked_at;
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let st = Inner::lock(&inner.balance).clone();
        if st.at.is_some_and(|at| asked.is_none_or(|a| at >= a)) {
            return Ok(st);
        }
    }
    Err(Error::timeout("the carrier's reply"))
}

/// A pooled message: a balance from the carrier is taken. True when it answered our query (it is then marked read).
pub(crate) async fn offer(inner: &Inner, m: &Incoming) -> bool {
    let Some(amount) = parse_amount(&m.text) else { return false };
    let now = unix_now();
    let ours = {
        let mut st = Inner::lock(&inner.balance);
        let from_carrier = st.asked_to.as_deref() == Some(normalize(&m.peer).as_str())
            || (!st.settings.number.is_empty() && normalize(&st.settings.number) == normalize(&m.peer))
            || ["10086", "10010", "10001"].contains(&normalize(&m.peer).as_str());
        if !from_carrier {
            return false;
        }
        let ours = st.pending(now) && st.asked_to.as_deref() == Some(normalize(&m.peer).as_str());
        st.amount = Some(amount);
        st.at = Some(now);
        st.text = Some(m.text.clone());
        st.error = None;
        ours
    };
    tracing::info!(amount, "SIM balance read");
    save(inner).await;
    ours
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_from_carrier_replies() {
        let cmcc = "【余额查询】尊敬的客户，您的135****5712账户余额为9.60元，详细如下：\n1.通用余额为9.60元";
        assert_eq!(parse_amount(cmcc), Some(9.6));
        assert_eq!(parse_amount("您好，您的账户余额：12.34元，实时话费"), Some(12.34));
        assert_eq!(parse_amount("当前可用余额 -3.5 元"), Some(-3.5));
        assert_eq!(parse_amount("您的话费余额已不足30元，为避免欠费停机"), None);
        assert_eq!(parse_amount("流量剩余2GB"), None);
    }

    #[test]
    fn carriers_by_plmn() {
        assert_eq!(carrier_query("46000"), Some(("10086", "CXYE")));
        assert_eq!(carrier_query("46001"), Some(("10010", "CXHF")));
        assert_eq!(carrier_query("46011"), Some(("10001", "102")));
        assert_eq!(carrier_query("45400"), None);
    }

    #[test]
    fn pending_until_answered_or_stale() {
        let mut st = State { asked_at: Some(1000), ..State::default() };
        assert!(st.pending(1100));
        assert!(!st.pending(1000 + REPLY_WAIT));
        st.at = Some(1050);
        assert!(!st.pending(1100));
    }
}

//! The full tier's extra reads: the operator's long name, the QoS of the bearer the data interface uses now, the
//! SIM/module identity and the own number.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use f50_core::time::unix_now;
use regex::Regex;
use serde_json::{Value, json};

use super::ident::{self, Identity, cnum_number, iccid_value, ident_token, ims_number};
use super::operator::{cscs_value, is_named_line, looks_hex_name, numeric_plmn};
use super::parse::{data_cid, qos};
use super::{Collector, Round};
use crate::shared::lock;

#[allow(clippy::expect_used)]
static IMEI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9]{15,17}$").expect("valid pattern"));
#[allow(clippy::expect_used)]
static IMSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9]{6,15}$").expect("valid pattern"));
#[allow(clippy::expect_used)]
static ICCID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9A-Fa-f]{18,22}$").expect("valid pattern"));

impl Collector {
    /// The long name and the character set for `cops` (see `operator`): asked once per PLMN, or the character set
    /// alone (once per boot) for a name the modem gave as UCS2 hex.
    pub(super) async fn operator_name(&self, r: &Round<'_>, cops: &str) -> (Option<String>, Option<String>) {
        if looks_hex_name(cops) {
            let kept = lock(&self.inner.operator).cscs.clone();
            if kept.is_some() {
                return (None, kept);
            }
            let cscs = r.value("AT+CSCS?", "+CSCS", 2).await.and_then(|v| cscs_value(&v));
            lock(&self.inner.operator).cscs.clone_from(&cscs);
            return (None, cscs);
        }
        let Some(plmn) = numeric_plmn(cops) else { return (None, None) };
        if let Some((p, cscs, named)) = lock(&self.inner.operator).named.clone()
            && p == plmn
        {
            return (Some(named), Some(cscs));
        }
        // one line that asks for the long name and puts the numeric format back: the format is the modem's, whoever
        // reads it next gets what it had
        let Some(reply) = r.send("AT+CSCS?;+COPS=3,0;+COPS?;+COPS=3,2", 3).await else { return (None, None) };
        let Some(named) = reply.lines.iter().find(|l| is_named_line(l)).cloned() else { return (None, None) };
        let cscs = reply.value("+CSCS").and_then(cscs_value).unwrap_or_else(|| "unknown".into());
        lock(&self.inner.operator).named = Some((plmn, cscs.clone(), named.clone()));
        (Some(named), Some(cscs))
    }

    /// The `+CGEQOSRDP` of the bearer the data interface is on now. The CID comes from netifd's live device, never a
    /// fixed 1 (that described context 1 whatever the WAN used, and an old bearer's QoS while it was down). The answer
    /// must be for that CID, and afterwards the WAN must still be the same connection (up, same device, its uptime
    /// not started over), or the previous bearer's numbers would be shown for the new one.
    pub(super) async fn bearer_qos(&self, r: &Round<'_>) -> Value {
        let iface = self.inner.cfg.data_interface.clone();
        let status = || async {
            let n = self.inner.platform.netifd();
            tokio::time::timeout(Duration::from_secs(2), n.status(&iface)).await.ok()?.ok()?
        };
        let Some(st) = status().await else { return Value::Null };
        if st["up"] != json!(true) {
            return Value::Null;
        }
        let Some(dev) = st["l3_device"].as_str().map(str::to_owned) else { return Value::Null };
        let Some(cid) = data_cid(&dev, self.inner.slot.index()) else { return Value::Null };
        let Some(age) = st["uptime"].as_u64() else { return Value::Null };
        let Some(v) = r.value(&format!("AT+CGEQOSRDP={cid}"), "+CGEQOSRDP", 2).await else { return Value::Null };
        if v.split(',').next().map(str::trim) != Some(cid.to_string().as_str()) {
            return Value::Null;
        }
        let Some(st2) = status().await else { return Value::Null };
        let same = st2["up"] == json!(true)
            && st2["l3_device"].as_str() == Some(dev.as_str())
            && st2["uptime"].as_u64().is_some_and(|u| u >= age);
        if same { qos(Some(&v)) } else { Value::Null }
    }

    /// The identity: a fresh complete one is kept; a partial or expired one is asked again after `ident_retry`; the
    /// flash cache is served while it is fresh; an expired complete one stays shown until a new one is complete.
    pub(super) async fn identity(&self, r: &Round<'_>) {
        let (inner, cfg) = (&self.inner, &self.inner.cfg);
        let fresh = |id: &Identity| unix_now().saturating_sub(id.ts) <= cfg.ident_max_age.as_secs();
        {
            let st = lock(&inner.ident);
            if st.shown.as_ref().is_some_and(|id| id.complete() && fresh(id)) {
                return;
            }
            if st.read_at.is_some_and(|t| t.elapsed() < cfg.ident_retry) {
                return;
            }
        }
        let cached = ident::load_cache(inner.paths(), inner.slot);
        if let Some((id, age)) = &cached
            && age <= &cfg.ident_max_age.as_secs()
        {
            let mut st = lock(&inner.ident);
            st.shown = Some(id.clone());
            st.read_at = Some(Instant::now());
            return;
        }
        let first = |rep: Option<f50_at::Reply>| rep.and_then(|x| x.lines.into_iter().next());
        let mut id = Identity {
            ts: unix_now(),
            slot: inner.slot.number(),
            mfg: first(r.send("AT+CGMI", 2).await),
            model: first(r.send("AT+CGMM", 2).await),
            fw: first(r.send("AT+CGMR", 2).await),
            ..Identity::default()
        };
        // fifteen zeros is no IMEI (FINDINGS 220); the U30 Air's diag fallback (imei-diag) is not carried over
        id.imei = r
            .send("AT+CGSN", 2)
            .await
            .and_then(|x| ident_token(&x.lines, &IMEI_RE))
            .map(|v| v[..15].to_owned())
            .filter(|v| v != "000000000000000");
        id.imsi = r.send("AT+CIMI", 2).await.and_then(|x| ident_token(&x.lines, &IMSI_RE));
        id.iccid =
            r.send("AT+CCID", 2).await.and_then(|x| ident_token(&x.lines, &ICCID_RE)).and_then(|v| iccid_value(&v));
        let shown = if id.complete() {
            ident::save_cache(inner.paths(), inner.slot, &id);
            Some(id)
        } else if let Some((old, _)) = cached {
            Some(old)
        } else if id.empty() {
            None
        } else {
            Some(id)
        };
        let mut st = lock(&inner.ident);
        st.shown = shown;
        st.read_at = Some(Instant::now());
    }

    /// The SIM's own number: asked on every full round until one is found (IMS registers later than the SIM is
    /// read), then kept for this boot (FINDINGS 49).
    pub(super) async fn own_number(&self, r: &Round<'_>) {
        if lock(&self.inner.ident).own.is_some() {
            return;
        }
        let mut found = r.send("AT+CNUM", 2).await.and_then(|x| cnum_number(&x.lines)).map(|n| (n, "sim"));
        if found.is_none() {
            found = r.send("AT+SPPAURI?", 2).await.and_then(|x| ims_number(&x.lines)).map(|n| (n, "ims"));
        }
        if found.is_some() {
            lock(&self.inner.ident).own = found;
        }
    }
}

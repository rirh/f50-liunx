//! The SIM's own number, written by the user: many cards (China Mobile's among them) leave EF_MSISDN empty, and
//! `AT+CNUM` then answers `+CME ERROR: 22` while IMS gives none either. The number goes where `AT+CNUM` reads it,
//! the card's own-number phonebook (`AT+CPBS="ON"`, entry 1), never on this device's flash (personal data).

use std::sync::Arc;
use std::time::Duration;

use f50_at::{Final, Reply};
use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::collector::ident::cnum_number;
use crate::shared::{Inner, lock};

const T: Duration = Duration::from_secs(5);

/// `+CPBS: "FD"` (some modems add `,used,total`): the storage's name.
fn storage(r: &Reply) -> Option<String> {
    let v = r.value("+CPBS")?;
    let name = v.split('"').nth(1)?;
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| name.to_owned())
}

/// `+905551234567` -> (`905551234567`, 145), `05551234567` -> (`05551234567`, 129). The RPC field allows only
/// an optional `+` and 3 to 15 digits, so the number is safe inside the quoted AT argument.
fn entry(number: &str) -> (&str, u8) {
    match number.strip_prefix('+') {
        Some(d) => (d, 145),
        None => (number, 129),
    }
}

fn refused(what: &str, r: &Reply) -> Error {
    match r.status {
        // 3 operation not allowed, 17/18 SIM PIN2/PUK2 required, 16 incorrect password
        Final::CmeError(3 | 16 | 17 | 18) => {
            Error::forbidden(format!("this SIM does not allow writing its own number ({what}: {:?})", r.status))
        }
        _ => Error::failed(format!("{what}: {:?}", r.status)),
    }
}

/// Writes `number` as the SIM's own number (entry 1 of the ON phonebook), puts the phonebook storage back as it
/// was, and reads the number back with `AT+CNUM`.
pub(crate) async fn write(inner: &Arc<Inner>, number: &str, log: &(dyn Fn(&str) + Sync)) -> Result<Value> {
    let (digits, kind) = entry(number);
    let mut s = inner.at.session("own number", Duration::from_secs(10)).await?;
    let before = storage(&s.send("AT+CPBS?", T).await?);
    let sel = s.send("AT+CPBS=\"ON\"", T).await?;
    if !sel.is_ok() {
        return Err(refused("AT+CPBS=\"ON\"", &sel));
    }
    let written = s.send(&format!("AT+CPBW=1,\"{digits}\",{kind}"), T).await;
    // the storage as it was, whatever the write did (SMS and the stock tools never see ON selected)
    if let Some(b) = before.as_deref().filter(|b| *b != "ON")
        && let Err(e) = s.send(&format!("AT+CPBS=\"{b}\""), T).await
    {
        tracing::warn!(error = %e, storage = b, "the phonebook storage could not be put back");
    }
    let written = written?;
    if !written.is_ok() {
        return Err(refused("AT+CPBW", &written));
    }
    log(&format!("written to the SIM's own-number phonebook (type {kind})"));
    let read = s.send("AT+CNUM", T).await?;
    drop(s);
    let Some(n) = cnum_number(&read.lines) else {
        return Err(Error::failed("the SIM took the number, but AT+CNUM does not read it back"));
    };
    log("read back with AT+CNUM");
    lock(&inner.ident).own = Some((n.clone(), "sim"));
    Ok(json!({ "msisdn": n, "msisdn_src": "sim", "slot": inner.slot.number() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(line: &str) -> Reply {
        Reply { lines: vec![line.to_owned()], status: Final::Ok }
    }

    #[test]
    fn storage_names() {
        assert_eq!(storage(&reply("+CPBS: \"FD\"")).as_deref(), Some("FD"));
        assert_eq!(storage(&reply("+CPBS: \"SM\",3,250")).as_deref(), Some("SM"));
        assert_eq!(storage(&reply("+CPBS: \"\"")), None);
        assert_eq!(storage(&reply("+CPBS: \"F\\\"D\"")), None);
    }

    #[test]
    fn entries() {
        assert_eq!(entry("+905551234567"), ("905551234567", 145));
        assert_eq!(entry("13800138000"), ("13800138000", 129));
    }
}

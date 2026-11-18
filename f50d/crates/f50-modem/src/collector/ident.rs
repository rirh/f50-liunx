//! The SIM/module identity (IMEI, IMSI, ICCID, model, firmware) and the SIM's own number.
//!
//! A complete identity is kept on flash per slot (FINDINGS 174). When the SIM is unavailable, the last modem IMEI
//! can still be kept for that slot. Fifteen zeros is no IMEI (FINDINGS 220). The own number
//! (FINDINGS 49): `AT+CNUM`, else the IMS public identity, on full rounds until one is found, then kept in RAM for
//! this boot; never on flash (personal data), never in the fast tier. Only `Write`-access methods return any of it.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Instant;

use f50_at::Reply;
use f50_core::{Error, Result};
use f50_core::fsx;
use f50_core::paths::Paths;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::Slot;

/// What the identity methods return.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub ts: u64,
    pub slot: u8,
    pub mfg: Option<String>,
    pub model: Option<String>,
    pub fw: Option<String>,
    pub imei: Option<String>,
    pub imsi: Option<String>,
    pub iccid: Option<String>,
}

impl Identity {
    /// IMEI, IMSI and ICCID all read (fifteen zeros is no IMEI; a real one may start with 0).
    pub fn complete(&self) -> bool {
        self.imei.as_deref().is_some_and(|i| i.starts_with(|c: char| c.is_ascii_digit()) && i != "000000000000000")
            && self.imsi.as_deref().is_some_and(|i| i.starts_with(|c: char| c.is_ascii_digit()))
            && self.iccid.as_deref().is_some_and(|i| i.starts_with(|c: char| c.is_ascii_hexdigit()))
    }

    pub fn empty(&self) -> bool {
        self.imei.is_none() && self.imsi.is_none() && self.iccid.is_none()
    }
}

/// The collector's identity state.
#[derive(Debug, Default)]
pub struct IdentState {
    /// What is shown (complete, or partial, or nothing).
    pub shown: Option<Identity>,
    /// When the shown one was read; a partial one is asked again after `ident_retry`.
    pub read_at: Option<Instant>,
    /// The own number and where it came from (`sim`, `ims`).
    pub own: Option<(String, &'static str)>,
}

/// Where the last identity observation is kept (on flash, root only).
pub fn cache_path(paths: &Paths, slot: Slot) -> PathBuf {
    paths.state().join(format!("ident-sim{}.json", slot.number()))
}

/// The persisted identity observation for this slot, even if the SIM fields were unavailable.
pub fn load_cache(paths: &Paths, slot: Slot) -> Option<Identity> {
    let s = fsx::read_opt(&cache_path(paths, slot)).ok()??;
    let id: Identity = serde_json::from_str(&s).ok()?;
    (id.slot == slot.number()).then_some(id)
}

fn original_imei_path(paths: &Paths, slot: Slot) -> PathBuf {
    paths.state().join(format!("original-imei-sim{}", slot.number()))
}

pub(crate) fn load_original_imei(paths: &Paths, slot: Slot) -> Result<Option<String>> {
    let saved = fsx::read_opt(&original_imei_path(paths, slot))?;
    if saved.as_deref().is_some_and(|imei| !valid_imei(imei)) {
        return Err(Error::failed("the saved original IMEI is invalid"));
    }
    Ok(saved)
}

/// Save the first confirmed IMEI independently of the rolling identity cache. Call under `ident_update`.
pub(crate) fn save_original_imei(paths: &Paths, slot: Slot, imei: &str) -> Result<()> {
    if load_original_imei(paths, slot)?.is_some() {
        return Ok(());
    }
    if !valid_imei(imei) {
        return Err(Error::failed("the original IMEI from the modem is invalid"));
    }
    fsx::private_dir(&paths.state())?;
    fsx::write_atomic(&original_imei_path(paths, slot), imei.as_bytes(), 0o600)
}

/// An IMEI must contain 15 decimal digits and satisfy Luhn. An all-zero reading means it is missing.
pub(crate) fn valid_imei(imei: &str) -> bool {
    if imei.len() != 15 || !imei.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = imei
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let digit = u32::from(b - b'0');
            let weighted = if i.is_multiple_of(2) { digit } else { digit * 2 };
            if weighted > 9 { weighted - 9 } else { weighted }
        })
        .sum();
    sum != 0 && sum.is_multiple_of(10)
}

/// The IMEI from a successful `AT+CGSN` reply.
pub(crate) fn modem_imei(reply: &Reply) -> Option<String> {
    #[allow(clippy::expect_used)]
    static IMEI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9]{15,17}$").expect("valid pattern"));
    if !reply.is_ok() {
        return None;
    }
    ident_token(&reply.lines, &IMEI_RE)
        .map(|v| v[..15].to_owned())
        .filter(|v| v != "000000000000000")
}

pub fn save_cache(paths: &Paths, slot: Slot, id: &Identity) {
    let Ok(b) = serde_json::to_vec(id) else { return };
    let r = fsx::private_dir(&paths.state()).and_then(|()| fsx::write_atomic(&cache_path(paths, slot), &b, 0o600));
    if let Err(e) = r {
        tracing::warn!(error = %e, "could not keep the SIM identity");
    }
}

/// The first line of an identity answer that is the value alone, after an optional `+CGSN: `-like prefix and
/// quotes; an error line is never a value. `re` must match the whole value.
pub fn ident_token(lines: &[String], re: &Regex) -> Option<String> {
    lines.iter().find_map(|l| {
        let l: String = l.chars().filter(|c| *c != '\r' && *c != '"').collect();
        if l.to_ascii_lowercase().contains("error") {
            return None;
        }
        let mut v = l.as_str();
        if let Some(rest) = v.strip_prefix(['+', '^']) {
            let name_len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
            if rest[name_len..].starts_with(':') {
                v = &rest[name_len + 1..];
            }
        }
        let v = v.trim_matches(' ');
        re.is_match(v).then(|| v.to_owned())
    })
}

/// An ICCID in upper case; the F that pads a 19-digit one to 20 nibbles is dropped, the letters of one that has
/// them (some eSIM profiles) are kept.
pub fn iccid_value(v: &str) -> Option<String> {
    let mut v = v.to_ascii_uppercase();
    let all_digit_f = v.bytes().all(|b| b.is_ascii_digit() || b == b'F');
    let b = v.as_bytes();
    if all_digit_f && b.len() >= 2 && b[b.len() - 1] == b'F' && b[b.len() - 2].is_ascii_digit() {
        v.pop();
        if v.len() < 18 {
            return None;
        }
    }
    (!v.is_empty()).then_some(v)
}

/// The first number of an `AT+CNUM` answer, `+CNUM: "alpha","number",type`: digits (spaces, dashes, brackets
/// dropped), 3 to 15 of them, "+" put in front for type 145 (international).
pub fn cnum_number(lines: &[String]) -> Option<String> {
    for l in lines.iter().filter(|l| l.starts_with("+CNUM:")) {
        let f: Vec<&str> = l.split('"').collect();
        let n: String = f.get(3).unwrap_or(&"").chars().filter(|c| !matches!(c, ' ' | '(' | ')' | '-')).collect();
        let d = n.strip_prefix('+').unwrap_or(&n);
        if d.len() < 3 || d.len() > 15 || !d.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let rest = f.get(4).unwrap_or(&"").trim_start_matches([' ', '\t']);
        let intl = rest.strip_prefix(',').map(|r| r.trim_start_matches([' ', '\t'])).is_some_and(|r| {
            r.strip_prefix("145").is_some_and(|t| {
                let t = t.trim_start_matches([' ', '\t']);
                t.is_empty() || t.starts_with(',')
            })
        });
        return Some(if intl && !n.starts_with('+') { format!("+{n}") } else { n });
    }
    None
}

/// The operator-provisioned IMS public identity (`AT+SPPAURI?`): a tel: URI first, else a sip:/sips: URI whose user
/// part is an international number. A bare numeric SIP user is the IMSI on many networks: not taken.
pub fn ims_number(lines: &[String]) -> Option<String> {
    let mut sip = None;
    for l in lines.iter().filter_map(|l| l.strip_prefix("+SPPAURI:")) {
        for uri in l.split([' ', '\t', '"', ',', '<', '>']).filter(|s| !s.is_empty()) {
            let lower = uri.to_ascii_lowercase();
            let tel = lower.starts_with("tel:");
            if !tel && !lower.starts_with("sip:") && !lower.starts_with("sips:") {
                continue;
            }
            let after = uri.split_once(':').map_or("", |(_, r)| r);
            let user = after.split(['@', ';', '?']).next().unwrap_or("");
            let n: String = user.chars().filter(|c| !matches!(c, ' ' | '(' | ')' | '.' | '-')).collect();
            let d = n.strip_prefix('+').unwrap_or(&n);
            if d.len() < 3 || d.len() > 15 || !d.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if tel {
                return Some(n);
            }
            if n.starts_with('+') && sip.is_none() {
                sip = Some(n);
            }
        }
    }
    sip
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().filter(|l| *l != "OK").map(str::to_owned).collect()
    }

    #[test]
    fn iccid_forms() {
        let re = Regex::new("^[0-9A-Fa-f]{18,22}$").unwrap();
        let cases = [
            ("+CCID: 8990011234567890123", Some("8990011234567890123")),
            ("+CCID: \"89900112345678901234\"", Some("89900112345678901234")),
            ("+CCID: 8990011234567890123F", Some("8990011234567890123")),
            ("+CCID: 89860a1b2c3d4e5f6071", Some("89860A1B2C3D4E5F6071")),
            ("^ICCID: 98680021436587092143", Some("98680021436587092143")),
            ("8944500102198304826", Some("8944500102198304826")),
            ("+CME ERROR: 10", None),
            ("+CCID: 12345", None),
            ("+CCID: 8990011234567890123ZZ", None),
        ];
        for (reply, want) in cases {
            let got = ident_token(&lines(reply), &re).and_then(|v| iccid_value(&v));
            assert_eq!(got.as_deref(), want, "{reply}");
        }
    }

    #[test]
    fn imei_and_completeness() {
        let re = Regex::new("^[0-9]{15,17}$").unwrap();
        assert_eq!(ident_token(&lines("+CGSN: 861234567890123"), &re).as_deref(), Some("861234567890123"));
        let mut id = Identity {
            imei: Some("000000000000000".into()),
            imsi: Some("286011234567890".into()),
            iccid: Some("8990011234567890123".into()),
            ..Identity::default()
        };
        assert!(!id.complete());
        id.imei = Some("012345678901234".into());
        assert!(id.complete());
    }

    #[test]
    fn own_numbers() {
        let cases = [
            ("+CNUM: \"Line 1\",\"05551234567\",129", "ERROR", Some("05551234567")),
            ("+CNUM: \"\",\"905551234567\",145", "ERROR", Some("+905551234567")),
            ("+CNUM: \"\",\"+90 555 123-4567\",145", "ERROR", Some("+905551234567")),
            ("+CNUM: \"\",\"\",129", "+SPPAURI: \"tel:+905551234567\"", Some("+905551234567")),
            ("ERROR", "+SPPAURI: \"sip:+905551234567@ims.mnc001.mcc286.3gppnetwork.org\"", Some("+905551234567")),
            ("ERROR", "+SPPAURI: \"sip:286016012345678@ims.mnc001.mcc286.3gppnetwork.org\"", None),
            ("+CNUM: \"\",\"not a number\",129", "+SPPAURI: \"sip:x@y\"", None),
            ("+CNUM: \"\",\"12345678901234567890\",129", "ERROR", None),
        ];
        for (cnum, ims, want) in cases {
            let got = cnum_number(&lines(cnum)).or_else(|| ims_number(&lines(ims)));
            assert_eq!(got.as_deref(), want, "{cnum} / {ims}");
        }
        let both = "+SPPAURI: \"sip:+8613800000000@ims.mnc001.mcc460.3gppnetwork.org\",\"tel:+8613800000001\"";
        assert_eq!(ims_number(&lines(both)).as_deref(), Some("+8613800000001"));
    }
}

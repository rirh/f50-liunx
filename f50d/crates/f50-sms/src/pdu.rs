//! SMS PDUs (3GPP TS 23.040): SMS-DELIVER and SMS-STATUS-REPORT decoded, SMS-SUBMIT built.
//!
//! PDU mode is not a preference. In text mode this modem mangles a sender that is a name rather than a number
//! ("144414+4140224P444" for `ADANA BLD`, whatever `AT+CSCS` says), and text mode cannot tell that stored messages
//! are the parts of one long message (docs/research/FINDINGS.md 26c). Everything here is a pure function of its
//! input; the modem conversation is `modem`'s.

mod decode;
mod encode;
mod gsm7;

use serde::{Deserialize, Serialize};

pub use decode::{Decoded, Deliver, StatusReport, decode};
pub use encode::{MAX_PARTS, Submit, encode_submit};

/// How a message's text is coded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coding {
    /// The GSM 7-bit default alphabet (with a national language table when the header names one).
    #[serde(rename = "gsm")]
    Gsm,
    /// 8-bit data, shown as Latin-1.
    #[serde(rename = "8bit")]
    Eight,
    /// UCS-2 (UTF-16 on the wire, surrogate pairs included).
    #[serde(rename = "ucs2")]
    Ucs2,
}

/// The concatenation header element of one part of a long message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Concat {
    /// The reference number (8 or 16 bits wide on the wire).
    pub reference: u16,
    pub total: u8,
    /// 1-based.
    pub part: u8,
}

/// A service centre timestamp: the sender's local time and its offset from UTC.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timestamp {
    /// `YYYY-MM-DD HH:MM:SS`, empty when the octets are not a time.
    pub local: String,
    /// Minutes east of UTC.
    pub offset_min: Option<i16>,
}

/// Uppercase hexadecimal of `bytes`.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// The octets of a hexadecimal string (either case); `None` for an odd length or a non-hex digit.
pub(crate) fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            let hi = char::from(p[0]).to_digit(16)?;
            let lo = char::from(p[1]).to_digit(16)?;
            u8::try_from(hi * 16 + lo).ok()
        })
        .collect()
}

/// A line that is a whole PDU as the modem prints one: an even number of hexadecimal digits, at least two octets.
/// (A PDU with its SMSC is under 180 octets; anything over 512 is not one.)
pub(crate) fn is_pdu_line(s: &str) -> bool {
    s.len() >= 4 && s.len().is_multiple_of(2) && s.len() <= 1024 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A UCS-2 part of a long message, as tests/test_sms_pool.py builds one (from +31641600986, at `at`).
#[cfg(test)]
pub(crate) fn test_part(reference: u8, total: u8, n: u8, text: &str, at: &str) -> String {
    let mut ud = vec![5, 0, 3, reference, total, n];
    for u in text.encode_utf16() {
        ud.extend(u.to_be_bytes());
    }
    let ts: String = at
        .get(2..)
        .unwrap_or_default()
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.chars().rev().collect::<String>())
        .collect();
    format!("00440B911346610089F60008{ts}00{:02X}{}", ud.len(), to_hex(&ud))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(from_hex("00aF"), Some(vec![0, 0xAF]));
        assert_eq!(from_hex("0"), None);
        assert_eq!(from_hex("0g"), None);
        assert_eq!(to_hex(&[0, 0xAF]), "00AF");
        assert!(is_pdu_line("07911326"));
        assert!(!is_pdu_line("079"));
        assert!(!is_pdu_line("OK"));
    }
}

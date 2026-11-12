//! SMS-SUBMIT: GSM 7-bit while every character fits it, UCS-2 otherwise; a long text split into concatenated parts.

use f50_core::{Error, Result};

use super::{Coding, gsm7, to_hex};

/// The most parts one message is sent in (10 x 153 GSM characters = 1530, the API's limit).
pub const MAX_PARTS: usize = 10;

/// One part, ready for `AT+CMGS=<tpdu_len>` and its prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submit {
    /// The TPDU's length in octets: what `AT+CMGS` wants (the leading SMSC octet does not count).
    pub tpdu_len: usize,
    /// "00" (no SMSC in the PDU: the modem's own is used) and the TPDU, in hexadecimal.
    pub pdu: String,
}

const GSM_SINGLE: usize = 160;
const GSM_PART: usize = 153;
const UCS2_SINGLE: usize = 70;
const UCS2_PART: usize = 67;

/// The PDUs for `text` to `number` (`+` and digits). `reference` numbers the parts of a long message (an 8-bit
/// concatenation element). `bad-value` for a number without digits, an empty text, or one over [`MAX_PARTS`].
pub fn encode_submit(number: &str, text: &str, reference: u8) -> Result<(Coding, Vec<Submit>)> {
    let (intl, digits) = match number.strip_prefix('+') {
        Some(d) => (true, d),
        None => (false, number),
    };
    if digits.is_empty() || digits.len() > 20 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::bad_value("number"));
    }
    if text.is_empty() {
        return Err(Error::bad_value("text"));
    }
    let gsm: Option<Vec<Vec<u8>>> = text.chars().map(gsm7::septets_of).collect();
    let (coding, chunks) = match gsm {
        Some(units) => (Coding::Gsm, split(&units, |u| u.len(), GSM_SINGLE, GSM_PART)),
        None => {
            let units: Vec<Vec<u16>> = text.chars().map(|c| c.encode_utf16(&mut [0; 2]).to_vec()).collect();
            let chunks = split(&units, |u| u.len(), UCS2_SINGLE, UCS2_PART);
            (Coding::Ucs2, chunks.into_iter().map(|c| c.into_iter().flat_map(|u| u.to_be_bytes()).collect()).collect())
        }
    };
    if chunks.len() > MAX_PARTS {
        return Err(Error::field(
            f50_core::Code::BadValue,
            "text",
            format!("the text needs {} messages; at most {MAX_PARTS} are sent", chunks.len()),
        ));
    }
    let total = chunks.len();
    let mut out = Vec::with_capacity(total);
    for (i, chunk) in chunks.iter().enumerate() {
        let udh = (total > 1).then(|| [5u8, 0, 3, reference, total as u8, (i + 1) as u8]);
        out.push(submit(intl, digits, coding, udh.as_ref().map(|h| &h[..]), chunk));
    }
    Ok((coding, out))
}

/// Splits whole characters (each a group of septets or UTF-16 units) into chunks: one when the whole fits `single`,
/// else chunks of at most `part`, never cutting a character (an escape sequence, a surrogate pair).
fn split<T: Clone>(units: &[Vec<T>], len: impl Fn(&Vec<T>) -> usize, single: usize, part: usize) -> Vec<Vec<T>> {
    let total: usize = units.iter().map(&len).sum();
    let limit = if total <= single { single } else { part };
    let mut chunks = vec![Vec::new()];
    let mut used = 0;
    for u in units {
        let n = len(u);
        if used + n > limit {
            chunks.push(Vec::new());
            used = 0;
        }
        used += n;
        if let Some(last) = chunks.last_mut() {
            last.extend(u.iter().cloned());
        }
    }
    chunks
}

/// One SMS-SUBMIT: `chunk` is septets (GSM) or UCS-2 octets.
fn submit(intl: bool, digits: &str, coding: Coding, udh: Option<&[u8]>, chunk: &[u8]) -> Submit {
    // SMS-SUBMIT with a relative validity period (0x11), a user data header when there is one (0x40)
    let mut t = vec![if udh.is_some() { 0x51 } else { 0x11 }, 0x00];
    // the destination: its digit count, its type (international or unknown/ISDN), the semi-octets
    t.push(digits.len() as u8);
    t.push(if intl { 0x91 } else { 0x81 });
    for pair in digits.as_bytes().chunks(2) {
        let lo = pair[0] - b'0';
        let hi = pair.get(1).map_or(0x0f, |d| d - b'0');
        t.push(hi << 4 | lo);
    }
    // PID 0, DCS, validity 0xAA (four days)
    t.push(0x00);
    t.push(if coding == Coding::Ucs2 { 0x08 } else { 0x00 });
    t.push(0xAA);
    let hdr = udh.unwrap_or_default();
    match coding {
        Coding::Ucs2 | Coding::Eight => {
            t.push((hdr.len() + chunk.len()) as u8);
            t.extend_from_slice(hdr);
            t.extend_from_slice(chunk);
        }
        Coding::Gsm => {
            // the text starts on the first septet boundary after the header
            let skip = (hdr.len() * 8).div_ceil(7);
            let fill = skip * 7 - hdr.len() * 8;
            t.push((skip + chunk.len()) as u8);
            t.extend_from_slice(hdr);
            t.extend(gsm7::pack(chunk, fill));
        }
    }
    Submit { tpdu_len: t.len(), pdu: format!("00{}", to_hex(&t)) }
}

#[cfg(test)]
mod tests {
    use super::super::{Decoded, decode};
    use super::*;

    fn one(number: &str, text: &str) -> (usize, String) {
        let (_, parts) = encode_submit(number, text, 0).expect("encodes");
        assert_eq!(parts.len(), 1);
        (parts[0].tpdu_len, parts[0].pdu.clone())
    }

    #[test]
    fn single_parts_match_sms_pdu_awk() {
        // the exact output of `awk -v mode=encode -f sms-pdu.awk` for these
        assert_eq!(
            one("+31641600986", "How are you?"),
            (25, "0011000B911346610089F60000AA0CC8F71D14969741F977FD07".into())
        );
        assert_eq!(one("+31641600986", "hello"), (19, "0011000B911346610089F60000AA05E8329BFD06".into()));
        assert_eq!(
            one("+31641600986", "satır ğüş"),
            (32, "0011000B911346610089F60008AA12007300610074013100720020011F00FC015F".into())
        );
        assert_eq!(
            one("+31641600986", "a{b}€[x]"),
            (26, "0011000B911346610089F60000AA0DE10D4ABC496DCA1B1E7EE303".into())
        );
        assert_eq!(one("10086", "1234567"), (18, "00110005810180F60000AA0731D98C56B3DD00".into()));
    }

    #[test]
    fn limits_of_one_part() {
        assert_eq!(encode_submit("1", &"a".repeat(160), 0).expect("gsm").1.len(), 1);
        assert_eq!(encode_submit("1", &"a".repeat(161), 0).expect("gsm").1.len(), 2);
        // an extension character counts twice
        assert_eq!(encode_submit("1", &"€".repeat(80), 0).expect("gsm").1.len(), 1);
        assert_eq!(encode_submit("1", &"ı".repeat(70), 0).expect("ucs2").1.len(), 1);
        assert_eq!(encode_submit("1", &"ı".repeat(71), 0).expect("ucs2").1.len(), 2);
    }

    #[test]
    fn long_texts_are_concatenated_and_decode_back() {
        let text: String = (0..400).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let (coding, parts) = encode_submit("+905551112233", &text, 0x42).expect("encodes");
        assert_eq!((coding, parts.len()), (Coding::Gsm, 3));
        let mut back = String::new();
        for (i, p) in parts.iter().enumerate() {
            assert_eq!(p.tpdu_len * 2 + 2, p.pdu.len());
            // the SUBMIT turned into a DELIVER around the same PID, DCS and user data: first, MR, length, type,
            // six octets of digits, PID, DCS, then the validity period (skipped) and UDL + UD
            let tpdu = &p.pdu[2..];
            assert_eq!(&tpdu[..2], "51");
            let deliver = format!("00400C91{}{}20806291731448{}", &tpdu[8..20], &tpdu[20..24], &tpdu[26..]);
            match decode(&deliver) {
                Some(Decoded::Deliver(d)) => {
                    let c = d.concat.expect("a part");
                    assert_eq!((c.reference, c.total, c.part), (0x42, 3, (i + 1) as u8));
                    back.push_str(&d.text);
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(back, text);
        // an escape sequence is never cut between two parts
        let text = format!("{}€", "a".repeat(152));
        let (_, parts) = encode_submit("1", &format!("{text}{}", "b".repeat(10)), 1).expect("encodes");
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn refusals() {
        assert_eq!(encode_submit("+", "x", 0).map(|_| ()).expect_err("no digits").field_name(), Some("number"));
        assert_eq!(encode_submit("12a", "x", 0).map(|_| ()).expect_err("letter").field_name(), Some("number"));
        assert_eq!(encode_submit("1", "", 0).map(|_| ()).expect_err("empty").field_name(), Some("text"));
        assert!(encode_submit("1", &"a".repeat(1530), 0).is_ok());
        assert_eq!(
            encode_submit("1", &"a".repeat(1531), 0).map(|_| ()).expect_err("11 parts").field_name(),
            Some("text")
        );
        assert!(encode_submit("1", &"ı".repeat(670), 0).is_ok());
        assert!(encode_submit("1", &"ı".repeat(671), 0).is_err());
    }
}

//! SMS-DELIVER and SMS-STATUS-REPORT, as the modem gives them (`+CMGL`, `+CMGR`, `+CMT`, `+CDS`): the SMSC
//! address first, then the TPDU.

use super::{Coding, Concat, Timestamp, from_hex, gsm7};

/// A received message (one part of it, for a long one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliver {
    /// The service centre that delivered it, empty when the PDU names none.
    pub smsc: String,
    /// "+31641600986", "10086", or a name ("ADANA BLD") for an alphanumeric sender.
    pub sender: String,
    pub timestamp: Timestamp,
    pub pid: u8,
    pub dcs: u8,
    pub coding: Coding,
    /// Set for a part of a long message whose header says something usable.
    pub concat: Option<Concat>,
    pub text: String,
}

/// The network's report on a message sent with a status report requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// The TP-MR of the submission (what `+CMGS:` answered).
    pub reference: u8,
    pub recipient: String,
    /// When the centre took the message.
    pub sent: Timestamp,
    /// When it was delivered, or the delivery given up.
    pub done: Timestamp,
    /// TP-ST: 0x00-0x1F delivered, 0x20-0x3F still trying, 0x40 and up failed.
    pub status: u8,
}

impl StatusReport {
    pub fn delivered(&self) -> bool {
        self.status < 0x20
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Deliver(Deliver),
    StatusReport(StatusReport),
}

/// Decodes a PDU with its SMSC prefix. `None` for anything that is not a well-formed SMS-DELIVER or
/// SMS-STATUS-REPORT (an SMS-SUBMIT stored on the SIM, garbage).
pub fn decode(hex: &str) -> Option<Decoded> {
    let bytes = from_hex(hex.trim())?;
    let mut c = Cur { b: &bytes, i: 0 };
    let smsc_len = usize::from(c.u8()?);
    let smsc = if smsc_len == 0 { String::new() } else { smsc_address(c.take(smsc_len)?) };
    let first = c.u8()?;
    match first & 0x03 {
        0 => deliver(&mut c, first, smsc).map(Decoded::Deliver),
        2 => status_report(&mut c).map(Decoded::StatusReport),
        _ => None,
    }
}

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }
    fn rest(&self) -> &'a [u8] {
        self.b.get(self.i..).unwrap_or_default()
    }
}

/// Semi-octets: the low nibble is the first digit; `F` fills an odd count.
fn bcd(bytes: &[u8], digits: usize) -> String {
    const D: &[u8; 16] = b"0123456789*#abc?";
    let mut out = String::with_capacity(digits);
    for i in 0..digits {
        let Some(&b) = bytes.get(i / 2) else { break };
        let n = if i % 2 == 0 { b & 0x0f } else { b >> 4 };
        if n == 0x0f {
            break;
        }
        out.push(char::from(D[usize::from(n)]));
    }
    out
}

fn smsc_address(b: &[u8]) -> String {
    let Some((&toa, digits)) = b.split_first() else { return String::new() };
    let plus = if (toa >> 4) & 7 == 1 { "+" } else { "" };
    format!("{plus}{}", bcd(digits, digits.len() * 2))
}

/// An originating or recipient address: its length in digits (semi-octets), its type, the digits.
fn address(c: &mut Cur<'_>) -> Option<String> {
    let digits = usize::from(c.u8()?);
    let toa = c.u8()?;
    let bytes = c.take(digits.div_ceil(2))?;
    Some(match (toa >> 4) & 7 {
        // alphanumeric: GSM 7-bit packed into the semi-octets (a brand name, FINDINGS 26c)
        5 => gsm7::to_text(&gsm7::unpack(bytes, digits * 4 / 7), 0, 0),
        1 => format!("+{}", bcd(bytes, digits)),
        _ => bcd(bytes, digits),
    })
}

/// Seven octets of semi-octet digits: year, month, day, hour, minute, second, and the zone in quarter hours (its
/// sign in bit 3).
fn timestamp(b: &[u8]) -> Timestamp {
    let two = |v: u8| -> Option<u8> {
        let (lo, hi) = (v & 0x0f, v >> 4);
        (lo <= 9 && hi <= 9).then_some(lo * 10 + hi)
    };
    let f: Option<Vec<u8>> = b.iter().take(6).map(|&v| two(v)).collect();
    let local = match f.as_deref() {
        Some([y, mo, d, h, mi, s]) => format!("20{y:02}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"),
        _ => String::new(),
    };
    let offset_min = b.get(6).and_then(|&z| {
        let (lo, hi) = (z & 0x0f, z >> 4);
        if hi > 9 {
            return None;
        }
        let q = i16::from(lo & 0x07) * 10 + i16::from(hi);
        Some(if lo & 0x08 != 0 { -q * 15 } else { q * 15 })
    });
    Timestamp { local, offset_min }
}

/// The alphabet from the data coding scheme (TS 23.038 4).
fn coding_of(dcs: u8) -> Coding {
    match dcs >> 4 {
        0x0c | 0x0d => Coding::Gsm,
        0x0e => Coding::Ucs2,
        0x0f if dcs & 0x04 != 0 => Coding::Eight,
        0x0f => Coding::Gsm,
        _ => match (dcs >> 2) & 0x03 {
            1 => Coding::Eight,
            2 => Coding::Ucs2,
            _ => Coding::Gsm,
        },
    }
}

/// What the user data header says: the concatenation element, and the national language tables.
#[derive(Debug, Default, PartialEq, Eq)]
struct Header {
    concat: Option<Concat>,
    lock: u8,
    shift: u8,
}

/// Walks every element: the concatenation one is not always first, and the language one never is (FINDINGS 26c).
fn header(h: &[u8]) -> Header {
    let mut out = Header::default();
    let mut j = 0;
    while j + 1 < h.len() {
        let (iei, len) = (h[j], usize::from(h[j + 1]));
        // an element cut short by the header's end counts for nothing: it would take its values from the text
        let Some(v) = h.get(j + 2..j + 2 + len) else { break };
        match (iei, v) {
            (0x00, &[r, t, p]) => out.concat = Some(Concat { reference: u16::from(r), total: t, part: p }),
            (0x08, &[r1, r2, t, p]) => {
                out.concat = Some(Concat { reference: u16::from_be_bytes([r1, r2]), total: t, part: p });
            }
            (0x24, &[l, ..]) => out.shift = l,
            (0x25, &[l, ..]) => out.lock = l,
            _ => {}
        }
        j += 2 + len;
    }
    // part 0, part 5 of 3, no parts: nothing could ever make it whole, so it is a message of its own (FINDINGS 63)
    if out.concat.is_some_and(|c| c.total < 1 || c.part < 1 || c.part > c.total) {
        out.concat = None;
    }
    out
}

/// The text of the user data; `udl` in septets (GSM) or octets (the others), header included.
fn user_data(coding: Coding, udl: usize, ud: &[u8], udhi: bool) -> Option<(Header, String)> {
    let (hdr, hlen) = if udhi {
        let n = usize::from(*ud.first()?);
        (header(ud.get(1..1 + n)?), n + 1)
    } else {
        (Header::default(), 0)
    };
    let text = match coding {
        Coding::Gsm => {
            let skip = (hlen * 8).div_ceil(7);
            let septets = gsm7::unpack(ud, udl);
            gsm7::to_text(septets.get(skip..).unwrap_or_default(), hdr.lock, hdr.shift)
        }
        Coding::Ucs2 => {
            let body = ud.get(hlen..udl.min(ud.len())).unwrap_or_default();
            let units: Vec<u16> = body.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p)).collect();
            String::from_utf16_lossy(&units)
        }
        Coding::Eight => ud.get(hlen..udl.min(ud.len())).unwrap_or_default().iter().map(|&b| char::from(b)).collect(),
    };
    Some((hdr, text))
}

fn deliver(c: &mut Cur<'_>, first: u8, smsc: String) -> Option<Deliver> {
    let sender = address(c)?;
    let pid = c.u8()?;
    let dcs = c.u8()?;
    let timestamp = timestamp(c.take(7)?);
    let udl = usize::from(c.u8()?);
    let coding = coding_of(dcs);
    let (hdr, text) = user_data(coding, udl, c.rest(), first & 0x40 != 0)?;
    Some(Deliver { smsc, sender, timestamp, pid, dcs, coding, concat: hdr.concat, text })
}

fn status_report(c: &mut Cur<'_>) -> Option<StatusReport> {
    let reference = c.u8()?;
    let recipient = address(c)?;
    let sent = timestamp(c.take(7)?);
    let done = timestamp(c.take(7)?);
    let status = c.u8()?;
    Some(StatusReport { reference, recipient, sent, done, status })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The textbook SMS-DELIVER (tests/test_sms_pool.py PDU_GSM, checked against sms-pdu.awk).
    const PDU_GSM: &str = "07911326040000F0040B911346610089F60000208062917314480CC8F71D14969741F977FD07";
    /// UCS-2 from an alphanumeric sender (PDU_UCS2 there: the case text mode mangles, FINDINGS 26c).
    const PDU_UCS2: &str = "000410D04162D0190409994400086290517181004024004B006100720067006F006E0075007A00200079006F006C006400610020011F00FC015F";

    fn dl(hex: &str) -> Deliver {
        match decode(hex) {
            Some(Decoded::Deliver(d)) => d,
            other => panic!("not a deliver: {other:?}"),
        }
    }

    #[test]
    fn gsm_textbook() {
        let d = dl(PDU_GSM);
        assert_eq!(d.smsc, "+31624000000");
        assert_eq!(d.sender, "+31641600986");
        assert_eq!(d.timestamp, Timestamp { local: "2002-08-26 19:37:41".into(), offset_min: Some(-60) });
        assert_eq!((d.coding, d.concat), (Coding::Gsm, None));
        assert_eq!(d.text, "How are you?");
    }

    #[test]
    fn ucs2_from_a_name() {
        let d = dl(PDU_UCS2);
        assert_eq!(d.smsc, "");
        assert_eq!(d.sender, "ADANA BLD");
        assert_eq!(d.timestamp, Timestamp { local: "2026-09-15 17:18:00".into(), offset_min: Some(60) });
        assert_eq!(d.coding, Coding::Ucs2);
        assert_eq!(d.text, "Kargonuz yolda ğüş");
    }

    #[test]
    fn concatenated_parts() {
        let d = dl(&super::super::test_part(0x42, 2, 1, "hello", "2026-10-12 08:00:00"));
        assert_eq!(d.concat, Some(Concat { reference: 0x42, total: 2, part: 1 }));
        assert_eq!(d.text, "hello");
        assert_eq!(d.timestamp.local, "2026-10-12 08:00:00");
        // nothing usable: a message of its own
        for (t, n) in [(0, 0), (3, 5), (0, 1)] {
            assert_eq!(dl(&super::super::test_part(1, t, n, "x", "2026-10-12 08:00:00")).concat, None);
        }
    }

    #[test]
    fn sixteen_bit_reference_and_gsm_behind_a_header() {
        // UDHI, GSM 7-bit: header 06 08 04 1234 02 01 (7 octets = 56 bits, 8 septets), then "hi"
        let mut ud = vec![6u8, 8, 4, 0x12, 0x34, 2, 1];
        let text = [0x68u8, 0x69];
        ud.extend(gsm7::pack(&text, 0));
        let pdu = format!("00400B911346610089F6000020806291731448{:02X}{}", 8 + 2, super::super::to_hex(&ud));
        let d = dl(&pdu);
        assert_eq!(d.concat, Some(Concat { reference: 0x1234, total: 2, part: 1 }));
        assert_eq!(d.text, "hi");
    }

    #[test]
    fn turkish_locking_shift() {
        // header 03 25 01 01 (4 octets, 32 bits: 5 septets, 3 fill bits), then "ı" (0x07 in the Turkish table)
        let mut ud = vec![3u8, 0x25, 1, 1];
        ud.extend(gsm7::pack(&[0x07], 3));
        let pdu = format!("00400B911346610089F600002080629173144806{}", super::super::to_hex(&ud));
        assert_eq!(dl(&pdu).text, "ı");
    }

    #[test]
    fn status_report() {
        let pdu = "00062A0B911346610089F620806291731448208062917315480000";
        match decode(pdu) {
            Some(Decoded::StatusReport(r)) => {
                assert_eq!((r.reference, r.recipient.as_str(), r.status), (0x2A, "+31641600986", 0));
                assert_eq!(r.done.local, "2002-08-26 19:37:51");
                assert!(r.delivered());
            }
            other => panic!("{other:?}"),
        }
        // the trailing 00 above is a parameter indicator; a report cut before its status is nothing
        assert_eq!(decode("00062A0B911346610089F62080629173144820806291731548"), None);
    }

    #[test]
    fn refuses_what_is_not_a_message() {
        // an SMS-SUBMIT (as stored on the SIM), odd length, cut short, empty
        assert_eq!(decode("0011000B911346610089F60000AA0CC8F71D14969741F977FD07"), None);
        assert_eq!(decode("079"), None);
        assert_eq!(decode("07911326040000F0040B91134661"), None);
        assert_eq!(decode(""), None);
    }
}

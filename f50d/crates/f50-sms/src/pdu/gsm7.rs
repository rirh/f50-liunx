//! The GSM 7-bit default alphabet (3GPP TS 23.038 6.2.1), its extension table, the Turkish national language
//! tables, and septet packing.
//!
//! Turkish arrives as ordinary GSM 7-bit with a header element saying "read it against this table instead"
//! (`0x25` locking shift, `0x24` single shift, value 1). Without it the text has the right shape and the wrong
//! letters: "hastası" as "hastasì" (docs/research/FINDINGS.md 26c). Eight positions move, exactly the Turkish ones.

/// The escape septet: the next septet is read in the extension (single shift) table.
pub(crate) const ESC: u8 = 27;

/// The default alphabet, septet value to code point (from sms-pdu.awk, which has been read against the device).
const DEFAULT: [u16; 128] = [
    64, 163, 36, 165, 232, 233, 249, 236, 242, 199, 10, 216, 248, 13, 197, 229, //
    916, 95, 934, 915, 923, 937, 928, 936, 931, 920, 926, 27, 198, 230, 223, 201, //
    32, 33, 34, 35, 164, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, //
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, //
    161, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, //
    80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 196, 214, 209, 220, 167, //
    191, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, //
    112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 228, 246, 241, 252, 224,
];

/// The default extension table: the septet after ESC, and its code point.
const EXTENSION: [(u8, u16); 10] =
    [(10, 12), (20, 94), (40, 123), (41, 125), (47, 92), (60, 91), (61, 126), (62, 93), (64, 124), (101, 8364)];

/// The Turkish locking shift table: the positions that differ from the default alphabet (TS 23.038 A.3.1).
const TURKISH_LOCK: [(u8, u16); 8] =
    [(4, 8364), (7, 305), (11, 286), (12, 287), (28, 350), (29, 351), (64, 304), (96, 231)];

/// The Turkish single shift table (reached through ESC).
const TURKISH_SHIFT: [(u8, u16); 16] = [
    (20, 94),
    (40, 123),
    (41, 125),
    (47, 92),
    (60, 91),
    (61, 126),
    (62, 93),
    (64, 124),
    (101, 8364),
    (71, 286),
    (73, 304),
    (83, 350),
    (99, 231),
    (103, 287),
    (105, 305),
    (115, 351),
];

/// The national language value for Turkish in the shift header elements.
const TURKISH: u8 = 1;

fn lookup(table: &[(u8, u16)], septet: u8) -> Option<u16> {
    table.iter().find(|(s, _)| *s == septet).map(|(_, c)| *c)
}

fn to_char(cp: u16) -> char {
    char::from_u32(u32::from(cp)).unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// The text of `septets`, in the tables the header asked for (`lock`, `shift`: the national language values of the
/// locking and single shift elements, 0 for none).
pub(crate) fn to_text(septets: &[u8], lock: u8, shift: u8) -> String {
    let mut out = String::with_capacity(septets.len());
    let mut esc = false;
    for &s in septets {
        let s = s & 0x7f;
        if esc {
            esc = false;
            let cp = if shift == TURKISH { lookup(&TURKISH_SHIFT, s) } else { None }
                .or_else(|| lookup(&EXTENSION, s))
                .unwrap_or(DEFAULT[usize::from(s)]);
            out.push(to_char(cp));
        } else if s == ESC {
            esc = true;
        } else {
            let cp = if lock == TURKISH { lookup(&TURKISH_LOCK, s) } else { None }.unwrap_or(DEFAULT[usize::from(s)]);
            out.push(to_char(cp));
        }
    }
    out
}

/// The septets for one character in the default alphabet and its extension table: one, or ESC and one. `None` when
/// the character is in neither (the message then goes as UCS-2). ESC itself is never a character of a text.
pub(crate) fn septets_of(c: char) -> Option<Vec<u8>> {
    let cp = u16::try_from(u32::from(c)).ok()?;
    if cp == u16::from(ESC) {
        return None;
    }
    if let Some(i) = DEFAULT.iter().position(|&d| d == cp) {
        return u8::try_from(i).ok().map(|s| vec![s]);
    }
    EXTENSION.iter().find(|(_, d)| *d == cp).map(|(s, _)| vec![ESC, *s])
}

/// `count` septets unpacked from `data`, the first starting at bit 0 of byte 0. Stops early when `data` ends.
pub(crate) fn unpack(data: &[u8], count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bit = i * 7;
        let byte = bit / 8;
        let shift = bit % 8;
        let Some(&lo) = data.get(byte) else { break };
        let mut v = u16::from(lo) >> shift;
        if shift > 1 {
            // the septet runs into the next octet; a last octet that is missing reads as zero bits
            v |= u16::from(data.get(byte + 1).copied().unwrap_or(0)) << (8 - shift);
        }
        out.push((v & 0x7f) as u8);
    }
    out
}

/// `septets` packed into octets after `fill` padding bits (0-6: the fill that aligns the text on a septet boundary
/// after a user data header).
pub(crate) fn pack(septets: &[u8], fill: usize) -> Vec<u8> {
    let total_bits = fill + septets.len() * 7;
    let mut out = vec![0u8; total_bits.div_ceil(8)];
    for (i, &s) in septets.iter().enumerate() {
        let bit = fill + i * 7;
        let (byte, shift) = (bit / 8, bit % 8);
        let v = u16::from(s & 0x7f) << shift;
        out[byte] |= (v & 0xff) as u8;
        if v > 0xff {
            out[byte + 1] |= (v >> 8) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_and_unpack_are_inverse() {
        let text: Vec<u8> = "hellohello".chars().flat_map(|c| septets_of(c).expect("gsm")).collect();
        let packed = pack(&text, 0);
        assert_eq!(packed, [0xE8, 0x32, 0x9B, 0xFD, 0x46, 0x97, 0xD9, 0xEC, 0x37]);
        assert_eq!(unpack(&packed, text.len()), text);
        // behind a six-octet header (48 bits) one fill bit puts the text on septet 7
        let mut ud = vec![5, 0, 3, 0x42, 2, 1];
        ud.extend(pack(&text, 1));
        let back = unpack(&ud, 7 + text.len());
        assert_eq!(&back[7..], &text[..]);
    }

    #[test]
    fn alphabet_and_extension() {
        assert_eq!(septets_of('@'), Some(vec![0]));
        assert_eq!(septets_of('€'), Some(vec![ESC, 101]));
        assert_eq!(septets_of('\n'), Some(vec![10]));
        assert_eq!(septets_of('ı'), None);
        assert_eq!(septets_of('\u{1b}'), None);
        assert_eq!(to_text(&[ESC, 40, 0x41, ESC, 101], 0, 0), "{A€");
    }

    #[test]
    fn turkish_tables() {
        // "hastası": s=0x73 a=0x61 ... ı is 0x07 in the Turkish locking table, ì in the default one
        let septets = [0x68, 0x61, 0x73, 0x74, 0x61, 0x73, 0x07];
        assert_eq!(to_text(&septets, 0, 0), "hastasì");
        assert_eq!(to_text(&septets, 1, 0), "hastası");
        assert_eq!(to_text(&[ESC, 71], 0, 1), "Ğ");
    }
}

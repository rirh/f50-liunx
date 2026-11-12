//! Network names as they may be shown.
//!
//! `iw` and wpa_supplicant print every byte of a name outside printable ASCII as `\xNN` (the supplicant also `\\`,
//! `\"`, `\e`, `\n`, `\t`, `\r`), so a UTF-8 name comes as `\xc3\x87...`: [`decode`] gives the bytes back. A name is
//! whatever an access point broadcasts, so what reaches the page is printable text only ([`shown`]): well-formed
//! UTF-8 of visible characters. Every other byte is shown as `\xNN` - C0 controls, DEL, the C1 controls, bytes that
//! are not UTF-8 or are overlong or surrogate encodings, the backslash itself - and so are the invisible characters
//! that make one name look like another: zero-width ones, bidirectional controls, line separators, fillers,
//! variation selectors, tags, spaces other than the ASCII one, noncharacters and private use, and a space at either
//! end. Ported from `wifi-client`'s `unescape`.

/// The bytes of a name as `iw`/the supplicant escape it.
pub(crate) fn decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == b'\\' && next == Some(b'x') {
            let hex = b.get(i + 2..i + 4).and_then(|h| std::str::from_utf8(h).ok());
            if let Some(v) =
                hex.filter(|h| h.bytes().all(|x| x.is_ascii_hexdigit())).and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 4;
                continue;
            }
        }
        if c == b'\\' {
            let esc = match next {
                Some(b'\\') => Some(b'\\'),
                Some(b'"') => Some(b'"'),
                Some(b'e') => Some(27),
                Some(b'n') => Some(b'\n'),
                Some(b't') => Some(b'\t'),
                Some(b'r') => Some(b'\r'),
                _ => None,
            };
            if let Some(e) = esc {
                out.push(e);
                i += 2;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Format, filler, variation-selector, blank and spacing characters, noncharacters, private use: whatever shows as
/// nothing (or as an ordinary space) and so can make one name look like another.
fn invisible(c: u32) -> bool {
    matches!(c,
        160 | 173 | 5760 | 847 | 1564 | 1536..=1541 | 1757 | 1807 | 2274 | 4447 | 4448 | 6068 | 6069
        | 6155..=6159 | 8192..=8207 | 8232..=8239 | 8287..=8303 | 10240 | 12288 | 12644 | 57344..=63743
        | 64976..=65007 | 65024..=65039 | 65279 | 65440 | 65520..=65531 | 113824..=113827 | 119155..=119162
        | 917504..=921599)
        || c >= 983040
        || (c % 65536) >= 65534
}

/// The name as it may be shown.
pub(crate) fn shown(b: &[u8]) -> String {
    let x = |v: u8| format!("\\x{v:02x}");
    let n = b.len();
    let mut o = String::new();
    let mut i = 0;
    while i < n {
        let v = b[i];
        if v == b' ' && (i == 0 || i == n - 1) {
            o.push_str(&x(v));
            i += 1;
            continue;
        }
        if (32..127).contains(&v) && v != b'\\' {
            o.push(char::from(v));
            i += 1;
            continue;
        }
        if !(194..=244).contains(&v) {
            o.push_str(&x(v));
            i += 1;
            continue;
        }
        let m = if v < 224 {
            1
        } else if v < 240 {
            2
        } else {
            3
        };
        let (lo, hi) = match v {
            194 | 224 => (160, 191), // U+0080-U+009F (C1 controls); overlong
            237 => (128, 159),       // surrogates
            240 => (144, 191),       // overlong
            244 => (128, 143),       // above U+10FFFF
            _ => (128, 191),
        };
        let mut ok = i + m < n && (lo..=hi).contains(&b[i + 1]);
        for j in 2..=m {
            ok = ok && (128..=191).contains(&b[i + j]);
        }
        if ok {
            let s = std::str::from_utf8(&b[i..=i + m]).ok();
            let c = s.and_then(|s| s.chars().next());
            ok = c.is_some_and(|c| !invisible(u32::from(c)));
            if let (true, Some(c)) = (ok, c) {
                o.push(c);
                i += m + 1;
                continue;
            }
        }
        o.push_str(&x(v));
        i += 1;
    }
    o
}

/// An escaped name, decoded and shown.
pub(crate) fn shown_escaped(s: &str) -> String {
    shown(&decode(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escapes() {
        assert_eq!(decode(r"\xc3\x87ay"), "Çay".as_bytes());
        assert_eq!(decode(r#"a\\b\"c\e"#), b"a\\b\"c\x1b");
        assert_eq!(decode(r"\xZZ"), b"\\xZZ");
    }

    #[test]
    fn shows_only_visible_text() {
        assert_eq!(shown("Çay Evi".as_bytes()), "Çay Evi");
        assert_eq!(shown(b"Home "), "Home\\x20");
        assert_eq!(shown(b" Home"), "\\x20Home");
        assert_eq!(shown(b"a\x1bb"), "a\\x1bb");
        assert_eq!(shown(b"a\\b"), "a\\x5cb");
        assert_eq!(shown("a\u{200b}b".as_bytes()), "a\\xe2\\x80\\x8bb");
        assert_eq!(shown("a\u{85}b".as_bytes()), "a\\xc2\\x85b");
        assert_eq!(shown(b"\xc3"), "\\xc3");
        assert_eq!(shown(b"\xed\xa0\x80"), "\\xed\\xa0\\x80");
        assert_eq!(shown_escaped(r"\xe2\x80\xae"), "\\xe2\\x80\\xae");
        assert_eq!(shown("日本".as_bytes()), "日本");
    }
}

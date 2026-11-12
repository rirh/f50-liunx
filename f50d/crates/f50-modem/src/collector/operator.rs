//! The operator: `+COPS?` in whichever format the modem was left in, its long name asked once per PLMN, and a
//! name in UCS2 hex decoded.
//!
//! `<format>` 2 is the numeric PLMN, 0 and 1 a name; the format field decides what the quoted text is, never its
//! look (a name may be all digits). The F50's modem keeps format 2, as Android's RIL leaves it, so the long name is
//! asked with one line that puts the numeric format back (`AT+COPS=3,0` then `=3,2`), once per PLMN. A name is read
//! as UCS2 hex when the character set is UCS2; under any other set only a text that cannot reasonably be a name
//! (issue #174: character set "HEX" gives "4E2D56FD8054901A"): two code units or more, not all decimal digits, no
//! private-use or non-character code point. Decoding happens on every round; nothing decoded is cached.

use serde_json::{Value, json};

/// What the collector keeps between rounds: the long-name answer per PLMN and the character set.
#[derive(Debug, Default, Clone)]
pub struct OperatorCache {
    /// (PLMN, character set, the named `+COPS:` line)
    pub named: Option<(String, String, String)>,
    pub cscs: Option<String>,
}

/// One `+COPS:` line -> (format, the quoted text, the access technology).
fn parse(line: &str) -> Option<(String, String, String)> {
    let rest = line.strip_prefix("+COPS:")?;
    let q = rest.find('"')?;
    let head = &rest[..q];
    // ` *[0-9]+ *, *[0-9]+ *, *` before the quote
    let parts: Vec<&str> = head.split(',').collect();
    if parts.len() != 3
        || !parts[2].trim().is_empty()
        || ![parts[0], parts[1]].iter().all(|p| {
            let p = p.trim_matches(' ');
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return None;
    }
    let after = &rest[q + 1..];
    let end = after.find('"')?;
    let text = after[..end].to_owned();
    let tail = after[end + 1..].trim_start_matches(' ');
    let tail = tail.strip_prefix(',').unwrap_or(tail).trim_start_matches(' ');
    let act: String = tail.chars().take_while(char::is_ascii_digit).collect();
    let format: String = parts[1].chars().filter(char::is_ascii_digit).collect();
    Some((format, text, act))
}

/// The UCS2 hex of a name, decoded; `None` when it is not one (`strict`: the character set did not say UCS2).
pub fn ucs2(s: &str, strict: bool) -> Option<String> {
    if s.is_empty() || !s.len().is_multiple_of(4) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if strict && (s.len() < 8 || s.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let units: Vec<u16> = (0..s.len()).step_by(4).filter_map(|i| u16::from_str_radix(&s[i..i + 4], 16).ok()).collect();
    let mut high = false;
    for &v in &units {
        if v < 32 || (127..160).contains(&v) {
            return None;
        }
        if strict && ((0xE000..=0xF8FF).contains(&v) || (0xFDD0..=0xFDEF).contains(&v) || v >= 0xFFFE) {
            return None;
        }
        match v {
            0xD800..=0xDBFF if high => return None,
            0xD800..=0xDBFF => high = true,
            0xDC00..=0xDFFF if !high => return None,
            0xDC00..=0xDFFF => high = false,
            _ if high => return None,
            _ => {}
        }
    }
    if high {
        return None;
    }
    char::decode_utf16(units).collect::<Result<String, _>>().ok()
}

fn name_json(t: &str, cscs: Option<&str>) -> Value {
    if t.is_empty() {
        return Value::Null;
    }
    let strict = !cscs.is_some_and(|c| c.eq_ignore_ascii_case("UCS2"));
    json!(ucs2(t, strict).unwrap_or_else(|| t.to_owned()))
}

/// `{name, plmn, act}` from the `+COPS:` line, the long-name line (when the first is numeric) and the character set.
pub fn fragment(line: Option<&str>, named: Option<&str>, cscs: Option<&str>) -> Value {
    let Some((format, text, act)) = line.and_then(parse) else { return Value::Null };
    let (mut plmn, mut name) = (Value::Null, Value::Null);
    if format == "2" {
        if (5..=6).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()) {
            plmn = json!(text);
        }
        if let Some((f2, t2, _)) = named.and_then(parse)
            && (f2 == "0" || f2 == "1")
        {
            name = name_json(&t2, cscs);
        }
    } else {
        name = name_json(&text, cscs);
    }
    let act = act.parse::<u64>().map_or(Value::Null, |a| json!(a));
    json!({ "name": name, "plmn": plmn, "act": act })
}

/// The PLMN of a numeric-format line (`+COPS: 0,2,"46001",7`).
pub fn numeric_plmn(line: &str) -> Option<String> {
    let (format, text, _) = parse(line)?;
    (format == "2" && (5..=6).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())).then_some(text)
}

/// A name-format line whose text looks like UCS2 code units: the character set is needed to read it.
pub fn looks_hex_name(line: &str) -> bool {
    match parse(line) {
        Some((f, t, _)) if f == "0" || f == "1" => {
            !t.is_empty() && t.len().is_multiple_of(4) && t.bytes().all(|b| b.is_ascii_hexdigit())
        }
        _ => false,
    }
}

/// The character set of a `+CSCS:` value (`"UCS2"`).
pub fn cscs_value(v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix('"')?;
    let end = inner.find('"')?;
    let name = &inner[..end];
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then(|| name.to_owned())
}

/// A long-name line of a `+COPS?` answer (`+COPS: 0,0,"..."`).
pub fn is_named_line(line: &str) -> bool {
    matches!(parse(line), Some((f, _, _)) if f == "0" || f == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(line: &str, named: &str, cscs: Option<&str>) -> Value {
        fragment(Some(line), Some(named), cscs)["name"].clone()
    }

    #[test]
    fn the_reported_case() {
        let v = fragment(Some("+COPS: 0,2,\"46001\",7"), Some("+COPS: 0,0,\"4E2D56FD8054901A\",7"), Some("HEX"));
        assert_eq!(v, json!({"name":"中国联通","plmn":"46001","act":7}));
    }

    #[test]
    fn decoded_whatever_the_character_set_says() {
        let cases = [
            ("+COPS: 0,2,\"46000\",7", "+COPS: 0,0,\"4E2D56FD79FB52A8\",7", Some("GSM"), "中国移动"),
            ("+COPS: 0,2,\"46001\",7", "+COPS: 0,0,\"4E2D56FD8054901A\",7", None, "中国联通"),
            ("+COPS: 0,0,\"4E2D56FD75354FE1\",7", "", Some("IRA"), "中国电信"),
            ("+COPS: 0,0,\"4e2d56fd8054901a\",7", "", Some("HEX"), "中国联通"),
            ("+COPS: 0,0,\"0054002D004D006F00620069006C0065\",7", "", Some("IRA"), "T-Mobile"),
            ("+COPS: 0,0,\"D83DDCF10041\",7", "", Some("HEX"), "\u{1f4f1}A"),
            ("+COPS: 0,0,\"4E2D56FD8054901A\",7", "", Some("UCS2"), "中国联通"),
            ("+COPS: 0,0,\"00540054\",7", "", Some("UCS2"), "TT"),
        ];
        for (line, named, cscs, want) in cases {
            assert_eq!(name(line, named, cscs), json!(want), "{line}");
        }
    }

    #[test]
    fn an_ordinary_name_stays_what_it_is() {
        let names = [
            "vodafone TR",
            "China Unicom",
            "CAFE",
            "BEEF",
            "FACE",
            "12345678",
            "00540054",
            "4600146001460011",
            "DEADBEEF",
            "D83D0041",
            "000A0041",
            "0041007F",
            "E000E001",
            "F8FF0041",
            "FFFE0041",
            "0041FFFF",
            "FDD00041",
            "4E2D56FD80",
            "4E2D56FD805",
            "4E2D 56FD",
            "4E2D56FG",
        ];
        for n in names {
            for cscs in [Some("HEX"), Some("GSM"), None] {
                assert_eq!(name(&format!("+COPS: 0,0,\"{n}\",7"), "", cscs), json!(n), "{n} {cscs:?}");
            }
            let v = fragment(Some("+COPS: 0,2,\"28602\",13"), Some(&format!("+COPS: 0,0,\"{n}\",13")), None);
            assert_eq!((v["name"].clone(), v["plmn"].clone()), (json!(n), json!("28602")));
        }
    }

    #[test]
    fn no_name_leaves_the_plmn() {
        let v = fragment(Some("+COPS: 0,2,\"46001\",7"), Some("ERROR"), None);
        assert_eq!((v["name"].clone(), v["plmn"].clone()), (Value::Null, json!("46001")));
        assert_eq!(fragment(Some("+COPS: 0"), None, None), Value::Null);
        assert_eq!(numeric_plmn("+COPS: 0,2,\"46001\",7").as_deref(), Some("46001"));
        assert!(looks_hex_name("+COPS: 0,0,\"4E2D56FD\",7"));
        assert_eq!(cscs_value("\"UCS2\"").as_deref(), Some("UCS2"));
    }
}

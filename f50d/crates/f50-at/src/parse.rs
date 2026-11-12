//! Splitting AT parameter lists: quoted strings (which may hold commas), empty fields, numbers, hexadecimal, and the
//! parenthesised groups of test commands (`+COPS: (2,"Op","Op","46000",7),(...)`).
//!
//! The input is a reply's value, as [`crate::Reply::value`] gives it: `+CEREG: 2,1,"2F4B","0A1B2C3",13` yields
//! `2,1,"2F4B","0A1B2C3",13`.

/// One parameter of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item<'a> {
    /// The text, trimmed, without its quotes.
    pub text: &'a str,
    /// It was a quoted string.
    pub quoted: bool,
}

impl<'a> Item<'a> {
    /// The text, empty for an omitted parameter (`1,,3`).
    pub fn as_str(&self) -> &'a str {
        self.text
    }

    /// An omitted parameter (or `""`).
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// A decimal number, optionally signed.
    pub fn int(&self) -> Option<i64> {
        let t = self.text;
        let digits = t.strip_prefix(['-', '+']).unwrap_or(t);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse().ok()
    }

    /// A hexadecimal number, with or without `0x` (a TAC, a cell id: `"2F4B"`).
    pub fn hex(&self) -> Option<u64> {
        let t = self.text;
        let digits = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
        if digits.is_empty() || digits.len() > 16 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u64::from_str_radix(digits, 16).ok()
    }
}

/// The parameters of `s`, split at the commas outside quotes and parentheses. An empty `s` has no parameters; a
/// trailing comma makes an empty last one. An unterminated quote runs to the end.
pub fn split(s: &str) -> Vec<Item<'_>> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut depth = 0u32;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b'(' if !quoted => depth += 1,
            b')' if !quoted => depth = depth.saturating_sub(1),
            b',' if !quoted && depth == 0 => {
                out.push(item(&s[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(item(&s[start..]));
    out
}

fn item(raw: &str) -> Item<'_> {
    let t = raw.trim();
    match t.strip_prefix('"') {
        Some(rest) => Item { text: rest.strip_suffix('"').unwrap_or(rest), quoted: true },
        None => Item { text: t, quoted: false },
    }
}

/// The parenthesised groups of a test command's answer, without their parentheses: `(1,"a"),(2,"b")` yields
/// `1,"a"` and `2,"b"`. Text outside parentheses is skipped.
pub fn groups(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut depth = 0u32;
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b'(' if !quoted => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            b')' if !quoted && depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(&s[start..i]);
                }
            }
            _ => {}
        }
    }
    out
}

/// The `i`th parameter as text (`None` past the end).
pub fn str_at<'a>(items: &[Item<'a>], i: usize) -> Option<&'a str> {
    items.get(i).map(Item::as_str)
}

/// The `i`th parameter as a decimal number (`None` past the end, when omitted, or not a number).
pub fn int_at(items: &[Item<'_>], i: usize) -> Option<i64> {
    items.get(i).and_then(Item::int)
}

/// The `i`th parameter as a hexadecimal number.
pub fn hex_at(items: &[Item<'_>], i: usize) -> Option<u64> {
    items.get(i).and_then(Item::hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_quoted() {
        let p = split(r#"2,1,"2F4B","0A1B2C3",13"#);
        assert_eq!(p.len(), 5);
        assert_eq!(int_at(&p, 0), Some(2));
        assert_eq!(hex_at(&p, 2), Some(0x2F4B));
        assert_eq!(hex_at(&p, 3), Some(0x00A1_B2C3));
        assert!(p[2].quoted && !p[0].quoted);
        assert_eq!(int_at(&p, 4), Some(13));
        assert_eq!(int_at(&p, 5), None);
    }

    #[test]
    fn commas_in_quotes_and_empty_fields() {
        let p = split(r#" "a, b" ,,3, "#);
        assert_eq!(p.iter().map(Item::as_str).collect::<Vec<_>>(), vec!["a, b", "", "3", ""]);
        assert!(p[1].is_empty());
        assert_eq!(int_at(&p, 1), None);
        assert!(split("").is_empty());
        assert_eq!(split("\"\"")[0], Item { text: "", quoted: true });
        assert_eq!(split("\"open")[0].text, "open");
    }

    #[test]
    fn numbers() {
        assert_eq!(split("-5")[0].int(), Some(-5));
        assert_eq!(split("+7")[0].int(), Some(7));
        assert_eq!(split("1a")[0].int(), None);
        assert_eq!(split("-")[0].int(), None);
        assert_eq!(split("0x1F")[0].hex(), Some(31));
        assert_eq!(split("FFFFFFFFFFFFFFFFF")[0].hex(), None);
        assert_eq!(split("G1")[0].hex(), None);
    }

    #[test]
    fn groups_of_a_test_answer() {
        let v = r#"(2,"Op A","OpA","46000",7),(1,"B,x","B","46001",2),,(0-4),(0-2)"#;
        let g = groups(v);
        assert_eq!(g, vec![r#"2,"Op A","OpA","46000",7"#, r#"1,"B,x","B","46001",2"#, "0-4", "0-2"]);
        assert_eq!(str_at(&split(g[1]), 1), Some("B,x"));
        assert_eq!(split(v).len(), 5);
        assert_eq!(groups(r#"("(x",1)"#), vec![r#""(x",1"#]);
    }
}

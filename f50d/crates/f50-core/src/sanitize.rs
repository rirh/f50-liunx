//! Text that leaves the daemon: tool output, modem replies and error messages.

use std::sync::LazyLock;

use regex::Regex;

#[allow(clippy::expect_used)] // a constant pattern: a failure is a bug caught by the tests
static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"((?:https?|wss?|ftp)://)[^ "\n]*"#).expect("static regex"));

/// The most an error message may be, in bytes.
pub const MESSAGE_MAX: usize = 200;

/// `s` cut to at most `max` bytes, on a character boundary.
pub fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// The last `max` bytes of `s`, starting on a character boundary.
pub fn tail_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// Text for a reply: control characters other than newline and tab removed, carriage returns dropped, the path and
/// query of an address (a subscription's token) replaced by "...".
pub fn sanitize_text(s: &str) -> String {
    let clean: String = s.chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    URL.replace_all(&clean, "${1}...").into_owned()
}

/// One line for an error message: as `sanitize_text`, on one line, trimmed, at most `MESSAGE_MAX` bytes.
pub fn sanitize_message(s: &str) -> String {
    let one: String = sanitize_text(s).chars().map(|c| if c == '\n' || c == '\t' { ' ' } else { c }).collect();
    truncate_bytes(one.trim(), MESSAGE_MAX).trim_end().to_owned()
}

/// The last message of a tool's stderr with the indented lines that continue it, on one line (tools write
/// "prog: first line" and indent the rest).
pub fn last_message(s: &str) -> String {
    let mut m = String::new();
    for line in s.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with([' ', '\t']) {
            let t = line.trim_start();
            if m.is_empty() {
                m = t.to_owned();
            } else {
                m.push(' ');
                m.push_str(t);
            }
        } else {
            m = line.to_owned();
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_controls_and_tokens() {
        assert_eq!(sanitize_message("a\x1b[31mb\r\nc"), "a[31mb c");
        assert_eq!(sanitize_message("get https://host/sub?token=abc failed"), "get https://... failed");
        assert_eq!(sanitize_text("x\ty\n"), "x\ty\n");
    }

    #[test]
    fn truncates_on_char_boundary() {
        let s = "é".repeat(200);
        let m = sanitize_message(&s);
        assert!(m.len() <= MESSAGE_MAX);
        assert!(m.chars().all(|c| c == 'é'));
        assert_eq!(tail_bytes("aé", 1), "");
        assert_eq!(tail_bytes("aéb", 3), "éb");
    }

    #[test]
    fn last_message_joins_indented() {
        assert_eq!(last_message("prog: one\nprog: two\n  more\n\n"), "prog: two more");
        assert_eq!(last_message(""), "");
    }
}
